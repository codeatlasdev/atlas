/**
 * The annotation document and its reducer. Pure and immutable: every function takes a `Doc` and returns a new one,
 * so the page keeps it in a `$state` and the tests need no DOM. Coordinates are IMAGE pixels, never CSS pixels:
 * the page converts pointer positions with `viewToImage`.
 *
 * Undo is a stack of snapshots `{items, crop}`. Edits replace `items`/`crop` instead of mutating them, so a snapshot
 * is just the two references and "unsaved changes" is reference equality against the snapshot taken at save time.
 */
import { counterRadius, markerBounds, textBox, type BlurMode } from "./render.ts";
import type { Rect } from "./types.ts";

export interface Point {
	x: number;
	y: number;
}

export type Tool =
	"select" | "arrow" | "rect" | "text" | "highlight" | "blur" | "crop" | "counter" | "spotlight" | "marker";

export type TextWeight = "regular" | "bold";
export type TextBg = "none" | "solid" | "pill";
export type SpotlightShape = "rect" | "ellipse";

/**
 * A counter stores no number: it is derived from the z-order of the live counters (`counterNumbers`), so deleting
 * #2 renumbers the following ones and undo/redo bring the old numbers back without any bookkeeping.
 */
export type Item =
	| { id: number; type: "arrow"; from: Point; to: Point; color: string; width: number }
	| { id: number; type: "rect"; rect: Rect; color: string; width: number; fill: null | "highlight" }
	| {
			id: number;
			type: "text";
			at: Point;
			text: string;
			color: string;
			size: number;
			weight: TextWeight;
			bg: TextBg;
	  }
	| { id: number; type: "highlight"; rect: Rect; color: string }
	| { id: number; type: "blur"; rect: Rect; mode: BlurMode; amount: number }
	| { id: number; type: "counter"; at: Point; color: string; width: number }
	| { id: number; type: "spotlight"; rect: Rect; shape: SpotlightShape }
	| { id: number; type: "marker"; points: Point[]; color: string; width: number };

/** The number each live counter shows: 1, 2, 3 ... in z-order. Other items consume none. */
export function counterNumbers(items: Item[]): Map<number, number> {
	const m = new Map<number, number>();
	for (const it of items) if (it.type === "counter") m.set(it.id, m.size + 1);
	return m;
}

export interface Snapshot {
	items: Item[];
	crop: Rect | null;
}

/** What the toolbar currently has selected; copied into each new item. */
export interface Style {
	color: string;
	width: number;
	size: number;
	mode: BlurMode;
	amount: number;
	weight: TextWeight;
	bg: TextBg;
	shape: SpotlightShape;
}

export const COLORS = [
	"#ef4444",
	"#f97316",
	"#facc15",
	"#22c55e",
	"#14b8a6",
	"#3b82f6",
	"#a855f7",
	"#ec4899",
	"#ffffff",
	"#111111",
];
/** Stroke widths (image px) of the toolbar; a counter's size follows the width too. */
export const WIDTHS = [2, 4, 6, 10];
/** Text size presets (image px). */
export const TEXT_SIZES = [
	{ label: "S", size: 18 },
	{ label: "M", size: 28 },
	{ label: "L", size: 44 },
	{ label: "XL", size: 72 },
];
export const DEFAULT_STYLE: Style = {
	color: COLORS[0],
	width: 4,
	size: 28,
	mode: "pixelate",
	amount: 12,
	weight: "regular",
	bg: "none",
	shape: "rect",
};

/** The style new items get: the defaults with the toolbar's choices on top. */
export const styleOf = (patch: Partial<Style>): Style => ({ ...DEFAULT_STYLE, ...patch });

/** `#rgb` / `#rrggbb` (with or without `#`, any case) as lower-case `#rrggbb`; null for anything else. */
export function normalizeHex(input: string): string | null {
	const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(input.trim());
	if (!m) return null;
	const h = m[1].toLowerCase();
	return "#" + (h.length === 3 ? [...h].map((c) => c + c).join("") : h);
}

/** True for a canonical `#rrggbb` / `#rgb` colour (what the document stores and a canvas accepts). */
export const isHex = (c: string): boolean => /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(c);

type Gesture =
	| { kind: "draw"; id: number; origin: Point; before: Snapshot }
	| { kind: "move"; id: number; last: Point; before: Snapshot; moved: boolean }
	| { kind: "crop"; origin: Point; before: Snapshot };

export interface Doc extends Snapshot {
	width: number;
	height: number;
	selected: number | null;
	past: Snapshot[];
	future: Snapshot[];
	gesture: Gesture | null;
	/** The text item whose inline input is open. `fresh`: it was created by this edit (cancelling removes it). */
	editing: { id: number; fresh: boolean } | null;
	nextId: number;
}

/** Smallest crop, in image pixels. */
export const MIN_CROP = 8;
/** A drag shorter than this is a click, not a shape. */
const MIN_DRAW = 3;
const MAX_HISTORY = 100;
/** Arrows hit within this distance of the segment. */
const HIT_ARROW = 6;
const HIT_AREA = 2;
const HIT_TEXT = 4;
/** Extra reach around a marker stroke when picking it. */
const HIT_MARKER = 3;

export function newDoc(width: number, height: number): Doc {
	return {
		width,
		height,
		crop: null,
		items: [],
		selected: null,
		past: [],
		future: [],
		gesture: null,
		editing: null,
		nextId: 1,
	};
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const snap = (d: Doc): Snapshot => ({ items: d.items, crop: d.crop });

/** `before` becomes an undo step and the redo stack is dropped: the one place an edit is recorded. */
function record(d: Doc, before: Snapshot): Doc {
	return { ...d, past: [...d.past.slice(-(MAX_HISTORY - 1)), before], future: [] };
}

function withItem(d: Doc, id: number, f: (it: Item) => Item): Doc {
	return { ...d, items: d.items.map((it) => (it.id === id ? f(it) : it)) };
}

function restore(d: Doc, s: Snapshot): Doc {
	return {
		...d,
		items: s.items,
		crop: s.crop,
		selected: s.items.some((i) => i.id === d.selected) ? d.selected : null,
	};
}

function rectOf(a: Point, b: Point): Rect {
	return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
}

/**
 * The rect dragged from `origin` to `p`, inside the image. `square` (shift) keeps the drag direction and the
 * longer side, limited by the room left on the short side so it never leaves the image.
 */
function dragRect(d: Doc, origin: Point, p: Point, square: boolean): Rect {
	const q = { x: clamp(p.x, 0, d.width), y: clamp(p.y, 0, d.height) };
	if (!square) return rectOf(origin, q);
	const dx = q.x - origin.x;
	const dy = q.y - origin.y;
	const sx = dx < 0 ? -1 : 1;
	const sy = dy < 0 ? -1 : 1;
	const roomX = sx > 0 ? d.width - origin.x : origin.x;
	const roomY = sy > 0 ? d.height - origin.y : origin.y;
	const s = Math.min(Math.max(Math.abs(dx), Math.abs(dy)), roomX, roomY);
	return rectOf(origin, { x: origin.x + sx * s, y: origin.y + sy * s });
}

/** `p` rotated around `origin` to the nearest multiple of 45 degrees, at the same distance. */
function snap45(origin: Point, p: Point): Point {
	const len = Math.hypot(p.x - origin.x, p.y - origin.y);
	const a = Math.round(Math.atan2(p.y - origin.y, p.x - origin.x) / (Math.PI / 4)) * (Math.PI / 4);
	return { x: origin.x + len * Math.cos(a), y: origin.y + len * Math.sin(a) };
}

function translate(it: Item, dx: number, dy: number): Item {
	const mv = (p: Point) => ({ x: p.x + dx, y: p.y + dy });
	const mr = (r: Rect) => ({ ...r, x: r.x + dx, y: r.y + dy });
	switch (it.type) {
		case "arrow":
			return { ...it, from: mv(it.from), to: mv(it.to) };
		case "text":
		case "counter":
			return { ...it, at: mv(it.at) };
		case "marker":
			return { ...it, points: it.points.map(mv) };
		default:
			return { ...it, rect: mr(it.rect) };
	}
}

/** Distance from `p` to the segment `a`-`b` (to the nearest end cap beyond its ends; to the point when `a == b`). */
export function distToSegment(p: Point, a: Point, b: Point): number {
	const dx = b.x - a.x;
	const dy = b.y - a.y;
	const len2 = dx * dx + dy * dy;
	const t = len2 === 0 ? 0 : clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / len2, 0, 1);
	return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/** Bounding box of an item (the selection outline). */
export function boundsOf(it: Item): Rect {
	switch (it.type) {
		case "arrow":
			return rectOf(it.from, it.to);
		case "text":
			return { x: it.at.x, y: it.at.y, ...textBox(it.text, it.size, it.weight) };
		case "counter": {
			const r = counterRadius(it.width);
			return { x: it.at.x - r, y: it.at.y - r, w: 2 * r, h: 2 * r };
		}
		case "marker":
			return markerBounds(it.points, it.width);
		default:
			return it.rect;
	}
}

const inside = (p: Point, r: Rect, pad: number) =>
	p.x >= r.x - pad && p.x <= r.x + r.w + pad && p.y >= r.y - pad && p.y <= r.y + r.h + pad;

function hits(it: Item, p: Point): boolean {
	switch (it.type) {
		case "arrow":
			return distToSegment(p, it.from, it.to) <= Math.max(HIT_ARROW, it.width / 2);
		case "text":
			return inside(p, boundsOf(it), HIT_TEXT);
		case "counter":
			return Math.hypot(p.x - it.at.x, p.y - it.at.y) <= counterRadius(it.width);
		case "marker": {
			const reach = it.width / 2 + HIT_MARKER;
			const pts = it.points;
			if (pts.length === 1) return Math.hypot(p.x - pts[0].x, p.y - pts[0].y) <= reach;
			for (let i = 1; i < pts.length; i++) if (distToSegment(p, pts[i - 1], pts[i]) <= reach) return true;
			return false;
		}
		case "rect": {
			if (it.fill) return inside(p, it.rect, HIT_AREA);
			// Unfilled: only the outline, so what sits inside the frame stays reachable.
			const pad = Math.max(HIT_ARROW, it.width / 2);
			const r = it.rect;
			const core = { x: r.x + pad, y: r.y + pad, w: r.w - 2 * pad, h: r.h - 2 * pad };
			return inside(p, r, pad) && !(core.w > 0 && core.h > 0 && inside(p, core, 0));
		}
		default:
			return inside(p, it.rect, HIT_AREA);
	}
}

/** Id of the topmost item under `p`, or null. */
export function hitTest(d: Doc, p: Point): number | null {
	for (let i = d.items.length - 1; i >= 0; i--) if (hits(d.items[i], p)) return d.items[i].id;
	return null;
}

export function select(d: Doc, id: number | null): Doc {
	return { ...d, selected: d.items.some((i) => i.id === id) ? id : null };
}

/** Start a gesture at `p` (image pixels). Text opens the inline editor instead of dragging. */
export function begin(doc: Doc, tool: Tool, p: Point, style: Style = DEFAULT_STYLE): Doc {
	const d = commitText(doc);
	const before = snap(d);
	if (tool === "select") {
		const id = hitTest(d, p);
		return {
			...d,
			selected: id,
			gesture: id === null ? null : { kind: "move", id, last: p, before, moved: false },
		};
	}
	if (tool === "crop")
		return {
			...d,
			gesture: { kind: "crop", origin: { x: clamp(p.x, 0, d.width), y: clamp(p.y, 0, d.height) }, before },
		};
	if (tool === "text") {
		const hit = hitTest(d, p);
		const existing = d.items.find((i) => i.id === hit);
		if (existing?.type === "text") return editText(d, existing.id);
		const id = d.nextId;
		const at = { x: clamp(p.x, 0, d.width), y: clamp(p.y, 0, d.height) };
		const item: Item = {
			id,
			type: "text",
			at,
			text: "",
			color: style.color,
			size: style.size,
			weight: style.weight,
			bg: style.bg,
		};
		return {
			...record(d, before),
			items: [...d.items, item],
			selected: id,
			editing: { id, fresh: true },
			nextId: id + 1,
		};
	}
	const o = { x: clamp(p.x, 0, d.width), y: clamp(p.y, 0, d.height) };
	const id = d.nextId;
	if (tool === "counter")
		// A click, not a drag: placed and recorded at once, no gesture to finish.
		return {
			...record(d, before),
			items: [...d.items, { id, type: "counter", at: o, color: style.color, width: style.width }],
			selected: id,
			nextId: id + 1,
		};
	const zero = { x: o.x, y: o.y, w: 0, h: 0 };
	const item: Item =
		tool === "arrow"
			? { id, type: "arrow", from: p, to: p, color: style.color, width: style.width }
			: tool === "rect"
				? { id, type: "rect", rect: zero, color: style.color, width: style.width, fill: null }
				: tool === "highlight"
					? { id, type: "highlight", rect: zero, color: style.color }
					: tool === "spotlight"
						? { id, type: "spotlight", rect: zero, shape: style.shape }
						: tool === "marker"
							? { id, type: "marker", points: [o], color: style.color, width: style.width }
							: { id, type: "blur", rect: zero, mode: style.mode, amount: style.amount };
	// The item is live in `items` while dragging; history is only written by `end`, so `record` is not used here.
	return {
		...d,
		items: [...d.items, item],
		nextId: id + 1,
		gesture: { kind: "draw", id, origin: o, before },
	};
}

/** Update the running gesture with the pointer at `p`; `shift` snaps arrows to 45 degrees and rects/crops to squares. */
export function drag(d: Doc, p: Point, shift = false): Doc {
	const g = d.gesture;
	if (!g) return d;
	if (g.kind === "move") {
		const dx = p.x - g.last.x;
		const dy = p.y - g.last.y;
		if (dx === 0 && dy === 0) return d;
		return { ...withItem(d, g.id, (it) => translate(it, dx, dy)), gesture: { ...g, last: p, moved: true } };
	}
	if (g.kind === "crop") return { ...d, crop: dragRect(d, g.origin, p, shift) };
	return withItem(d, g.id, (it) => {
		if (it.type === "arrow") return { ...it, to: shift ? snap45(it.from, p) : p };
		if (it.type === "rect" || it.type === "highlight" || it.type === "blur" || it.type === "spotlight")
			return { ...it, rect: dragRect(d, g.origin, p, shift) };
		if (it.type === "marker") {
			const q = { x: clamp(p.x, 0, d.width), y: clamp(p.y, 0, d.height) };
			const last = it.points.at(-1);
			return last && last.x === q.x && last.y === q.y ? it : { ...it, points: [...it.points, q] };
		}
		return it;
	});
}

function tooSmall(it: Item | undefined): boolean {
	if (!it) return true;
	if (it.type === "arrow") return Math.hypot(it.to.x - it.from.x, it.to.y - it.from.y) < MIN_DRAW;
	if (it.type === "text" || it.type === "counter") return false;
	if (it.type === "marker") {
		const b = markerBounds(it.points, 0);
		return Math.hypot(b.w, b.h) < MIN_DRAW;
	}
	return it.rect.w < MIN_DRAW || it.rect.h < MIN_DRAW;
}

/** Finish the gesture: a real shape/move/crop becomes one undo step, a click leaves no trace. */
export function end(d: Doc): Doc {
	const g = d.gesture;
	if (!g) return d;
	const done = { ...d, gesture: null };
	if (g.kind === "move") return g.moved ? record(done, g.before) : done;
	if (g.kind === "crop") {
		const c = d.crop;
		if (!c || c.w < MIN_CROP || c.h < MIN_CROP) return { ...done, crop: g.before.crop };
		return setCrop({ ...done, crop: g.before.crop }, c);
	}
	if (tooSmall(d.items.find((i) => i.id === g.id))) return { ...done, items: g.before.items };
	return { ...record(done, g.before), selected: g.id };
}

/** Drop the running gesture and put the document back as it was before it began. */
export function cancelGesture(d: Doc): Doc {
	const g = d.gesture;
	return g ? { ...restore(d, g.before), gesture: null } : d;
}

export const canUndo = (d: Doc) => d.gesture !== null || d.editing !== null || d.past.length > 0;
export const canRedo = (d: Doc) => d.gesture === null && d.future.length > 0;

/** Undo. In the middle of a drag (or of typing) it cancels that instead of popping history. */
export function undo(d: Doc): Doc {
	if (d.gesture) return cancelGesture(d);
	if (d.editing) return cancelEdit(d);
	const prev = d.past.at(-1);
	if (!prev) return d;
	return { ...restore(d, prev), past: d.past.slice(0, -1), future: [...d.future, snap(d)] };
}

export function redo(d: Doc): Doc {
	const next = d.future.at(-1);
	if (!next || d.gesture || d.editing) return d;
	return { ...restore(d, next), future: d.future.slice(0, -1), past: [...d.past, snap(d)] };
}

/** Remove the selected item; undo brings it back at the same place in the z-order. */
export function deleteSelected(d: Doc): Doc {
	if (d.selected === null || d.gesture || d.editing) return d;
	return { ...record(d, snap(d)), items: d.items.filter((i) => i.id !== d.selected), selected: null };
}

/** Change colour/width/size/mode of an item (one undo step). */
export function restyle(
	d: Doc,
	id: number,
	patch: Partial<Pick<Style, "color" | "width" | "size" | "mode" | "amount" | "weight" | "bg" | "shape">>,
): Doc {
	const it = d.items.find((i) => i.id === id);
	if (!it) return d;
	const allowed = (k: string) => k in it;
	const next = Object.fromEntries(Object.entries(patch).filter(([k]) => allowed(k)));
	if (Object.keys(next).length === 0) return d;
	return {
		...record(d, snap(d)),
		items: d.items.map((i) => (i.id === id ? ({ ...i, ...next } as Item) : i)),
	};
}

/** Normalise to whole pixels, inside the image, at least 8x8 (or the whole image when it is smaller). */
function normalizeCrop(r: Rect, width: number, height: number): Rect {
	const mw = Math.min(MIN_CROP, width);
	const mh = Math.min(MIN_CROP, height);
	const x1 = clamp(Math.round(Math.min(r.x, r.x + r.w)), 0, width - mw);
	const y1 = clamp(Math.round(Math.min(r.y, r.y + r.h)), 0, height - mh);
	const x2 = clamp(Math.round(Math.max(r.x, r.x + r.w)), x1 + mw, width);
	const y2 = clamp(Math.round(Math.max(r.y, r.y + r.h)), y1 + mh, height);
	return { x: x1, y: y1, w: x2 - x1, h: y2 - y1 };
}

/** Set (or clear with null) the crop. Non-destructive: the items and the image stay whole until export. */
export function setCrop(d: Doc, rect: Rect | null): Doc {
	const crop = rect && normalizeCrop(rect, d.width, d.height);
	const same =
		crop === d.crop ||
		(crop &&
			d.crop &&
			crop.x === d.crop.x &&
			crop.y === d.crop.y &&
			crop.w === d.crop.w &&
			crop.h === d.crop.h);
	return same ? d : { ...record(d, snap(d)), crop };
}

/** Open the inline editor on an existing text item. */
export function editText(doc: Doc, id: number): Doc {
	const d = commitText(doc);
	if (d.items.find((i) => i.id === id)?.type !== "text") return d;
	return { ...record(d, snap(d)), selected: id, editing: { id, fresh: false } };
}

/** Live text while typing; the history step was already written when the editor opened. */
export function setText(d: Doc, text: string): Doc {
	const e = d.editing;
	return e ? withItem(d, e.id, (it) => (it.type === "text" ? { ...it, text } : it)) : d;
}

/** Close the editor keeping the text. Empty text removes the item; an unchanged re-edit leaves no history step. */
export function commitText(d: Doc): Doc {
	const e = d.editing;
	if (!e) return d;
	const it = d.items.find((i) => i.id === e.id);
	const closed = { ...d, editing: null };
	if (it?.type !== "text") return closed;
	if (it.text.trim() === "")
		return e.fresh
			? { ...restore(closed, d.past.at(-1)!), past: d.past.slice(0, -1) }
			: { ...closed, items: d.items.filter((i) => i.id !== e.id), selected: null };
	const before = d.past.at(-1)?.items.find((i) => i.id === e.id);
	if (!e.fresh && before?.type === "text" && before.text === it.text)
		return { ...closed, past: d.past.slice(0, -1) };
	return closed;
}

/** Close the editor discarding the typing: a new text vanishes, an existing one gets its old string back. */
export function cancelEdit(d: Doc): Doc {
	const prev = d.past.at(-1);
	if (!d.editing || !prev) return { ...d, editing: null };
	return { ...restore(d, prev), past: d.past.slice(0, -1), editing: null };
}

export type EscapeAction = "gesture" | "edit" | "deselect" | "close";

/** Esc: cancel the gesture, else the text edit, else deselect, else ask to close. */
export function escape(d: Doc): { doc: Doc; action: EscapeAction } {
	if (d.gesture) return { doc: cancelGesture(d), action: "gesture" };
	if (d.editing) return { doc: cancelEdit(d), action: "edit" };
	if (d.selected !== null) return { doc: { ...d, selected: null }, action: "deselect" };
	return { doc: d, action: "close" };
}

/** The state to compare against for "unsaved edits". */
export const markSaved = (d: Doc): Snapshot => snap(d);

/** True when the items or crop differ from `saved`. Undoing back to the saved state is clean again. */
export const isDirty = (d: Doc, saved: Snapshot): boolean => d.items !== saved.items || d.crop !== saved.crop;
