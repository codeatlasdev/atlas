/**
 * Cuts the presented terminals' rects out of the page's backdrop layers (`.ground`, `.card`; app.css, holes.ts).
 *
 * Each open hole is registered by its slot with a function giving the slot's rect in CSS px, in the page's
 * viewport. The mask of each layer is that layer's own coordinate system (`.card` sits inside `.content`), so the
 * rects are shifted by the layer's origin. A write happens only when the set of rects changed since the last one
 * (`sameHoles`), in the frame of the layout batch that answered: never per frame, never on hover or on an overlay.
 */

import { maskStyle, sameHoles, type Hole } from "./holes.ts";

type Rect = { left: number; top: number; width: number; height: number };

const open = new Map<string, () => Rect | null>();
const last = new Map<Element, Hole[]>();
let raf = 0;

function apply() {
	raf = 0;
	const rects = [...open.values()].map((f) => f()).filter((r): r is Rect => !!r);
	// The page is opaque unless a terminal really shows through it (`html.term-hole`, app.css).
	document.documentElement.classList.toggle("term-hole", rects.length > 0);
	for (const el of document.querySelectorAll(".ground, .card")) {
		const o = el.getBoundingClientRect();
		const holes = rects.map((r) => ({ x: r.left - o.left, y: r.top - o.top, w: r.width, h: r.height }));
		const before = last.get(el);
		if (before && sameHoles(before, holes)) continue;
		last.set(el, holes);
		const style = (el as HTMLElement).style;
		for (const [k, v] of Object.entries(maskStyle(holes))) style.setProperty(k, v);
	}
}

/** Re-cut the layers in the next frame (coalesced). */
export function recut() {
	if (!raf) raf = requestAnimationFrame(apply);
}

/**
 * A slot's native view is on screen under it (`rect` is where, in CSS px): open its hole.
 *
 * The cut is written NOW, in the same task as the slot turning transparent (Svelte's effect runs before the frame):
 * a deferred write would let one frame show the slot transparent under a still-opaque card, or the card cut over a
 * slot already painting again. Layout answers that only move a rect still coalesce into one write per frame (`recut`).
 */
export function openHole(tab: string, rect: () => Rect | null) {
	open.set(tab, rect);
	flushNow();
}

/** The view is gone or hidden: close the hole so the page paints that rect again (same task as the slot painting). */
export function closeHole(tab: string) {
	if (open.delete(tab)) flushNow();
}

function flushNow() {
	if (raf) cancelAnimationFrame(raf);
	apply();
}

/** Test seam: forget every hole and cached mask. */
export function resetHoles() {
	open.clear();
	last.clear();
	document.documentElement.classList.remove("term-hole");
	if (raf) cancelAnimationFrame(raf);
	raf = 0;
}
