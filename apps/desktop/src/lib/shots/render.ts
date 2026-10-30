/**
 * Pure geometry and pixel helpers of the annotation editor. Everything is in IMAGE pixels; the page converts to CSS
 * pixels with a `View`. Nothing here touches the DOM, so it runs under bun. The canvas painters (`paintItems`) only
 * call methods of the context they are given.
 */
import type { Item, Point, SpotlightShape, TextBg, TextWeight } from "./annotate.ts";
import type { Rect } from "./types.ts";

/** Pixels in the layout `ImageData` has. A plain object works, so tests need no canvas. */
export interface Pixels {
	data: Uint8ClampedArray;
	width: number;
	height: number;
}

export type BlurMode = "blur" | "pixelate";

const DEG = Math.PI / 180;
const HEAD_HALF_ANGLE = 28 * DEG;
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 32;
/** Average glyph advance over the em size for a UI sans-serif. Only used for hit boxes and the input; drawing uses the real font. */
const CHAR_W = 0.56;
const LINE_H = 1.25;
/** Bold glyphs are a little wider than regular ones. */
const BOLD_W = 1.08;

/** The two barb tips of an arrow head at `to`: length 4*width clamped to 10..40, 28 degrees either side of the shaft. */
export function arrowHead(from: Point, to: Point, width: number): [Point, Point] {
	const len = Math.min(40, Math.max(10, 4 * width));
	const a = Math.atan2(to.y - from.y, to.x - from.x); // atan2(0, 0) = 0: a zero-length arrow points right, no NaN
	return [-1, 1].map((s) => ({
		x: to.x - len * Math.cos(a + s * HEAD_HALF_ANGLE),
		y: to.y - len * Math.sin(a + s * HEAD_HALF_ANGLE),
	})) as [Point, Point];
}

/** The grid of blocks that pixelate `rect`: block side max(2, round(amount)), anchored at the rect's corner, edge blocks partial. */
export function pixelateBlocks(rect: Rect, amount: number): Rect[] {
	if (!(rect.w > 0) || !(rect.h > 0)) return [];
	const s = Math.max(2, Math.round(amount));
	const out: Rect[] = [];
	for (let y = rect.y; y < rect.y + rect.h; y += s) {
		for (let x = rect.x; x < rect.x + rect.w; x += s) {
			out.push({ x, y, w: Math.min(s, rect.x + rect.w - x), h: Math.min(s, rect.y + rect.h - y) });
		}
	}
	return out;
}

/** Zoom that fits the image in the viewport minus `padding` on each side. Never upscales: 100% is the sharpest view. */
export function fitZoom(
	image: { w: number; h: number },
	viewport: { w: number; h: number },
	padding: number,
): number {
	if (!(image.w > 0) || !(image.h > 0)) return 1;
	const z = Math.min((viewport.w - 2 * padding) / image.w, (viewport.h - 2 * padding) / image.h);
	return Math.min(1, Math.max(MIN_ZOOM, z));
}

/** Where the image sits in the viewport: view = image * zoom + (x, y). */
export interface View {
	zoom: number;
	x: number;
	y: number;
}

export function imageToView(p: Point, v: View): Point {
	return { x: p.x * v.zoom + v.x, y: p.y * v.zoom + v.y };
}

export function viewToImage(p: Point, v: View): Point {
	return { x: (p.x - v.x) / v.zoom, y: (p.y - v.y) / v.zoom };
}

/** A view at `zoom` with the image centred in the viewport. */
export function centerView(
	image: { w: number; h: number },
	viewport: { w: number; h: number },
	zoom: number,
): View {
	return { zoom, x: (viewport.w - image.w * zoom) / 2, y: (viewport.h - image.h * zoom) / 2 };
}

/** Change the zoom keeping the image point under `anchor` (a view point) where it is. */
export function zoomAt(v: View, zoom: number, anchor: Point): View {
	const z = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
	const k = z / v.zoom;
	return { zoom: z, x: anchor.x - (anchor.x - v.x) * k, y: anchor.y - (anchor.y - v.y) * k };
}

/** Size of the exported image: the crop when there is one, else the whole image. */
export function exportSize(doc: { width: number; height: number; crop: Rect | null }): {
	w: number;
	h: number;
} {
	return doc.crop ? { w: doc.crop.w, h: doc.crop.h } : { w: doc.width, h: doc.height };
}

/**
 * APPROXIMATE size of a single line of text: a fixed advance per character. The editor uses it for hit boxes and
 * the inline input; the real draw uses the font's own metrics (`ctx.measureText`). Empty text still has a caret's width.
 */
export function textBox(
	text: string,
	size: number,
	weight: TextWeight = "regular",
): { w: number; h: number } {
	return { w: Math.max(1, text.length) * size * CHAR_W * (weight === "bold" ? BOLD_W : 1), h: size * LINE_H };
}

/** The canvas font of a text item. `size` is in image pixels. */
export function textFont(size: number, weight: TextWeight = "regular"): string {
	return `${weight === "bold" ? 700 : 400} ${size}px system-ui, -apple-system, 'Segoe UI', sans-serif`;
}

/** Black or white, whichever reads on `color` (a `#rgb`/`#rrggbb` string); white when it cannot be parsed. */
function contrastOn(color: string): string {
	const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color);
	if (!m) return "#ffffff";
	const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
	const [r, g, b] = [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
	// Rec. 601 luma: cheap and good enough to pick a text colour
	return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.6 ? "#111111" : "#ffffff";
}

/**
 * The box behind a text (`solid` or `pill`), in image pixels, and the colour its glyphs take to stay readable
 * on the item's colour. Null for no background. `box` is the text's own box (`textBox`).
 */
export function textBg(
	bg: TextBg,
	at: Point,
	box: { w: number; h: number },
	size: number,
	color = "#ffffff",
): { x: number; y: number; w: number; h: number; r: number; fg: string } | null {
	if (bg === "none") return null;
	const padX = Math.round(size * (bg === "pill" ? 0.5 : 0.3));
	const padY = Math.round(size * 0.15);
	const h = box.h + 2 * padY;
	return {
		x: at.x - padX,
		y: at.y - padY,
		w: box.w + 2 * padX,
		h,
		r: bg === "pill" ? h / 2 : Math.round(size * 0.15),
		fg: contrastOn(color),
	};
}

/** Radius of a counter's disc for a stroke width: 10 + 2*width, whole pixels, so the digits stay legible at width 2. */
export function counterRadius(width: number): number {
	return 10 + 2 * Math.round(Math.max(0, Number.isFinite(width) ? width : 0));
}

/** Bounding box of a marker stroke: its points grown by half the line width. No points: an empty box at the origin. */
export function markerBounds(points: Point[], width: number): Rect {
	if (points.length === 0) return { x: 0, y: 0, w: 0, h: 0 };
	let x1 = Infinity;
	let y1 = Infinity;
	let x2 = -Infinity;
	let y2 = -Infinity;
	for (const p of points) {
		x1 = Math.min(x1, p.x);
		y1 = Math.min(y1, p.y);
		x2 = Math.max(x2, p.x);
		y2 = Math.max(y2, p.y);
	}
	const h = width / 2;
	return { x: x1 - h, y: y1 - h, w: x2 - x1 + width, h: y2 - y1 + width };
}

/** The holes of the spotlight dim layer: one per non-empty spotlight, in z-order. */
export function spotlightHoles(items: Item[]): { rect: Rect; shape: SpotlightShape }[] {
	const out: { rect: Rect; shape: SpotlightShape }[] = [];
	for (const it of items)
		if (it.type === "spotlight" && it.rect.w > 0 && it.rect.h > 0)
			out.push({ rect: it.rect, shape: it.shape });
	return out;
}

export interface Gradient {
	name: string;
	/** CSS angle in degrees: 0 is bottom-to-top, 90 left-to-right, 135 top-left to bottom-right. */
	angle: number;
	stops: string[];
}

export const BEAUTIFY_GRADIENTS: Gradient[] = [
	{ name: "Sunset", angle: 135, stops: ["#f97316", "#ec4899"] },
	{ name: "Ocean", angle: 135, stops: ["#06b6d4", "#3b82f6"] },
	{ name: "Aurora", angle: 135, stops: ["#22c55e", "#3b82f6", "#a855f7"] },
	{ name: "Midnight", angle: 135, stops: ["#1e293b", "#6366f1"] },
];
export const BEAUTIFY_PADDINGS = [0, 32, 64, 96];
export const BEAUTIFY_RADII = [0, 12, 24];

export type BeautifyBackground = { kind: "solid"; color: string } | { kind: "gradient"; index: number };

/** Export-only styling: the picture on a background. Not part of the undo history; the page keeps it beside the doc. */
export interface Beautify {
	on: boolean;
	background: BeautifyBackground;
	padding: number;
	radius: number;
	shadow: boolean;
}

export const BEAUTIFY_DEFAULT: Beautify = {
	on: false,
	background: { kind: "gradient", index: 0 },
	padding: 64,
	radius: 12,
	shadow: true,
};

export interface BeautifyLayout {
	/** Size of the exported picture. */
	out: { w: number; h: number };
	/** Where the (already cropped and annotated) image sits inside it. */
	image: Rect;
	/** Corner radius of the image, clamped to half its shorter side. */
	radius: number;
}

/** The preset nearest to `v` (the first one on a tie); the first preset for NaN. */
const nearest = (v: number, presets: number[]) =>
	Number.isFinite(v) ? presets.reduce((a, b) => (Math.abs(b - v) < Math.abs(a - v) ? b : a)) : presets[0];

/**
 * Layout of the beautified export from the size of the image after cropping: output = image + 2*padding, the radius
 * limited so the corners never overlap. The padding snaps to the presets so a stale value cannot produce an odd size.
 */
export function beautifyLayout(img: { w: number; h: number }, b: Beautify): BeautifyLayout {
	if (!b.on) return { out: { w: img.w, h: img.h }, image: { x: 0, y: 0, w: img.w, h: img.h }, radius: 0 };
	const pad = nearest(b.padding, BEAUTIFY_PADDINGS);
	const radius = Number.isFinite(b.radius) ? Math.min(Math.max(0, b.radius), Math.min(img.w, img.h) / 2) : 0;
	return {
		out: { w: img.w + 2 * pad, h: img.h + 2 * pad },
		image: { x: pad, y: pad, w: img.w, h: img.h },
		radius,
	};
}

/** Endpoints of a CSS-angle gradient across a `w` x `h` box (the line through the centre, as long as the box needs). */
function gradientLine(w: number, h: number, angle: number): [number, number, number, number] {
	const a = angle * DEG;
	const dx = Math.sin(a);
	const dy = -Math.cos(a);
	const half = (Math.abs(w * dx) + Math.abs(h * dy)) / 2;
	return [w / 2 - dx * half, h / 2 - dy * half, w / 2 + dx * half, h / 2 + dy * half];
}

/**
 * The backdrop of a beautified picture: the background over the whole output and the soft shadow of the image. The
 * image itself is NOT drawn, so the editor can show it live on top (`paintBeautified` adds it for the export).
 */
export function paintBackdrop(
	ctx: CanvasRenderingContext2D,
	img: { w: number; h: number },
	b: Beautify,
): void {
	if (!b.on) return;
	const l = beautifyLayout(img, b);
	if (b.background.kind === "solid") ctx.fillStyle = b.background.color;
	else {
		const g = BEAUTIFY_GRADIENTS[b.background.index] ?? BEAUTIFY_GRADIENTS[0];
		const [x0, y0, x1, y1] = gradientLine(l.out.w, l.out.h, g.angle);
		const grad = ctx.createLinearGradient(x0, y0, x1, y1);
		g.stops.forEach((c, i) => grad.addColorStop(g.stops.length === 1 ? 0 : i / (g.stops.length - 1), c));
		ctx.fillStyle = grad;
	}
	ctx.fillRect(0, 0, l.out.w, l.out.h);
	if (!b.shadow) return;
	const r = l.image;
	// The shadow is cast by an opaque rounded shape under the image, so the image's clip does not cut it.
	ctx.save();
	ctx.shadowColor = "rgba(0, 0, 0, 0.45)";
	ctx.shadowBlur = Math.max(12, Math.round(Math.min(r.w, r.h) * 0.08));
	ctx.shadowOffsetY = Math.round(ctx.shadowBlur / 3);
	ctx.fillStyle = "#000000";
	ctx.beginPath();
	ctx.roundRect(r.x, r.y, r.w, r.h, l.radius);
	ctx.fill();
	ctx.restore();
}

/**
 * Draw `source` (the cropped, annotated picture, `img` big) onto `ctx` as `beautifyLayout` says: backdrop, then the
 * image with rounded corners. The caller sizes the canvas to `layout.out`.
 */
export function paintBeautified(
	ctx: CanvasRenderingContext2D,
	source: CanvasImageSource,
	img: { w: number; h: number },
	b: Beautify,
): void {
	const l = beautifyLayout(img, b);
	paintBackdrop(ctx, img, b);
	const r = l.image;
	ctx.save();
	if (l.radius > 0) {
		ctx.beginPath();
		ctx.roundRect(r.x, r.y, r.w, r.h, l.radius);
		ctx.clip();
	}
	ctx.drawImage(source, r.x, r.y, r.w, r.h);
	ctx.restore();
}

function clipToImage(img: Pixels, r: Rect): Rect | null {
	const x1 = Math.max(0, Math.round(r.x));
	const y1 = Math.max(0, Math.round(r.y));
	const x2 = Math.min(img.width, Math.round(r.x + r.w));
	const y2 = Math.min(img.height, Math.round(r.y + r.h));
	return x2 > x1 && y2 > y1 ? { x: x1, y: y1, w: x2 - x1, h: y2 - y1 } : null;
}

/** Mean of each block, plain per-channel (screenshots are opaque). */
function pixelate(img: Pixels, r: Rect, amount: number): void {
	const { data, width } = img;
	for (const b of pixelateBlocks(r, amount)) {
		const sum = [0, 0, 0, 0];
		for (let y = b.y; y < b.y + b.h; y++) {
			for (let x = b.x; x < b.x + b.w; x++) {
				const i = (y * width + x) * 4;
				for (let c = 0; c < 4; c++) sum[c] += data[i + c];
			}
		}
		const n = b.w * b.h;
		for (let y = b.y; y < b.y + b.h; y++) {
			for (let x = b.x; x < b.x + b.w; x++) {
				const i = (y * width + x) * 4;
				for (let c = 0; c < 4; c++) data[i + c] = Math.round(sum[c] / n);
			}
		}
	}
}

/** One box-blur pass over a line, edges clamped to the line (so it never reads outside the region). */
function boxLine(src: Float32Array, dst: Float32Array, n: number, r: number): void {
	const at = (i: number) => src[i < 0 ? 0 : i >= n ? n - 1 : i];
	let sum = 0;
	for (let k = -r; k <= r; k++) sum += at(k);
	const d = 2 * r + 1;
	for (let i = 0; i < n; i++) {
		dst[i] = sum / d;
		sum += at(i + r + 1) - at(i - r);
	}
}

/** Three box passes per axis (close to a gaussian) over the region, sampling only inside it. */
function boxBlur(img: Pixels, r: Rect, amount: number): void {
	const radius = Math.max(1, Math.round(amount));
	const { data, width } = img;
	const buf = new Float32Array(r.w * r.h * 4);
	for (let y = 0; y < r.h; y++) {
		for (let x = 0; x < r.w; x++) {
			const s = ((r.y + y) * width + r.x + x) * 4;
			const d = (y * r.w + x) * 4;
			for (let c = 0; c < 4; c++) buf[d + c] = data[s + c];
		}
	}
	const line = new Float32Array(Math.max(r.w, r.h));
	const out = new Float32Array(Math.max(r.w, r.h));
	for (let pass = 0; pass < 3; pass++) {
		for (const horizontal of [true, false]) {
			const n = horizontal ? r.w : r.h;
			const lines = horizontal ? r.h : r.w;
			const step = horizontal ? 4 : r.w * 4;
			for (let l = 0; l < lines; l++) {
				const base = horizontal ? l * r.w * 4 : l * 4;
				for (let c = 0; c < 4; c++) {
					for (let i = 0; i < n; i++) line[i] = buf[base + i * step + c];
					boxLine(line, out, n, radius);
					for (let i = 0; i < n; i++) buf[base + i * step + c] = out[i];
				}
			}
		}
	}
	for (let y = 0; y < r.h; y++) {
		for (let x = 0; x < r.w; x++) {
			const d = ((r.y + y) * width + r.x + x) * 4;
			const s = (y * r.w + x) * 4;
			for (let c = 0; c < 4; c++) data[d + c] = Math.round(buf[s + c]);
		}
	}
}

/**
 * Replace the pixels of `rect` (clipped to the image) with their blurred or pixelated version, in place. This is
 * what the export does: the original pixels are gone, not hidden under an overlay. Pixels outside `rect` are never
 * read or written.
 */
export function applyBlur(img: Pixels, rect: Rect, mode: BlurMode, amount: number): void {
	const r = clipToImage(img, rect);
	if (!r) return;
	if (mode === "pixelate") pixelate(img, r, amount);
	else boxBlur(img, r, amount);
}

export interface Patch extends Pixels {
	x: number;
	y: number;
}

/** The blurred pixels of a blur item's region, cut from `base` (which is left untouched). Null when the region is off the image. */
export function blurPatch(base: Pixels, item: Extract<Item, { type: "blur" }>): Patch | null {
	const r = clipToImage(base, item.rect);
	if (!r) return null;
	const data = new Uint8ClampedArray(r.w * r.h * 4);
	for (let y = 0; y < r.h; y++) {
		const s = ((r.y + y) * base.width + r.x) * 4;
		data.set(base.data.subarray(s, s + r.w * 4), y * r.w * 4);
	}
	const patch = { x: r.x, y: r.y, width: r.w, height: r.h, data };
	applyBlur(patch, { x: 0, y: 0, w: r.w, h: r.h }, item.mode, item.amount);
	return patch;
}

export interface PaintOptions {
	/** The untouched source pixels: blur items are cut from here, so they also hide whatever was drawn beneath them. */
	base: Pixels;
	/** Builds the context's `ImageData` from a patch. Defaults to `new ImageData(...)`; tests pass a plain object. */
	toImageData?: (p: Pixels) => ImageData;
	/** Cache of already blurred patches keyed by the item's blur parameters, for the live preview. */
	cache?: Map<string, Patch | null>;
	/** Counter numbers (`counterNumbers(items)`); a counter with no entry draws its disc only. */
	numbers?: Map<number, number>;
	/** Size of the image: the spotlight dim layer covers it. Without it spotlights draw nothing. */
	size?: { w: number; h: number };
}

const MARKER_ALPHA = 0.55;
/** The freehand highlighter: thick, translucent, multiplied onto the picture. */
const HIGHLIGHTER_ALPHA = 0.4;
const SPOTLIGHT_DIM = "rgba(0, 0, 0, 0.6)";

function paintCounter(
	ctx: CanvasRenderingContext2D,
	it: Extract<Item, { type: "counter" }>,
	n: number | undefined,
) {
	const r = counterRadius(it.width);
	ctx.fillStyle = it.color;
	ctx.beginPath();
	ctx.arc(it.at.x, it.at.y, r, 0, 2 * Math.PI);
	ctx.fill();
	if (n === undefined) return;
	ctx.fillStyle = contrastOn(it.color);
	ctx.font = `700 ${Math.round(r * 1.1)}px system-ui, -apple-system, 'Segoe UI', sans-serif`;
	ctx.textAlign = "center";
	ctx.textBaseline = "middle";
	ctx.fillText(String(n), it.at.x, it.at.y);
	ctx.textAlign = "start";
}

function paintMarkerStroke(ctx: CanvasRenderingContext2D, it: Extract<Item, { type: "marker" }>) {
	const pts = it.points;
	if (pts.length === 0) return;
	ctx.save();
	ctx.globalCompositeOperation = "multiply";
	ctx.globalAlpha = HIGHLIGHTER_ALPHA;
	ctx.strokeStyle = ctx.fillStyle = it.color;
	ctx.lineWidth = it.width;
	ctx.lineCap = ctx.lineJoin = "round";
	if (pts.length === 1) {
		ctx.beginPath();
		ctx.arc(pts[0].x, pts[0].y, it.width / 2, 0, 2 * Math.PI);
		ctx.fill();
	} else {
		ctx.beginPath();
		ctx.moveTo(pts[0].x, pts[0].y);
		for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y);
		ctx.stroke();
	}
	ctx.restore();
}

/** One dim layer over the whole image with a hole per spotlight (even-odd), so overlapping spotlights do not double-dim. */
function paintSpotlights(
	ctx: CanvasRenderingContext2D,
	holes: ReturnType<typeof spotlightHoles>,
	size: { w: number; h: number },
) {
	ctx.save();
	ctx.fillStyle = SPOTLIGHT_DIM;
	ctx.beginPath();
	ctx.rect(0, 0, size.w, size.h);
	for (const { rect: r, shape } of holes) {
		if (shape === "ellipse") {
			ctx.moveTo(r.x + r.w, r.y + r.h / 2);
			ctx.ellipse(r.x + r.w / 2, r.y + r.h / 2, r.w / 2, r.h / 2, 0, 0, 2 * Math.PI);
		} else ctx.rect(r.x, r.y, r.w, r.h);
	}
	ctx.fill("evenodd");
	ctx.restore();
}

function paintText(ctx: CanvasRenderingContext2D, it: Extract<Item, { type: "text" }>) {
	const weight = it.weight ?? "regular";
	ctx.font = textFont(it.size, weight);
	ctx.textBaseline = "top";
	const bg = textBg(it.bg ?? "none", it.at, textBox(it.text, it.size, weight), it.size, it.color);
	if (bg) {
		ctx.fillStyle = it.color;
		ctx.beginPath();
		ctx.roundRect(bg.x, bg.y, bg.w, bg.h, bg.r);
		ctx.fill();
		ctx.fillStyle = bg.fg;
	} else ctx.fillStyle = it.color;
	ctx.fillText(it.text, it.at.x, it.at.y);
}

function paintArrow(ctx: CanvasRenderingContext2D, it: Extract<Item, { type: "arrow" }>): void {
	const [b1, b2] = arrowHead(it.from, it.to, it.width);
	// The shaft ends at the middle of the head's base, so its round cap cannot poke out of the tip.
	const end = { x: (b1.x + b2.x) / 2, y: (b1.y + b2.y) / 2 };
	ctx.strokeStyle = ctx.fillStyle = it.color;
	ctx.lineWidth = it.width;
	ctx.lineCap = ctx.lineJoin = "round";
	ctx.beginPath();
	ctx.moveTo(it.from.x, it.from.y);
	ctx.lineTo(end.x, end.y);
	ctx.stroke();
	ctx.beginPath();
	ctx.moveTo(it.to.x, it.to.y);
	ctx.lineTo(b1.x, b1.y);
	ctx.lineTo(b2.x, b2.y);
	ctx.closePath();
	ctx.fill();
	ctx.stroke();
}

function paintMarker(ctx: CanvasRenderingContext2D, r: Rect, color: string): void {
	ctx.save();
	ctx.globalCompositeOperation = "multiply";
	ctx.globalAlpha = MARKER_ALPHA;
	ctx.fillStyle = color;
	ctx.fillRect(r.x, r.y, r.w, r.h);
	ctx.restore();
}

/** Draw `items` in z-order onto a context whose pixels are the image's (scale 1, no transform). */
export function paintItems(ctx: CanvasRenderingContext2D, items: Item[], opts: PaintOptions): void {
	const toImageData =
		opts.toImageData ??
		((p: Pixels) => new ImageData(p.data as Uint8ClampedArray<ArrayBuffer>, p.width, p.height));
	// Every spotlight shares one dim layer, drawn where the first one sits in the z-order.
	const holes = spotlightHoles(items);
	const firstSpot = items.findIndex((i) => i.type === "spotlight");
	for (const [idx, it] of items.entries()) {
		switch (it.type) {
			case "counter":
				paintCounter(ctx, it, opts.numbers?.get(it.id));
				break;
			case "marker":
				paintMarkerStroke(ctx, it);
				break;
			case "spotlight":
				if (idx === firstSpot && opts.size && holes.length > 0) paintSpotlights(ctx, holes, opts.size);
				break;
			case "arrow":
				paintArrow(ctx, it);
				break;
			case "rect":
				if (it.fill === "highlight") paintMarker(ctx, it.rect, it.color);
				ctx.strokeStyle = it.color;
				ctx.lineWidth = it.width;
				ctx.lineJoin = "round";
				ctx.strokeRect(it.rect.x, it.rect.y, it.rect.w, it.rect.h);
				break;
			case "highlight":
				paintMarker(ctx, it.rect, it.color);
				break;
			case "text":
				paintText(ctx, it);
				break;
			case "blur": {
				const key = `${it.mode}:${it.amount}:${it.rect.x},${it.rect.y},${it.rect.w},${it.rect.h}`;
				let patch = opts.cache?.get(key);
				if (patch === undefined) {
					patch = blurPatch(opts.base, it);
					opts.cache?.set(key, patch);
				}
				if (patch) ctx.putImageData(toImageData(patch), patch.x, patch.y);
				break;
			}
		}
	}
}
