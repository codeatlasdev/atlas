/**
 * The native-view core. A native view (the GPU terminal, a browser tab, any future kind) is an `NSView` BELOW the
 * webview that a DOM slot stands in for: the page is transparent exactly over the view (`ground.ts`) and paints
 * everything else, overlays included, above it. Everything those views have in common lives here, once; a kind is only
 * an ADAPTER: how to open, place and release its own native view.
 *
 *     const surface = createNativeSurface({
 *       kind: "player", id: tab.id,
 *       slot: () => area,                 // the DOM element the view stands in for
 *       visible: () => active,            // does the owner want it on screen?
 *       open: (rect, shown) => invoke("player_open", { tab: id, ...rect, visible: shown }),
 *       place: (rect, shown) => invoke("player_bounds", { tab: id, ...rect, visible: shown }),
 *       release: () => invoke("player_close", { tab: id }),   // close it, or park it if the tab may be re-parented
 *     });
 *     onMount(() => { const off = watchSlot(area, surface.sync); return () => { off(); void surface.dispose(); }; });
 *
 * and on the Rust side `natives::register("player", tab, view_ptr, interactive)` / `unregister`, and the view put
 * below the page (`natives::lower_below_page`). For that the kind gets, with no code of its own:
 *
 *  - the serialized open → place → release chain (observers fire concurrently; a release outside the chain used to
 *    land before an in-flight open and leave an owner-less view on screen over every other space), coalesced, measured
 *    when each call RUNS, never a stale rect; nothing opens or shows after `dispose`;
 *  - the hole (`ground.ts`): the page's grounds are carved at the view's box, in the same task as every sync, closed
 *    at once when the view is hidden or parked, and opened only once the view has been placed shown;
 *  - the registry the reconciler (`reconcileNatives`) walks, so a view nobody owns is taken off the screen;
 *  - the loading strip: `surface.loading.start()/progress()/finish()/fail()` (`loading.ts`) draw ONE 2 px indicator
 *    above the view (`indicator.ts`); the native rect always starts `STRIP` px below the slot's top so the bar can
 *    never be covered by, or bleed into, the native view. A slow `open` is a load too.
 *
 * Overlays (menus, dialogs, a modal's scrim) are not the core's business at all: they are DOM above the view. Their
 * input is the hit map's (`hit.ts`). Only a tab swipe still parks the view (`data-swiping`).
 */

import { nativeRect } from "#lib/zoom.ts";
import { dropHole, setHole } from "./ground.ts";
import { createIndicator, type Indicator, STRIP } from "./indicator.ts";
import { createLoading, type Loading, type Timers } from "./loading.ts";

/** A rect in window points (what an NSView frame takes). */
export interface NativeRect {
	x: number;
	y: number;
	w: number;
	h: number;
}

export interface NativeAdapter {
	/** The kind's name; the same string `natives::register` takes on the Rust side. */
	kind: string;
	/** The tab id (or any id unique within the kind). */
	id: string;
	/** The DOM element the view stands in for; none while it is not laid out. */
	slot: () => HTMLElement | undefined;
	/** Does the owner want the view on screen (active tab, current space)? A swipe is the core's business. */
	visible: () => boolean;
	/** Also hidden while true (a failed page draws its own screen instead). */
	extraHidden?: () => boolean;
	/** The view's box inside the slot in CSS px (a terminal's padding); the slot's own box when omitted. */
	rect?: (slot: DOMRect) => { x: number; y: number; w: number; h: number };
	/** May the view be created now? `shown` is whether it would be on screen. Default: only once it is to be shown (a
	 *  hidden view would flash over the UI); a browser opens parked, but only once it has a URL. */
	canOpen?: (shown: boolean) => boolean;
	/** Create the native view at `rect`. `shown` is false when it must start parked. */
	open: (rect: NativeRect, shown: boolean) => Promise<unknown>;
	/** Move/resize it, or park it (`shown: false`). */
	place: (rect: NativeRect, shown: boolean) => Promise<unknown>;
	/** The owner is going away: close the view, or park it when the tab may only be re-parented. */
	release: () => Promise<unknown>;
	/** Is this error "the native view is gone" (its render thread ended)? The core opens it again over the same id. */
	gone?: (e: unknown) => boolean;
	/** An open or place failed. Return `"stop"` to end the surface for good; otherwise the next sync retries. */
	onError?: (e: unknown) => "stop" | void;
	/** Kind-specific "reload", reachable through `reloadNativeSurface`. */
	reload?: () => Promise<unknown>;
	/** No load event for `stallMs` (default 15 s): the load is over and never finished. */
	onStall?: () => void;
	stallMs?: number;
}

export interface NativeSurface {
	/** Carve the hole now, then queue a sync (open if needed, place). Coalesces: at most one run waits behind the running one. */
	sync: () => Promise<void>;
	/** The owner is going away: release the view AFTER whatever is queued, and let nothing open or show it again. */
	dispose: () => Promise<void>;
	/** Resolves when everything queued so far has run. */
	settled: () => Promise<void>;
	/** The native view exists (this surface opened or re-adopted it). */
	isOpen: () => boolean;
	/** The loading state of this surface; adapters report into it. */
	loading: Loading;
	/** The adapter's own reload, a no-op before the view is open. */
	reload: () => Promise<void>;
}

export interface SurfaceOptions {
	/** `setTimeout`/`clearTimeout` for the loading state; injected by the tests. */
	timers?: Timers;
}

const key = (kind: string, id: string) => `${kind}\u0000${id}`;
const surfaces = new Map<string, NativeSurface & { kind: string; id: string }>();

/** Every mounted surface, by kind and id. */
export const nativeSurfaces = {
	get: (kind: string, id: string) => surfaces.get(key(kind, id)),
	ofKind(kind: string): ReadonlyMap<string, NativeSurface> {
		const out = new Map<string, NativeSurface>();
		for (const s of surfaces.values()) if (s.kind === kind) out.set(s.id, s);
		return out;
	},
};

export const hasNativeSurface = (kind: string, id: string | null | undefined) =>
	!!id && surfaces.has(key(kind, id));

/** The adapter's reload for `id`; resolves `false` when there is no such surface or its kind has no reload. */
export async function reloadNativeSurface(kind: string, id: string | null | undefined): Promise<boolean> {
	const s = id ? surfaces.get(key(kind, id)) : undefined;
	if (!s) return false;
	await s.reload();
	return true;
}

export function createNativeSurface(a: NativeAdapter, opts: SurfaceOptions = {}): NativeSurface {
	let chain: Promise<void> = Promise.resolve();
	let queued = false;
	let disposed = false;
	let stopped = false;
	let opened = false;
	/** The native view has been placed SHOWN and nothing has hidden it since: only then is the page see-through over it. */
	let landed = false;

	/** Created with the first measure that finds a slot, and moved if the slot element is replaced. */
	let bar: Indicator | undefined;
	const loading = createLoading({
		timers: opts.timers,
		stallMs: a.stallMs,
		onChange: (s) => bar?.set(s.shown),
		onStall: () => a.onStall?.(),
	});

	/** Everything the adapter's calls need, measured NOW. */
	function measure() {
		const slot = a.slot();
		if (!slot) return null;
		// The indicator is a child of the slot; if the slot element changed (a remount under the same id), follow it.
		if (bar?.host !== slot) {
			bar?.remove();
			bar = createIndicator(slot);
			bar.set(loading.state().shown);
		}
		const r = slot.getBoundingClientRect();
		const box = a.rect ? a.rect(r) : { x: r.left, y: r.top, w: r.width, h: r.height };
		// The strip above the view belongs to the loading indicator, always: the layout never jumps when a load starts.
		const view = { x: box.x, y: box.y + STRIP, w: box.w, h: Math.max(0, box.h - STRIP) };
		bar.place(box.y - r.top);
		// A native view cannot follow a CSS transform and nothing crosses IPC per frame: a tab swipe parks the view.
		const parked = !!slot.closest("[data-swiping]");
		const show = a.visible() && !a.extraHidden?.() && r.width > 0 && r.height > 0 && !parked;
		return {
			slot,
			rect: nativeRect(view),
			show,
			/** The view's box in viewport CSS px: what the page's grounds are carved at. */
			box: { left: view.x, top: view.y, right: view.x + view.w, bottom: view.y + view.h },
		};
	}

	/** Carve (or restore) the page under the slot for what is true now: see-through only over a view that has landed. */
	function carve(m: NonNullable<ReturnType<typeof measure>>) {
		setHole(m.slot, landed && m.show ? m.box : null);
	}

	async function openIt(m: NonNullable<ReturnType<typeof measure>>) {
		opened = true;
		loading.start("open");
		try {
			await a.open(m.rect, m.show);
		} catch (e) {
			opened = false;
			throw e;
		} finally {
			loading.finish("open");
		}
	}

	function fail(e: unknown) {
		if (a.onError) {
			if (a.onError(e) === "stop") stopped = true;
		} else console.error(`[native] ${a.kind} ${a.id}:`, e);
	}

	async function apply() {
		queued = false;
		if (disposed || stopped) return;
		try {
			let m = measure();
			if (!m) return;
			if (!opened) {
				if (!(a.canOpen?.(m.show) ?? m.show)) return;
				await openIt(m);
				// Layout may have moved while the open was in flight: place with what is true now.
				if (disposed) return;
				m = measure() ?? m;
			}
			try {
				await place(m);
			} catch (e) {
				if (!a.gone?.(e)) throw e;
				// The native view is gone (its render thread ended): the page paints its slot until the new one lands.
				opened = false;
				landed = false;
				const gone = measure();
				if (gone) carve(gone);
				if (!m.show) return;
				await openIt(m);
				await place(m);
			}
		} catch (e) {
			fail(e);
		}
	}

	/** Place the view; once it has landed shown, open the hole with what is true NOW. */
	async function place(m: NonNullable<ReturnType<typeof measure>>) {
		if (!m.show) {
			landed = false;
			carve(m);
		}
		await a.place(m.rect, m.show);
		if (disposed) return;
		landed = m.show;
		const now = measure();
		if (now) carve(now);
	}

	function sync() {
		if (disposed) return chain;
		// The page is carved in THIS task (a resize observer, a Svelte effect), never a frame later; the IPC follows. A
		// layout read that throws (a detached slot) is reported like any other failure, never thrown at the caller.
		try {
			const m = measure();
			if (m) {
				if (!m.show) landed = false;
				carve(m);
			}
		} catch (e) {
			fail(e);
		}
		// A run is already waiting and will measure fresh when it starts: no need for another.
		if (!queued) {
			queued = true;
			chain = chain.then(apply).catch(() => {});
		}
		return chain;
	}

	function dispose() {
		if (disposed) return chain;
		disposed = true;
		if (surfaces.get(key(a.kind, a.id)) === self) surfaces.delete(key(a.kind, a.id));
		loading.dispose();
		const slot = a.slot();
		if (slot) dropHole(slot);
		// After whatever is queued (an in-flight open included): a release outside the chain landed before the open.
		chain = chain
			.then(() => a.release())
			.then(
				() => {},
				() => {},
			);
		void chain.then(() => bar?.remove());
		return chain;
	}

	const self: NativeSurface & { kind: string; id: string } = {
		kind: a.kind,
		id: a.id,
		sync,
		dispose,
		settled: () => chain,
		isOpen: () => opened,
		loading,
		async reload() {
			if (opened) await a.reload?.();
		},
	};
	surfaces.set(key(a.kind, a.id), self);
	return self;
}

// ── Kinds and the reconciler.

/** How the reconciler sees one kind of native view. */
export interface NativeKind {
	kind: string;
	/** Ids of every live native view of this kind (the Rust side's list). May reject on a platform without the kind. */
	list: () => Promise<string[]>;
	/** A live view nobody mounted owns: close it (a terminal: its pty lives on in hostd) or park it (a browser: the page survives). */
	orphan: (id: string) => Promise<unknown>;
}

const kinds = new Map<string, NativeKind>();

/** Register a kind with the reconciler; returns the unregister. Built-in kinds register in `kinds.ts`. */
export function registerNativeKind(k: NativeKind): () => void {
	kinds.set(k.kind, k);
	return () => {
		if (kinds.get(k.kind) === k) kinds.delete(k.kind);
	};
}

export interface ReconcileResult {
	/** The orphans each kind handled. */
	handled: Record<string, string[]>;
}

/**
 * A native view nobody owns stays on screen over whatever the page shows next (ghost frames,
 * docs/postmortems/ghost-frames.md). Each surface keeps its own view in step; this is the backstop that holds whatever
 * the cause: every kind's orphans go to its handler, and every owned surface re-syncs so it reflects its owner's
 * visibility right now. Run after every space commit and gesture rest, and when the window comes back.
 *
 * `ready: false` (the workspace has not loaded and mounted its panes) does nothing: nobody owns anything yet, and
 * closing then would kill every live view the page is about to re-adopt.
 */
export async function reconcileNatives(o: { ready?: boolean } = {}): Promise<ReconcileResult> {
	const handled: Record<string, string[]> = {};
	if (o.ready === false) return { handled };
	await Promise.all(
		[...kinds.values()].map(async (k) => {
			const live = await k.list().catch(() => null);
			if (!live) return;
			const owned = nativeSurfaces.ofKind(k.kind);
			const orphans = live.filter((id) => !owned.has(id));
			if (!orphans.length) return;
			handled[k.kind] = orphans;
			await Promise.all(orphans.map((id) => k.orphan(id).catch(() => {})));
		}),
	);
	for (const s of surfaces.values()) void s.sync();
	return { handled };
}

let pending = 0;
/** Coalesced `reconcileNatives`, after the next paint (the components have applied the state that triggered it). */
export function reconcileSoon(o: { ready?: boolean } = {}): void {
	if (pending) return;
	pending = requestAnimationFrame(() =>
		requestAnimationFrame(() => {
			pending = 0;
			void reconcileNatives(o);
		}),
	);
}
