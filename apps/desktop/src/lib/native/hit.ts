/**
 * The hit map: what the page paints over native views and owns input in.
 *
 * Native views sit BELOW the page, so AppKit's hit test always lands on the page. The Rust router (`natives/router.rs`)
 * turns that around: over an interactive native view's slot (a browser page) the mouse is the native view's, except
 * inside the rects published here: open menus, listboxes, tooltips, dialogs, the edge peek, anything marked
 * `data-native-cover`, and a modal's whole window (its scrim takes the pointer everywhere). Nothing here changes what
 * is DRAWN: overlays are ordinary DOM above the native view, shadows and blur included.
 *
 * Measured only when an overlay opens, closes or is placed (never per frame), and sent through `natives_hit` only
 * when the set changed. The page's message precedes the first frame showing the new state (measured, 30/30), which is
 * the order a click needs: it can never be routed against something the user cannot see yet.
 */

import { nativeRect } from "#lib/zoom.ts";
import { layoutBox, type Box, type Rect } from "./geometry.ts";

/** Kit dialogs, menus, listboxes and tooltips, and anything marked `data-native-cover` (the hook for future overlays). */
const OVERLAY = "[role=dialog], [role=menu], [role=listbox], [role=tooltip]";
const OPEN = `:is(${OVERLAY})[data-state=open], [data-native-cover]`;
const CLOSED = `:is(${OVERLAY})[data-state=closed]`;
/** Frames an overlay change keeps being re-measured for (a kit floating layer is placed a frame or two late). */
const FOLLOW_FRAMES = 3;

/** A kit floating layer sits at the top-layer origin until the layout engine writes its `translate` (async). */
function unplaced(el: Element): boolean {
	if (!el.matches(".ca-floating")) return false;
	const s = (el as HTMLElement).style;
	return !s.translate && !s.transform && s.position !== "absolute";
}

/** Is any animation (CSS, entry or exit) running on `el`? */
const animating = (el: Element) =>
	typeof el.getAnimations === "function" && el.getAnimations({ subtree: false }).length > 0;

/** The overlays over the page, in window points. */
export function overlayRects(): Rect[] {
	const boxes: Box[] = [];
	for (const d of document.querySelectorAll(OPEN)) {
		// A modal's scrim reaches the whole window even before its panel is placed.
		if (d.matches(":modal")) boxes.push({ left: 0, top: 0, right: innerWidth, bottom: innerHeight });
		if (!unplaced(d)) boxes.push(layoutBox(d));
	}
	// Closing: the exit animation is still painting the overlay over the slot.
	for (const d of document.querySelectorAll(CLOSED))
		if (animating(d) && !unplaced(d)) boxes.push(layoutBox(d));
	for (const p of document.querySelectorAll<HTMLElement>(".peek-panel[data-state=open]")) {
		const { offsetLeft: left, offsetTop: top } = p;
		boxes.push({ left, top, right: left + p.offsetWidth, bottom: top + p.offsetHeight });
	}
	const out: Rect[] = [];
	for (const b of boxes) {
		const r = nativeRect({ x: b.left, y: b.top, w: b.right - b.left, h: b.bottom - b.top });
		if (!out.some((o) => o.x === r.x && o.y === r.y && o.w === r.w && o.h === r.h)) out.push(r);
	}
	return out;
}

/** The `natives_hit` arguments: `natives::HitMap` reads `rects`, window points. */
export const hitPayload = () => ({ hit: { rects: overlayRects() } });

type Invoke = (cmd: string, args?: unknown) => Promise<unknown>;

/**
 * Follow the overlays for the life of the shell; returns the teardown. Every trigger only schedules one frame; the
 * measure runs there, after the frame's DOM writes, and an overlay change measures again for `FOLLOW_FRAMES` frames
 * (a placed floating layer, an exit animation ending). No loop at rest.
 */
export function startHitMap(invoke: Invoke): () => void {
	let raf = 0;
	let follow = 0;
	let sent = "";
	const measure = () => {
		raf = 0;
		const payload = hitPayload();
		const key = JSON.stringify(payload);
		if (key !== sent) {
			sent = key;
			void invoke("natives_hit", payload).catch(() => {});
		}
		if (follow > 0) {
			follow--;
			schedule();
		}
	};
	const schedule = () => {
		if (!raf) raf = requestAnimationFrame(measure);
	};
	const changed = () => {
		follow = FOLLOW_FRAMES;
		schedule();
	};
	const mo = new MutationObserver(changed);
	mo.observe(document.body, {
		subtree: true,
		childList: true,
		attributes: true,
		attributeFilter: ["data-state", "data-native-cover"],
	});
	document.addEventListener("layerchange", changed);
	document.addEventListener("animationend", changed, true);
	document.addEventListener("transitionend", changed, true);
	window.addEventListener("resize", changed);
	schedule();
	return () => {
		cancelAnimationFrame(raf);
		mo.disconnect();
		document.removeEventListener("layerchange", changed);
		document.removeEventListener("animationend", changed, true);
		document.removeEventListener("transitionend", changed, true);
		window.removeEventListener("resize", changed);
	};
}
