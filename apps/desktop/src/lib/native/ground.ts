/**
 * The ground: where the page is transparent so a native view shows through it.
 *
 * Native views (the GPU terminal, browser tabs, any kind) are `NSView`s BELOW the webview (`natives/`, Rust). The page
 * paints over them, so exactly over each view's slot it paints nothing: every ground that would paint there carries a
 * hole. Three grounds, all `clip-path`ed even-odd with the same holes in their own coordinates:
 *
 *  - `.window-ground`: the window's chrome colour behind everything (one, fixed, window coordinates);
 *  - `.ground`: the card colour and space tint of a space layer (one per layer, so a hole travels with its layer on the
 *    compositor and a parked neighbour's hole is right the moment it slides in);
 *  - the slot element itself: its own fill (a terminal's padding colour) paints around the view, not under it.
 *
 * Everything the page draws over a slot (menus, dialogs, a modal's scrim, shadows, blur) is ordinary DOM above the
 * native view in the same compositor commit, so there is nothing to keep in step with it: a hole only ever follows its
 * slot. `setHole` writes in the caller's task (a `ResizeObserver` callback, a Svelte effect), never in a later frame,
 * and skips an unchanged write. The engine is on only while `html.ground-holes` is set (`startGround`, macOS): on
 * Linux and Windows the grounds stay opaque and the page looks and works as before.
 *
 * Overlays never touch a hole. Only the INPUT needs to know about them: that is the hit map (`hit.ts`).
 */

import { disjoint, type Box, type Rect } from "./geometry.ts";

export { disjoint };

const CLASS = "ground-holes";
const root = () => document.documentElement;

/** Holes by slot element: the box of the native view in viewport CSS px. */
const holes = new Map<Element, Box>();
/** The last clip written to each ground, so an unchanged hole writes nothing. */
const written = new WeakMap<Element, string>();

/** The `clip-path` for `holes` (px of the ground's own box, `w` x `h`): the whole box minus each hole, or "" for none. */
export function clipPath(holes: Rect[], w: number, h: number): string {
	if (!holes.length) return "";
	const parts = [`M0 0H${w}V${h}H0Z`];
	for (const r of holes) parts.push(`M${r.x} ${r.y}H${r.x + r.w}V${r.y + r.h}H${r.x}Z`);
	return `path(evenodd, "${parts.join("")}")`;
}

/** Viewport boxes → rects relative to `at`'s top-left, clipped to its size, empty or non-finite ones dropped. */
function localHoles(at: DOMRect, boxes: Box[]): Rect[] {
	const rects: Rect[] = [];
	for (const b of boxes) {
		if (![b.left, b.top, b.right, b.bottom].every(Number.isFinite)) continue;
		const x0 = Math.max(b.left - at.left, 0);
		const y0 = Math.max(b.top - at.top, 0);
		const x1 = Math.min(b.right - at.left, at.width);
		const y1 = Math.min(b.bottom - at.top, at.height);
		if (x1 > x0 && y1 > y0) rects.push({ x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
	}
	return disjoint(rects);
}

/** Write one ground from the holes (viewport boxes) that fall inside it. */
export function carve(ground: Element, boxes: Box[]) {
	const at = ground.getBoundingClientRect();
	const clip = clipPath(localHoles(at, boxes), at.width, at.height);
	if (written.get(ground) === clip) return;
	written.set(ground, clip);
	(ground as HTMLElement).style.setProperty("clip-path", clip);
}

const ownGround = (slot: Element) => slot.closest(".space-layer")?.querySelector(":scope > .ground") ?? null;

/** Every hole whose slot lies inside `layer`. */
const inside = (layer: Element) => [...holes].filter(([slot]) => layer.contains(slot)).map(([, b]) => b);

/**
 * The native view of `slot` occupies `box` (viewport CSS px), or none (`null`: hidden, parked, not placed yet): carve
 * or restore the grounds under it, now.
 */
export function setHole(slot: Element, box: Box | null) {
	if (!root().classList.contains(CLASS)) return;
	if (box) holes.set(slot, box);
	else holes.delete(slot);
	const layer = slot.closest(".space-layer");
	const layerGround = ownGround(slot);
	const window = document.querySelector(".window-ground");
	if (window) carve(window, [...holes.values()]);
	if (layer && layerGround) carve(layerGround, inside(layer));
	carve(slot, box ? [box] : []);
}

/** A slot is gone: forget its hole and restore the grounds under where it was. */
export function dropHole(slot: Element) {
	if (holes.has(slot)) setHole(slot, null);
}

/** Holes currently carved (tests, the debug bridge). */
export const holeCount = () => holes.size;

/** Forget every hole without touching the DOM (a test's fresh page; the shell stopping). */
export function resetHoles() {
	holes.clear();
}

/**
 * Turn the engine on (macOS shell, once): the grounds become transparent-with-holes (`app.css`, `html.ground-holes`).
 * Returns the teardown. `surface` colour feeds the underlay: what a hole shows for the frame before a native view has
 * landed (a view just shown, resized or moved), so it is never the window material.
 */
export function startGround(sendGround: (rgba: [number, number, number, number]) => void): () => void {
	root().classList.add(CLASS);
	const push = () => sendGround(readSurface());
	push();
	// A theme switch rewrites the root's class/style/data attributes: the underlay follows.
	const mo = new MutationObserver(push);
	mo.observe(root(), { attributes: true, attributeFilter: ["class", "style", "data-theme", "data-style"] });
	return () => {
		mo.disconnect();
		root().classList.remove(CLASS);
		resetHoles();
	};
}

let probe: CanvasRenderingContext2D | null = null;

/** The card colour (`--ca-surface-base`) as `[r, g, b, a]` in 0..1, whatever CSS syntax the theme uses (oklch included). */
export function readSurface(): [number, number, number, number] {
	const css = getComputedStyle(root()).getPropertyValue("--ca-surface-base").trim() || "#111111";
	probe ??= document.createElement("canvas").getContext("2d", { willReadFrequently: true });
	if (!probe) return [0.07, 0.07, 0.07, 1];
	probe.clearRect(0, 0, 1, 1);
	probe.fillStyle = css;
	probe.fillRect(0, 0, 1, 1);
	const [r, g, b, a] = probe.getImageData(0, 0, 1, 1).data;
	return [r! / 255, g! / 255, b! / 255, a! / 255];
}
