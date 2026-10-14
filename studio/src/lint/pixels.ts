// Pixel primitives for the lint checks. Decoding goes through ffmpeg (rawvideo rgb24), so JPEG/PNG/WebP/video frames
// all work with no image library. Everything below is a pure function of an `Image`, which is what the tests build.

import { StudioError } from "../lib/paths.ts";
import { probeSize } from "./media.ts";
import type { Rect } from "./types.ts";

export interface Image {
	w: number;
	h: number;
	/** rgb24, row-major, length w*h*3 */
	data: Uint8Array;
}

/** Decode one frame (image file, or video at `at` seconds) to RGB. Optionally downscale to at most `maxW` wide. */
export async function decode(file: string, opts: { at?: number; maxW?: number } = {}): Promise<Image> {
	const src = await probeSize(file);
	const w = opts.maxW && src.w > opts.maxW ? opts.maxW : src.w;
	const h = Math.round((src.h * w) / src.w);
	const args = ["ffmpeg", "-v", "error", ...(opts.at ? ["-ss", String(opts.at)] : []), "-i", file, "-frames:v", "1", "-vf", `scale=${w}:${h}:flags=area`, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"];
	const p = Bun.spawn(args, { stdout: "pipe", stderr: "pipe" });
	const [buf, err] = await Promise.all([new Response(p.stdout as ReadableStream).arrayBuffer(), new Response(p.stderr as ReadableStream).text()]);
	if ((await p.exited) !== 0 || buf.byteLength !== w * h * 3) throw new StudioError(`cannot decode ${file}`, err.trim().split("\n").slice(-4).join("\n") || "ffmpeg returned a short frame");
	return { w, h, data: new Uint8Array(buf) };
}

/** Relative luminance (WCAG) of an sRGB triple. */
export function luminance(r: number, g: number, b: number): number {
	const lin = (c: number) => {
		const s = c / 255;
		return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
	};
	return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** WCAG 2.x contrast ratio between two relative luminances, 1..21. */
export function contrastRatio(l1: number, l2: number): number {
	const [hi, lo] = l1 > l2 ? [l1, l2] : [l2, l1];
	return (hi + 0.05) / (lo + 0.05);
}

/**
 * Mask of "not background". The background of a poster is a soft gradient (cobalt field), so the model is a per-row
 * linear blend between the left and right border strips; a pixel is subject when it differs from that blend by more
 * than `thresh` (Euclidean RGB). This is robust to vignettes and stars, and to a subject touching the top or bottom.
 * A subject flush with one side makes that side's strip subject colour, so the strips are cross-checked: when they
 * disagree by more than `thresh`, the one closer to a corner patch of the frame is the field and is used on both
 * sides (a subject spans at most one border, otherwise `bleedingSides` handles it).
 */
export function subjectMask(img: Image, thresh = 70, strip = 6): Uint8Array {
	const { w, h, data } = img;
	const m = new Uint8Array(w * h);
	const edge = (y: number, x0: number, x1: number): [number, number, number] => {
		const s = [0, 0, 0];
		for (let x = x0; x < x1; x++) for (let c = 0; c < 3; c++) s[c]! += data[(y * w + x) * 3 + c]!;
		return [s[0]! / (x1 - x0), s[1]! / (x1 - x0), s[2]! / (x1 - x0)];
	};
	const dist = (a: number[], b: number[]) => Math.hypot(a[0]! - b[0]!, a[1]! - b[1]!, a[2]! - b[2]!);
	// A subject flush with one side makes that border strip subject-coloured over the subject's rows. Over a real field
	// the two strips disagree in scattered short runs (stars, light streaks); next to a flush subject they disagree in
	// ONE long contiguous run (>= half the frame height). Inside such a run the strip that stays flat is the subject, and
	// the other strip is mirrored over it so the model stays a field blend.
	const left = Array.from({ length: h }, (_, y) => edge(y, 0, strip));
	const right = Array.from({ length: h }, (_, y) => edge(y, w - strip, w));
	const apart = left.map((l, y) => dist(l, right[y]!) > thresh);
	let runStart = -1;
	let best: [number, number] = [0, -1];
	for (let y = 0; y <= h; y++) {
		if (y < h && apart[y]) {
			if (runStart < 0) runStart = y;
		} else if (runStart >= 0) {
			if (y - runStart > best[1] - best[0] + 1) best = [runStart, y - 1];
			runStart = -1;
		}
	}
	const flush = best[1] - best[0] + 1 >= h * 0.5;
	let subjectOn: "l" | "r" | undefined;
	if (flush) {
		// just outside the run both strips are field: the strip that JUMPS when the run starts is the subject's
		const outside = best[0] > 0 ? best[0] - 1 : Math.min(h - 1, best[1] + 1);
		const inside = best[0] > 0 ? best[0] : best[1];
		subjectOn = dist(left[inside]!, left[outside]!) > dist(right[inside]!, right[outside]!) ? "l" : "r";
	}
	for (let y = 0; y < h; y++) {
		let l = left[y]!;
		let r = right[y]!;
		if (subjectOn && y >= best[0] && y <= best[1]) {
			if (subjectOn === "l") l = r;
			else r = l;
		}
		for (let x = 0; x < w; x++) {
			const t = x / (w - 1);
			let d = 0;
			for (let c = 0; c < 3; c++) {
				const diff = data[(y * w + x) * 3 + c]! - (l[c]! * (1 - t) + r[c]! * t);
				d += diff * diff;
			}
			m[y * w + x] = Math.sqrt(d) > thresh ? 1 : 0;
		}
	}
	return m;
}

/**
 * Sides of the frame that a subject runs off. The background model of `subjectMask` is built from the left/right
 * border strips, so a subject that fills the full width leaves it blind. This is the model-free signal: along a side,
 * the colour of the outermost `band` px is compared with the colour of that side's two frame corners (a rounded window
 * never fills a corner, so corners are field), blended along the side. If most of the middle 60% of the side differs
 * from that field by more than `thresh` (RGB distance), the content is cut by the frame there.
 */
export function bleedingSides(img: Image, band = 4, thresh = 30): Array<"top" | "bottom" | "left" | "right"> {
	const { w, h, data } = img;
	const px = (x: number, y: number): [number, number, number] => {
		const i = (y * w + x) * 3;
		return [data[i]!, data[i + 1]!, data[i + 2]!];
	};
	const corner = (x0: number, y0: number): [number, number, number] => meanRgb(img, { x: x0, y: y0, w: 6, h: 6 });
	const tl = corner(0, 0);
	const tr = corner(w - 6, 0);
	const bl = corner(0, h - 6);
	const br = corner(w - 6, h - 6);
	const mix = (a: [number, number, number], b: [number, number, number], t: number): [number, number, number] => [a[0] * (1 - t) + b[0] * t, a[1] * (1 - t) + b[1] * t, a[2] * (1 - t) + b[2] * t];
	const dist = (a: [number, number, number], b: [number, number, number]) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
	const n = 64;
	const along = (i: number) => 0.2 + (i / (n - 1)) * 0.6;
	const share = (at: (t: number) => { p: [number, number, number]; field: [number, number, number] }) => {
		let bad = 0;
		for (let i = 0; i < n; i++) {
			const { p, field } = at(along(i));
			if (dist(p, field) > thresh) bad++;
		}
		return bad / n;
	};
	const out: Array<"top" | "bottom" | "left" | "right"> = [];
	if (share((t) => ({ p: px(Math.round(t * (w - 1)), band), field: mix(tl, tr, t) })) > 0.8) out.push("top");
	if (share((t) => ({ p: px(Math.round(t * (w - 1)), h - 1 - band), field: mix(bl, br, t) })) > 0.8) out.push("bottom");
	if (share((t) => ({ p: px(band, Math.round(t * (h - 1))), field: mix(tl, bl, t) })) > 0.8) out.push("left");
	if (share((t) => ({ p: px(w - 1 - band, Math.round(t * (h - 1))), field: mix(tr, br, t) })) > 0.8) out.push("right");
	return out;
}

/**
 * Bounding box of the dominant subject. A fixed fraction of the whole frame would miss a subject that is small along
 * one axis (a 1040x650 terminal is 34% of a 1920 px frame), so the box is found in two passes: the first axis by
 * occupancy against the whole frame (`frac`), the second against the span the first one found. Both orders are tried
 * and the one that yields the larger, denser box wins.
 */
export function subjectBox(img: Image, mask: Uint8Array, frac = 0.4, minFrame = 0.15): Rect | undefined {
	const { w, h } = img;
	// the heaviest run of qualifying rows/columns. Runs separated by a gap of at most `gap` (a dark band, a seam: real
	// windows are not uniformly different from the field) are merged; a headline or caption farther away stays a
	// separate run and loses to the window because runs are ranked by how many rows/columns they cover.
	const span = (n: number, len: number, count: (i: number) => number, f: number) => {
		const gap = Math.max(4, Math.round(n * 0.02));
		const runs: Array<[number, number]> = [];
		let start = -1;
		let last = -1;
		for (let i = 0; i < n; i++) {
			if (count(i) / len < f) continue;
			if (start >= 0 && i - last <= gap) last = i;
			else {
				if (start >= 0) runs.push([start, last]);
				start = i;
				last = i;
			}
		}
		if (start >= 0) runs.push([start, last]);
		let best: [number, number] | undefined;
		let bestCover = 0;
		for (const [a, b] of runs) {
			let cover = 0;
			for (let i = a; i <= b; i++) if (count(i) / len >= f) cover++;
			if (cover > bestCover) {
				bestCover = cover;
				best = [a, b];
			}
		}
		if (!best) return undefined;
		// grow outwards over rounded corners and the bezel: while the next row/column is still at least 70% as occupied
		// as the current edge one. A soft shadow fades gradually and never keeps that ratio for long; the window's
		// own edge is a step, so growth stops right outside it.
		while (best[0] > 0 && count(best[0] - 1) >= count(best[0]) * 0.7 && count(best[0] - 1) / len >= f / 3) best = [best[0] - 1, best[1]];
		while (best[1] < n - 1 && count(best[1] + 1) >= count(best[1]) * 0.7 && count(best[1] + 1) / len >= f / 3) best = [best[0], best[1] + 1];
		return best;
	};
	const rowCount = (x0: number, x1: number) => (y: number) => {
		let c = 0;
		for (let x = x0; x <= x1; x++) c += mask[y * w + x]!;
		return c;
	};
	const colCount = (y0: number, y1: number) => (x: number) => {
		let c = 0;
		for (let y = y0; y <= y1; y++) c += mask[y * w + x]!;
		return c;
	};
	// pass 1 finds candidate rows/columns at a low bar (`minFrame` of the frame), pass 2 re-measures the other axis
	// inside that span at the real bar (`frac` of the span), so small subjects are found and sparse text is not
	const find = (rowsFirst: boolean) => {
		const first = rowsFirst ? span(h, w, rowCount(0, w - 1), minFrame) : span(w, h, colCount(0, h - 1), minFrame);
		if (!first) return undefined;
		const len = first[1] - first[0] + 1;
		const second = rowsFirst ? span(w, len, colCount(first[0], first[1]), frac) : span(h, len, rowCount(first[0], first[1]), frac);
		if (!second) return undefined;
		const [x0, x1, y0, y1] = rowsFirst ? [second[0], second[1], first[0], first[1]] : [first[0], first[1], second[0], second[1]];
		return { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 };
	};
	const a = find(true);
	const b = find(false);
	const big = (r: Rect | undefined) => r !== undefined && r.w >= w * minFrame && r.h >= h * minFrame;
	const best = big(a) && big(b) ? (a!.w * a!.h >= b!.w * b!.h ? a! : b!) : big(a) ? a : big(b) ? b : undefined;
	return best && mergeAligned(mask, img, best);
}

/**
 * A dark window interior can be almost the colour of the field behind it, leaving a "hole" in the mask taller than any
 * gap-merge could bridge (the published poster has one 100 px tall). Two runs of rows that share the same left and right
 * edges are one rectangle, so grow the box over every row below/above it that has subject at BOTH box edges (the bezel
 * lines), however empty the middle is.
 */
function mergeAligned(mask: Uint8Array, img: Image, box: Rect): Rect {
	const { w, h, data } = img;
	// the window's own bezel colour at its left and right edge columns, from the middle of the box (always window there)
	const mid = box.y + Math.round(box.h / 2);
	const edgeColour = (x: number): [number, number, number] => meanRgb(img, { x, y: mid - 8, w: 1, h: 16 });
	const bezel = [edgeColour(box.x + 6), edgeColour(box.x + box.w - 7)];
	const near = (x: number, y: number, ref: [number, number, number]) => {
		const i = (y * w + x) * 3;
		return Math.abs(data[i]! - ref[0]) + Math.abs(data[i + 1]! - ref[1]) + Math.abs(data[i + 2]! - ref[2]) < 24;
	};
	// a bezel column is present when the background model says "subject" there, or when the pixel is the window's own
	// bezel colour: a near-black window bottom is barely different from the field in the model, but never the same colour
	const hasEdges = (y: number) => {
		let l = 0;
		let r = 0;
		for (let k = 0; k < 6; k++) {
			const xl = Math.min(w - 1, box.x + 4 + k);
			const xr = Math.max(0, box.x + box.w - 5 - k);
			l += mask[y * w + xl]! || (near(xl, y, bezel[0]!) ? 1 : 0);
			r += mask[y * w + xr]! || (near(xr, y, bezel[1]!) ? 1 : 0);
		}
		return l >= 4 && r >= 4;
	};
	let top = box.y;
	let bottom = box.y + box.h - 1;
	// skip over holes: look ahead up to the box's own height for the next row where both bezel edges are present.
	// A bezel is a CONTINUOUS line: a run of edge rows shorter than `minRun` is a coincidence (a light streak or star
	// whose colour is near the bezel's in a dark field: runs of 1-6 rows on a 606 px window, 1.5.0-alpha.7 film) and is
	// ignored; the real edge runs measure >= 112 rows on the fixtures.
	const minRun = Math.max(8, Math.round(box.h * 0.02));
	const reach = (from: number, dir: 1 | -1) => {
		let last = from;
		for (let y = from + dir, miss = 0; y >= 0 && y < h && miss < box.h * 1.5; y += dir) {
			if (!hasEdges(y)) {
				miss++;
				continue;
			}
			let end = y;
			while (end + dir >= 0 && end + dir < h && hasEdges(end + dir)) end += dir;
			if (Math.abs(end - y) + 1 >= minRun) {
				last = end;
				miss = 0;
			} else miss += Math.abs(end - y) + 1;
			y = end;
		}
		return last;
	};
	bottom = reach(bottom, 1);
	top = reach(top, -1);
	return { x: box.x, y: top, w: box.w, h: bottom - top + 1 };
}

/** Mean RGB of a rectangle (clipped to the image). */
export function meanRgb(img: Image, r: Rect): [number, number, number] {
	let sr = 0;
	let sg = 0;
	let sb = 0;
	let n = 0;
	for (let y = Math.max(0, r.y); y < Math.min(img.h, r.y + r.h); y++)
		for (let x = Math.max(0, r.x); x < Math.min(img.w, r.x + r.w); x++) {
			const i = (y * img.w + x) * 3;
			sr += img.data[i]!;
			sg += img.data[i + 1]!;
			sb += img.data[i + 2]!;
			n++;
		}
	return n ? [sr / n, sg / n, sb / n] : [0, 0, 0];
}
