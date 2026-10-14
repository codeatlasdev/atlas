// Picture checks, pure functions of an `Image` + a `Profile`: subject edge clipping, safe areas, contrast.
//
// Edge clipping is the defect of site/public/media/film-poster.jpg and the README hero: the app window's top (its
// titlebar row) was cropped away by the scene's crop box, so the picture shows a window that starts mid-chrome. Two
// signals, either is an error:
//   1. headroom: the detected subject box sits closer to a frame edge than the profile's headroom (the subject runs
//      into or off the frame);
//   2. sliced edge: an intact window's outermost rows are a flat bezel (luminance std ~ 0 along the edge line). If the
//      outermost rows of the subject carry structure (a control, a label, a bar) the window was cut through it by a
//      crop, even when the cut leaves plenty of room to the frame edge. This is exactly the poster: 65 px of room,
//      and the controls pill of the recording sliced at the top edge (std 70-180 there, ~2 at the bottom edge).

import { bleedingSides, luminance, meanRgb, subjectBox, subjectMask, type Image } from "./pixels.ts";
import type { Diagnostic, Profile, Rect } from "./types.ts";

/** Luminance std (x1000) along an edge line above which structure crosses it. A flat bezel measures < 8. */
const SLICE_STD = 25;
/** Rows examined at each edge, and how far in from the corners (rounded corners are not structure). */
const EDGE_ROWS = 4;

export interface SubjectReport {
	box?: Rect;
	/** luminance std (x1000) of the outermost EDGE_ROWS rows/columns of the box, per side */
	edgeStd?: { top: number; bottom: number; left: number; right: number };
}

/** Std of luminance along a line of the box, skipping the first/last 8% (rounded corners, shadow). */
function lineStd(img: Image, from: number, to: number, at: (t: number) => [number, number]): number {
	const vals: number[] = [];
	for (let t = from; t < to; t++) {
		const [x, y] = at(t);
		const i = (y * img.w + x) * 3;
		vals.push(luminance(img.data[i]!, img.data[i + 1]!, img.data[i + 2]!));
	}
	const mean = vals.reduce((a, b) => a + b, 0) / vals.length;
	return Math.sqrt(vals.reduce((a, b) => a + (b - mean) ** 2, 0) / vals.length) * 1000;
}

export function analyzeSubject(img: Image): SubjectReport {
	const mask = subjectMask(img);
	const box = subjectBox(img, mask);
	if (!box) return {};
	return analyzeEdges(img, box);
}

function analyzeEdges(img: Image, box: Rect): SubjectReport {
	const worst = (n: number, line: (k: number) => number) => Math.max(...Array.from({ length: n }, (_, k) => line(k)));
	const x0 = box.x + Math.round(box.w * 0.08);
	const x1 = box.x + box.w - Math.round(box.w * 0.08);
	const y0 = box.y + Math.round(box.h * 0.08);
	const y1 = box.y + box.h - Math.round(box.h * 0.08);
	return {
		box,
		edgeStd: {
			top: worst(EDGE_ROWS, (k) => lineStd(img, x0, x1, (t) => [t, box.y + k + 1])),
			bottom: worst(EDGE_ROWS, (k) => lineStd(img, x0, x1, (t) => [t, box.y + box.h - 2 - k])),
			left: worst(EDGE_ROWS, (k) => lineStd(img, y0, y1, (t) => [box.x + k + 1, t])),
			right: worst(EDGE_ROWS, (k) => lineStd(img, y0, y1, (t) => [box.x + box.w - 2 - k, t])),
		},
	};
}

/**
 * True when the frame has no field around anything, in either of two ways:
 * - the four corners and the four mid-edges are one flat colour (a full-bleed terminal recording, whose border is the
 *   terminal's own background); or
 * - all four corners are near-black (< 0.01 relative luminance): the corners of a window-only frame are the window's
 *   own bezel (0.0003-0.0046 measured: the top pair is lifted by the toolbar highlight, and a VP9 encode of the same
 *   frame lands a few thousandths away from the H.264 one). A lit field (the cobalt backgrounds) is never that dark at
 *   all four corners, and the real cropped-poster defects measure 0.036 on their brightest corner.
 * A framed subject always sits on a field; with none, any "subject" found is a block of content (log lines, a sidebar)
 * and has no headroom to judge.
 */
function isFieldless(img: Image): boolean {
	const p = 6;
	const corners: Rect[] = [
		{ x: 0, y: 0, w: p, h: p },
		{ x: img.w - p, y: 0, w: p, h: p },
		{ x: 0, y: img.h - p, w: p, h: p },
		{ x: img.w - p, y: img.h - p, w: p, h: p },
	];
	const edges: Rect[] = [
		{ x: Math.round(img.w / 2) - p, y: 2, w: p, h: p },
		{ x: Math.round(img.w / 2) - p, y: img.h - p - 2, w: p, h: p },
		{ x: 2, y: Math.round(img.h / 2) - p, w: p, h: p },
		{ x: img.w - p - 2, y: Math.round(img.h / 2) - p, w: p, h: p },
	];
	const corner = corners.map((r) => luminance(...meanRgb(img, r)));
	if (Math.max(...corner) < 0.01) return true;
	const lum = [...corner, ...edges.map((r) => luminance(...meanRgb(img, r)))];
	return Math.max(...lum) - Math.min(...lum) < 0.004;
}

/**
 * True when the box's outermost row on `side` is EMPTY at one end (< 15% of the mask) against a filled middle: a
 * window's rounded corner. One end is enough: the field is a gradient and the mask against it is lopsided (measured on
 * the real frame: left corner 0.00, right 0.83), while a window whose bottom was cropped is cut square and FULL at
 * both ends (1.00 / 1.00).
 */
function hasRoundedCorners(img: Image, box: Rect, side: "top" | "bottom"): boolean {
	const mask = subjectMask(img);
	const y = side === "bottom" ? box.y + box.h - 1 : box.y;
	const cw = Math.max(6, Math.round(box.w * 0.012));
	const fill = (x0: number, x1: number) => {
		let c = 0;
		for (let x = x0; x < x1; x++) c += mask[y * img.w + x]!;
		return c / (x1 - x0);
	};
	const left = fill(box.x, box.x + cw);
	const right = fill(box.x + box.w - cw, box.x + box.w);
	const mid = fill(box.x + Math.round(box.w * 0.4), box.x + Math.round(box.w * 0.6));
	return (left < 0.15 || right < 0.15) && mid > 0.4;
}

/** Share of the middle 60% of a frame edge (4 px in) that the subject mask marks as subject. */
function edgeSubjectShare(mask: Uint8Array, w: number, h: number, side: "top" | "bottom" | "left" | "right"): number {
	const n = 64;
	let hit = 0;
	for (let i = 0; i < n; i++) {
		const t = 0.2 + (i / (n - 1)) * 0.6;
		const x = side === "left" ? 4 : side === "right" ? w - 5 : Math.round(t * (w - 1));
		const y = side === "top" ? 4 : side === "bottom" ? h - 5 : Math.round(t * (h - 1));
		hit += mask[y * w + x]!;
	}
	return hit / n;
}

/** True when both outer columns of the frame (3 px in, between the corners) are a flat line: a window's own bezel. */
function sideBezelFlat(img: Image): boolean {
	const y0 = Math.round(img.h * 0.08);
	const y1 = img.h - y0;
	return [3, img.w - 4].every((x) => lineStd(img, y0, y1, (t) => [x, t]) < SLICE_STD / 3);
}

export function checkEdgeClipping(img: Image, profile: Profile): Diagnostic[] {
	const { box, edgeStd } = analyzeSubject(img);
	if (!box || !edgeStd) {
		// the subject fills a whole axis, so the border-strip background model had nothing to compare against. When it
		// runs off 3+ sides the picture IS the content (a window-only screenshot): there is no frame to judge.
		const bleeding = bleedingSides(img);
		if (bleeding.length >= 3) return [];
		// A whole window that fills the frame (a loop frame): top and bottom bleed, but its left and right edge columns are a
		// flat bezel (luminance std ~ 0; measured 0-2 on every window-only fixture). On the dark Atlas look the side bezel
		// sits within `bleedingSides`' threshold of the corner colour, so the all-sides rule above no longer sees it.
		if (bleeding.length === 2 && bleeding.includes("top") && bleeding.includes("bottom") && sideBezelFlat(img)) return [];
		// A cut subject is subject-coloured along the edge it runs off, so the subject mask sees it there (1.00 on every
		// real case measured). A textured background (the aurora behind a title card, whose band crosses an edge) fools
		// the corner-blend model but is not subject to the mask (0.00): that edge is field, not content. The exception is
		// a subject spanning the axis: both opposite edges bleed, and the mask's own model is blind to exactly that.
		const mask = subjectMask(img);
		const opposite = { top: "bottom", bottom: "top", left: "right", right: "left" } as const;
		return bleeding
			.filter((side) => bleeding.includes(opposite[side]) || edgeSubjectShare(mask, img.w, img.h, side) >= 0.5)
			.map((side) => ({
				severity: "error" as const,
				code: "frame/edge-clipped",
				message: `content runs off the ${side} edge of the frame (it has no headroom there)`,
				fix: { action: "add-headroom" as const, description: `reframe so the subject has >= ${Math.ceil(profile.headroom * Math.min(img.w, img.h))}px on the ${side}`, params: { side } },
			}));
	}
	// A subject that covers (nearly) the frame along BOTH axes is not a framed object: the picture is the content itself
	// (a window-only screenshot, a full-bleed recording). One that is wide but short (a 1040 px terminal in a 1080 px
	// frame) is framed, and its thin margin is exactly what this check is for. A subject that touches both the top and
	// the bottom (or both sides) is also the picture: there is no field above or below to give headroom into.
	const coversBoth = box.w >= img.w * 0.9 && box.h >= img.h * 0.9;
	const spansY = box.y <= 1 && box.y + box.h >= img.h - 1;
	const spansX = box.x <= 1 && box.x + box.w >= img.w - 1;
	// a thin row of text (451x8) is not a window: the subject must be a real fraction of the frame on both axes
	const tiny = box.w < img.w * 0.15 || box.h < img.h * 0.15;
	if (coversBoth || spansY || spansX || tiny || isFieldless(img)) return [];
	const out: Diagnostic[] = [];
	const short = Math.min(img.w, img.h);
	const need = Math.ceil(profile.headroom * short);
	const sides: Array<["top" | "bottom" | "left" | "right", number]> = [
		["top", box.y],
		["bottom", img.h - (box.y + box.h)],
		["left", box.x],
		["right", img.w - (box.x + box.w)],
	];
	for (const [side, room] of sides) {
		if (room >= need) continue;
		out.push({
			severity: "error",
			code: "frame/edge-clipped",
			message: `the subject is ${room}px from the ${side} edge (needs >= ${need}px, ${(profile.headroom * 100).toFixed(0)}% of the short side): it touches or runs off the frame`,
			rect: box,
			measured: room,
			limit: need,
			fix: { action: "add-headroom", description: `reframe so the whole subject has >= ${need}px on the ${side} (scale the subject down or move it away from the ${side} edge)`, params: { side, add: need - room } },
		});
	}

	// sliced edge: structure crosses the outermost rows of the subject
	// Only the top and bottom edges: a window's chrome there is a flat bezel, while content (terminal text, panels)
	// legitimately runs up to its left and right edges.
	for (const side of ["top", "bottom"] as const) {
		if (edgeStd[side] < SLICE_STD) continue;
		if (out.some((d) => d.code === "frame/edge-clipped" && d.fix?.params?.side === side)) continue;
		// Content running down to a window's last row (a scrolled panel) is not a crop: a whole window ends in ROUNDED
		// corners (the mask is empty at the outermost pixels of the corner, measured 0.00 on real windows), a cropped one
		// is cut square (full there). Only the bottom: the top is where the titlebar defect lives.
		if (side === "bottom" && hasRoundedCorners(img, box, "bottom")) continue;
		const strip = Math.max(8, Math.round((side === "top" || side === "bottom" ? box.h : box.w) * 0.05));
		const rect: Rect = side === "top" ? { x: box.x, y: box.y, w: box.w, h: strip } : side === "bottom" ? { x: box.x, y: box.y + box.h - strip, w: box.w, h: strip } : side === "left" ? { x: box.x, y: box.y, w: strip, h: box.h } : { x: box.x + box.w - strip, y: box.y, w: strip, h: box.h };
		out.push({
			severity: "error",
			code: "frame/window-cropped",
			message: `the ${side} of the subject is sliced: structure (a control, label or bar; edge contrast ${edgeStd[side].toFixed(0)}, a clean window edge is < ${SLICE_STD}) crosses its ${side} edge, so the window's ${side === "top" ? "titlebar" : side === "bottom" ? "footer" : "side"} was cut off by a crop`,
			rect,
			measured: +edgeStd[side].toFixed(1),
			limit: SLICE_STD,
			fix: { action: "reframe", description: `stop cropping the ${side} of the source recording; include the full window (titlebar, rounded corners, shadow) with headroom`, params: { side } },
		});
	}
	return out;
}

/** Fraction of the frame that a platform UI covers: `safe` is per-side, in fractions of that axis. */
export function safeRect(img: Pick<Image, "w" | "h">, safe: NonNullable<Profile["safe"]>): Rect {
	const x = Math.round(img.w * safe.left);
	const y = Math.round(img.h * safe.top);
	return { x, y, w: img.w - x - Math.round(img.w * safe.right), h: img.h - y - Math.round(img.h * safe.bottom) };
}

/** Pixels of a rect outside a region, per side (0 = inside on that side). */
export interface Overflow {
	top: number;
	bottom: number;
	left: number;
	right: number;
}

/** Anything of `rect` outside `safe`, per side, in pixels. */
export function overflow(rect: Rect, safe: Rect): Overflow {
	return {
		top: Math.max(0, safe.y - rect.y),
		bottom: Math.max(0, rect.y + rect.h - (safe.y + safe.h)),
		left: Math.max(0, safe.x - rect.x),
		right: Math.max(0, rect.x + rect.w - (safe.x + safe.w)),
	};
}

/** Text boxes (from OCR) that enter the platform's UI zones. The subject itself may fill the frame; text may not. */
export function checkSafeArea(img: Image, profile: Profile, textBoxes: Array<{ text: string; rect: Rect }>): Diagnostic[] {
	if (!profile.safe) return [];
	const safe = safeRect(img, profile.safe);
	const out: Diagnostic[] = [];
	for (const t of textBoxes) {
		const o = overflow(t.rect, safe);
		const sides = (Object.entries(o) as Array<[string, number]>).filter(([, v]) => v > 0);
		if (sides.length === 0) continue;
		out.push({
			severity: "error",
			code: "safe-area/text-overlaps-ui",
			message: `text "${t.text}" enters the ${profile.id} UI zone (${sides.map(([k, v]) => `${k} by ${v}px`).join(", ")}); platform overlays will cover it`,
			rect: t.rect,
			fix: { action: "move-out-of-safe-zone", description: `keep text inside x ${safe.x}-${safe.x + safe.w}, y ${safe.y}-${safe.y + safe.h}`, params: { x: safe.x, y: safe.y, w: safe.w, h: safe.h } },
		});
	}
	return out;
}
