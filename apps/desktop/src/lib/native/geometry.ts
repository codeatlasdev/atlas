/** Box maths shared by the ground (holes) and the hit map. All values are CSS px unless a name says otherwise. */

export type Box = Pick<DOMRect, "left" | "top" | "right" | "bottom">;

export interface Rect {
	x: number;
	y: number;
	w: number;
	h: number;
}

/** `1.5px 2px` as px numbers (computed `transform-origin` is always resolved to px). */
function pair(css: string): [number, number] {
	const [a, b] = css.split(/\s+/).map((v) => Number.parseFloat(v));
	return [Number.isFinite(a!) ? a! : 0, Number.isFinite(b!) ? b! : 0];
}

/** `[sx, sy, tx, ty]` of a computed 2D `matrix(a, b, c, d, e, f)` / `matrix3d(...)`; identity for `none` or anything else. */
function scaleShift(transform: string): [number, number, number, number] {
	const m = /^matrix(3d)?\(([^)]*)\)$/.exec(transform.trim());
	if (!m) return [1, 1, 0, 0];
	const v = m[2]!.split(",").map((n) => Number.parseFloat(n));
	const ok = (i: number) => Number.isFinite(v[i]!);
	if (m[1]) return ok(0) && ok(5) && ok(12) && ok(13) ? [v[0]!, v[5]!, v[12]!, v[13]!] : [1, 1, 0, 0];
	return ok(0) && ok(3) && ok(4) && ok(5) ? [v[0]!, v[3]!, v[4]!, v[5]!] : [1, 1, 0, 0];
}

/**
 * The box an element rests at: where it lays out (its CSS `translate` and position included) with its animated
 * `transform` taken out. A kit layer scales in from 0.95 and a dialog slides, so its bounding rect, read on the first
 * frames, is the wrong place and size. Undo the computed `transform` (scale + shift about `transform-origin`) from the
 * bounding rect instead.
 */
export function layoutBox(el: Element): Box {
	const r = el.getBoundingClientRect();
	const cs = getComputedStyle(el);
	const [sx, sy, tx, ty] = scaleShift(cs.transform || "none");
	if (sx === 1 && sy === 1 && tx === 0 && ty === 0) return r;
	const [ox, oy] = pair(cs.transformOrigin || "0 0");
	const html = el as HTMLElement;
	// The layout size is transform-free; a fully collapsed scale(0) has no usable bounding size.
	const w = html.offsetWidth > 0 ? html.offsetWidth : sx !== 0 ? r.width / sx : 0;
	const h = html.offsetHeight > 0 ? html.offsetHeight : sy !== 0 ? r.height / sy : 0;
	const left = r.left - ox * (1 - sx) - tx;
	const top = r.top - oy * (1 - sy) - ty;
	return { left, top, right: left + w, bottom: top + h };
}

/**
 * The union of `rects` as non-overlapping rects: a grid over every distinct edge, each covered cell merged into
 * horizontal runs per row band. A clip is even-odd: two overlapping holes would fill each other back in.
 */
export function disjoint(rects: Rect[]): Rect[] {
	if (rects.length < 2) return rects.slice();
	const edges = (f: (c: Rect) => [number, number]) => [...new Set(rects.flatMap(f))].sort((a, b) => a - b);
	const xs = edges((c) => [c.x, c.x + c.w]);
	const ys = edges((c) => [c.y, c.y + c.h]);
	const covered = (x0: number, x1: number, y0: number, y1: number) =>
		rects.some((c) => c.x <= x0 && c.x + c.w >= x1 && c.y <= y0 && c.y + c.h >= y1);
	const out: Rect[] = [];
	for (let j = 0; j + 1 < ys.length; j++) {
		let run: number | null = null;
		for (let i = 0; i + 1 < xs.length; i++) {
			const on = covered(xs[i]!, xs[i + 1]!, ys[j]!, ys[j + 1]!);
			if (on && run === null) run = xs[i]!;
			const last = i + 2 === xs.length;
			if (run !== null && (!on || last)) {
				const x1 = on ? xs[i + 1]! : xs[i]!;
				out.push({ x: run, y: ys[j]!, w: x1 - run, h: ys[j + 1]! - ys[j]! });
				run = null;
			}
		}
	}
	return out;
}
