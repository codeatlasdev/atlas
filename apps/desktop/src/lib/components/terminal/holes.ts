/**
 * The holes a native terminal needs in the page.
 *
 * A GPU terminal is an NSView UNDER the transparent webview, so everything the page paints over its rect hides it:
 * the window ground, the card (`.content`) and its wash. Those three are painted by ONE backdrop layer
 * (`.backdrop` in app.css) and the page cuts the terminals' rects out of that layer with a CSS mask; the slot
 * itself is transparent. Nothing else paints over a slot, so the terminal shows through, and a menu, dialog or the
 * sidebar peek (which paint above the backdrop) simply cover it.
 *
 * The mask changes only when the set of presented rects changes (a layout batch answered, a tab left the screen),
 * never per frame and never on hover.
 */

export interface Hole {
	x: number;
	y: number;
	w: number;
	h: number;
}

const px = (v: number) => `${Math.round(v * 100) / 100}px`;

/**
 * `mask-image` (and `mask-size`/`mask-position`/`mask-repeat`, via `maskStyle`) that is opaque everywhere except
 * `holes`. One full-size opaque layer per hole is subtracted with `mask-composite: exclude`: each term is a
 * `linear-gradient` of a solid colour clipped to the hole's box by `mask-size`/`mask-position`. Zero holes: no mask.
 */
export function maskStyle(holes: Hole[]): Record<string, string> {
	const hs = holes.filter((h) => [h.x, h.y, h.w, h.h].every(Number.isFinite) && h.w > 0 && h.h > 0);
	if (!hs.length) return { "mask-image": "none", "-webkit-mask-image": "none" };
	const solid = "linear-gradient(#000 0 0)";
	const layers = ["linear-gradient(#000 0 0)", ...hs.map(() => solid)];
	const sizes = ["100% 100%", ...hs.map((h) => `${px(h.w)} ${px(h.h)}`)];
	const positions = ["0 0", ...hs.map((h) => `${px(h.x)} ${px(h.y)}`)];
	const image = layers.join(", ");
	const size = sizes.join(", ");
	const position = positions.join(", ");
	// Each hole layer is excluded from the running composite, so overlapping holes cannot re-fill each other
	// (`exclude` of two opaque layers is clear; callers never pass overlapping rects: panes do not overlap).
	const composite = ["exclude", ...hs.map(() => "exclude")].join(", ");
	const webkitComposite = ["xor", ...hs.map(() => "xor")].join(", ");
	return {
		"mask-image": image,
		"-webkit-mask-image": image,
		"mask-size": size,
		"-webkit-mask-size": size,
		"mask-position": position,
		"-webkit-mask-position": position,
		"mask-repeat": "no-repeat",
		"-webkit-mask-repeat": "no-repeat",
		"mask-composite": composite,
		"-webkit-mask-composite": webkitComposite,
	};
}

/** Does `a` overlap `b`? Used to refuse a mask whose holes would fight each other. */
export const overlap = (a: Hole, b: Hole) =>
	a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;

/** Same set of rects (order independent): an unchanged set must not rewrite the mask. */
export function sameHoles(a: Hole[], b: Hole[]): boolean {
	if (a.length !== b.length) return false;
	const key = (h: Hole) => `${h.x}|${h.y}|${h.w}|${h.h}`;
	const keys = new Set(a.map(key));
	return b.every((h) => keys.has(key(h)));
}
