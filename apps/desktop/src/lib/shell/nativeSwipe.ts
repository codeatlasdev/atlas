/**
 * Frontend half of the native trackpad swipe (`crates/atlas-desktop/src/swipe_mac.rs`).
 *
 * The Rust engine (`swipe_engine.rs`) recognizes the gesture from the raw `NSEvent` stream before the webview
 * sees it and owns all motion and decisions. It reports over two events, both stamped with the gesture's `seq`:
 *  - `swipe://frame`   every engine frame: `{ zone, id?, seq, amount, stage: "track" | "settle" | "rest" }`
 *  - `swipe://commit`  once per `seq` when the fingers lift: `{ zone, id?, seq, dir }` (`dir` 0 = cancel)
 * This module pushes the zone rects the engine hit-tests (`swipe_zones`) and dispatches the events to the
 * handler registered for that zone. Handlers only write transforms; they never decide anything.
 *
 * When native tracking is unavailable (not macOS) `nativeActive()` stays false and `swipe.ts` keeps its DOM
 * wheel recognizer.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CommitMsg, FrameMsg } from "#lib/shell/pager.ts";
import { inParkedLayer } from "#lib/shell/parkedLayer.ts";
import { edgesOf, probeOf } from "#lib/shell/scrollable.ts";
import { availability, availabilityKey, trailing } from "#lib/shell/trailing.ts";
import { nativeRect } from "#lib/zoom.ts";

export type SwipeZone = "space" | "tab" | "history";

export type SwipeFrame = FrameMsg & { zone: SwipeZone };
export type SwipeCommit = CommitMsg & { zone: SwipeZone };

export interface NativeSwipe {
	zone: SwipeZone;
	id?: string;
	/** Whether a neighbour exists in that direction: bounds the tracked amount natively. */
	can: (dir: 1 | -1) => boolean;
	/** Zone-specific: only count the zone when this is true (history: nothing scrolls under the pointer). */
	pinned?: () => boolean;
	/** Every engine frame, settle and rest included. */
	onFrame: (f: SwipeFrame) => void;
	/** The release decision, once per generation. */
	onCommit: (c: SwipeCommit) => void;
}

interface Entry {
	el: HTMLElement;
	spec: NativeSwipe;
	/** `availability` at the last push: an unchanged one needs no new push. */
	avail: string;
}

let native = false;
let ready: Promise<void> | null = null;
const entries = new Set<Entry>();
let last = "";

/** Is the native recognizer driving swipes? Resolved once at startup by `initNativeSwipe`. */
export const nativeActive = () => native;

/** Decide once whether native tracking is on, and subscribe to its events. Idempotent. */
export function initNativeSwipe(): Promise<void> {
	ready ??= (async () => {
		if (!("__TAURI_INTERNALS__" in window)) return;
		try {
			native = await invoke<boolean>("swipe_native");
		} catch {
			native = false;
		}
		if (!native) return;
		await listen<SwipeFrame>("swipe://frame", (e) => {
			for (const x of entries) if (matches(x, e.payload)) x.spec.onFrame(e.payload);
		});
		await listen<SwipeCommit>("swipe://commit", (e) => {
			for (const x of entries) if (matches(x, e.payload)) x.spec.onCommit(e.payload);
		});
		watchScrollers();
		push();
	})();
	return ready;
}

const matches = (x: Entry, p: { zone: SwipeZone; id?: string }) =>
	x.spec.zone === p.zone && (x.spec.id ?? undefined) === (p.id ?? undefined);

/** Rect in CSS px clipped to the viewport, or null (hidden, `inert`, in a parked pager layer, off-screen or empty). */
function rectOf(el: HTMLElement) {
	if (el.closest("[inert]") || inParkedLayer(el)) return null;
	const r = el.getBoundingClientRect();
	const x = Math.max(0, r.x);
	const y = Math.max(0, r.y);
	const w = Math.min(innerWidth, r.right) - x;
	const h = Math.min(innerHeight, r.bottom) - y;
	return w < 1 || h < 1 ? null : { x, y, w, h };
}

/** A horizontal scroller and which way it still can (`left` reveals more of its left side, `right` of its right). */
export interface ScrollerRect {
	x: number;
	y: number;
	w: number;
	h: number;
	left: boolean;
	right: boolean;
}

/** Surfaces that scroll sideways by themselves, found by marker instead of by walking every element. */
const MARKED = ".xterm, .cm-scroller, [data-gesture='own'], .scroll-x";

/** Inactive tabs stay laid out under the active one with `visibility: hidden`: whatever they could scroll is not there. */
const shown = (el: HTMLElement) => getComputedStyle(el).visibility !== "hidden";

/**
 * Rects (CSS px, clipped to the viewport) of everything under the content that scrolls horizontally itself: the
 * terminal, editors, `data-gesture="own"` surfaces and anything that overflows in x, each with the direction it can
 * still scroll. A gesture that lands on one that can still go the way the fingers go is the content's; at its edge
 * it is the pager's (the engine reads this list next to the zones). The tab strip is not one: it has its own zone.
 */
export function scrollerRects(root: ParentNode = document): ScrollerRect[] {
	const out: ScrollerRect[] = [];
	for (const el of root.querySelectorAll<HTMLElement>(MARKED)) {
		const e = edgesOf(probeOf(el));
		if (!e.left && !e.right) continue; // an editor that wraps its lines has nothing to scroll
		const r = shown(el) ? rectOf(el) : null;
		if (r) out.push({ ...r, ...e });
	}
	// Not `main.content *` over every layer: a parked layer never yields a rect (`rectOf`), and measuring its elements
	// (`scrollWidth`, `getComputedStyle`) cost O(all mounted spaces) per switch, 480 ms with five 400-line diffs (#121).
	// The walk rejects a parked layer as a whole subtree, so none of its nodes is even visited.
	const content = root.querySelector<HTMLElement>("main.content");
	const walker = content
		? document.createTreeWalker(content, NodeFilter.SHOW_ELEMENT, {
				acceptNode: (n) =>
					(n as HTMLElement).matches(".space-layer[data-role='near']")
						? NodeFilter.FILTER_REJECT
						: NodeFilter.FILTER_ACCEPT,
			})
		: null;
	for (let n = walker?.nextNode(); n; n = walker!.nextNode()) {
		const el = n as HTMLElement;
		if (el.scrollWidth <= el.clientWidth + 1 || el.closest(".strip") || el.matches(MARKED)) continue;
		const e = edgesOf(probeOf(el));
		if (!e.left && !e.right) continue; // `overflow-x` is not auto/scroll/overlay: nothing scrolls
		const r = shown(el) ? rectOf(el) : null;
		if (r) out.push({ ...r, ...e });
	}
	return out;
}

/**
 * Push the zones now. Order matters natively (later = deeper wins): outer zones first. DOM order
 * of registration is not nesting order, so sort by area, largest first.
 */
function push() {
	if (!native) return;
	const zones = [];
	for (const { el, spec } of entries) {
		const rect = rectOf(el);
		if (!rect) continue;
		zones.push({
			zone: spec.zone,
			id: spec.id,
			// The monitor hit-tests `locationInWindow` (window points); the page measures CSS px (layout zoom).
			rect: nativeRect(rect),
			prev: spec.can(-1),
			next: spec.can(1),
			pinned: spec.pinned?.() ?? false,
		});
	}
	zones.sort((a, b) => b.rect.w * b.rect.h - a.rect.w * a.rect.h);
	const scrollers = scrollerRects().map((s) => ({ ...nativeRect(s), left: s.left, right: s.right }));
	const key = JSON.stringify([zones, scrollers]);
	toldKey = availabilityKey(entries);
	if (key === last) return;
	last = key;
	void invoke("swipe_zones", { zones, scrollers });
}

/**
 * Push the zones once the burst of changes is over. A commit re-renders the whole space and every swipe surface
 * asks for a push; measuring in that frame forced layout on fresh DOM changes (a 30-50 ms stall measured by the
 * gesture bench), and nobody can start a swipe within 120 ms of a layout change.
 */
const schedulePush = trailing(() => {
	if (native) push();
}, 120);

/** Availability of every entry at the last push: a request that changes nothing the engine is told is dropped. */
let toldKey = "";

/**
 * Ask for a push. `geometry` = the layout may have moved (resize, a pane opened): always push, measuring rects.
 * Otherwise (a selection, a re-render) only when some entry's availability changed, which costs no layout read.
 */
export function pushZones(geometry = false) {
	if (!native) return;
	if (!geometry && availabilityKey(entries) === toldKey) return;
	schedulePush();
}

/**
 * What the engine knows about a scroller is a measurement: a sideways scroll, an edit that makes a line long or the
 * wheel itself changes it. Re-measure once such a burst is over, and tell the engine even when the rects did not
 * move (a scroller the engine guessed about while it scrolled must get its real edges back).
 */
const scrollSettled = trailing(() => {
	last = "";
	if (native) push();
}, 100);

/**
 * Page side of the swipe's two guarantees. Fresh edges: scroll positions (only a `scrollLeft` change counts, so
 * vertical scrolling costs nothing), sideways wheels and edits schedule a re-measure. A gesture never outlives its
 * window: when focus is lost or the page is hidden the engine settles whatever fingers it still counts as down.
 */
function watchScrollers() {
	const seen = new WeakMap<Element, number>();
	const passive = { capture: true, passive: true } as const;
	addEventListener(
		"scroll",
		(e) => {
			const el = e.target;
			if (!(el instanceof HTMLElement) || seen.get(el) === el.scrollLeft) return;
			seen.set(el, el.scrollLeft);
			scrollSettled();
		},
		passive,
	);
	addEventListener("wheel", (e) => Math.abs(e.deltaX) > Math.abs(e.deltaY) && scrollSettled(), passive);
	addEventListener("input", scrollSettled, passive);
	const abandon = () => void invoke("swipe_abandon").catch(() => {});
	addEventListener("blur", abandon);
	document.addEventListener("visibilitychange", () => document.hidden && abandon());
}

/**
 * Register an element as a native swipe zone. Returns a cleanup. Rects are re-pushed on resize
 * of the element or the window; call `pushZones()` when neighbour availability changes.
 */
export function registerZone(
	el: HTMLElement,
	spec: NativeSwipe,
): { update(s: NativeSwipe): void; destroy(): void } {
	const onResize = () => pushZones(true);
	const entry: Entry = { el, spec, avail: availability(spec) };
	entries.add(entry);
	const ro = new ResizeObserver(() => pushZones(true));
	ro.observe(el);
	addEventListener("resize", onResize);
	pushZones(true);
	return {
		update(s) {
			entry.spec = s;
			// A re-render hands over a new options object each time: only a change in what the engine is told counts.
			const now = availability(s);
			if (now === entry.avail) return;
			entry.avail = now;
			pushZones();
		},
		destroy() {
			entries.delete(entry);
			ro.disconnect();
			removeEventListener("resize", onResize);
			pushZones(true);
		},
	};
}

/** The engine's own programmatic page change (`swipe_go`): its spring and clock. False = refused or not native. */
export function engineGo(zone: SwipeZone, dir: 1 | -1, id?: string): Promise<boolean> {
	if (!native) return Promise.resolve(false);
	return invoke<boolean>("swipe_go", { zone, id: id ?? null, dir });
}

/** What the page has registered, for the gesture bench and `debug eval`: native on, entries, last pushed key. */
export function swipeDebug() {
	return {
		native,
		entries: entries.size,
		pushed: last ? (JSON.parse(last) as unknown[][]).map((a) => a.length) : null,
	};
}
