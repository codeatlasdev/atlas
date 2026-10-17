/**
 * Flicker detection on a recording of the real app: the pure half of `scripts/flicker-sweep.ts`.
 *
 * The existing benches (selection, gesture) measure the main thread and the DOM; a flicker is something the
 * *compositor showed*, so it is judged on pixels. The script decodes a recording into `Frame`s (a `GRID_W x GRID_H`
 * grid of mean luminance per cell, and the frame's presentation time) and this module decides:
 *
 *  - **blip**    a region that differs from the frame before AND the frame after, while those two agree with each
 *                other: the picture left its state for 1-2 frames and came back (a layer swapped out, a native view
 *                painted late, a backdrop re-blurred). A ramp, a one-way change and a held state (3+ frames) are
 *                motion or content, never a blip.
 *  - **blank**   a frame that is black, or flat as a cleared surface, between two frames that have content.
 *  - **flash**   BT.1702 luminance flashing of the whole frame (> 3 pairs of opposing steps per second), judged by
 *                the studio's own `flashWindows` so the app and the site videos share one definition. The script passes
 *                it in (`FlashJudge`): this module is part of the app's type program, which must not import the
 *                studio's Bun-typed code.
 *
 * The recorder (ScreenCaptureKit) is variable frame rate: it emits a frame only when the picture changed, so
 * neighbours are judged by order, never by clock, and a long gap is a still page, not a missing frame.
 */

export const GRID_W = 24;
export const GRID_H = 15;

export interface Frame {
	/** presentation time, seconds from the first frame */
	t: number;
	/** mean relative luminance (0..1) of each cell, row-major, length GRID_W * GRID_H */
	grid: number[];
}

export interface FlickerOpts {
	/** luminance step (0..1) a cell must change by to count as changed */
	delta: number;
	/** luminance step under which two frames count as "the same" on a cell */
	settle: number;
	/** cells that must change together: a caret or one spinner tick is not a flicker */
	minCells: number;
	/** longest excursion, in frames, that is a blip; longer is a state the page deliberately showed */
	maxLen: number;
	/** frames at the start to ignore (the window being created) */
	skipLeading: number;
	/** mean LINEAR luminance (the grid's unit) under which a frame is black: 0.0024 is sRGB 8/255. Dark theme
	 * backgrounds sit at sRGB 18-40/255 (linear 0.006-0.02), so a higher value calls a dark UI black. */
	blackMean: number;
	/** grid standard deviation under which a frame is flat */
	flatStd: number;
	/** neighbours must have at least this standard deviation to call a flat frame "cleared" */
	texturedStd: number;
}

export const DEFAULTS: FlickerOpts = {
	delta: 0.12,
	settle: 0.05,
	minCells: 6,
	maxLen: 2,
	skipLeading: 0,
	blackMean: 0.0024,
	flatStd: 0.004,
	texturedStd: 0.02,
};

const opt = (o: Partial<FlickerOpts>): FlickerOpts => ({ ...DEFAULTS, ...o });

export interface Box {
	x0: number;
	y0: number;
	/** exclusive */
	x1: number;
	y1: number;
}
export interface Blip {
	/** first frame of the excursion */
	frame: number;
	t: number;
	/** frames the picture stayed away (1 or 2) */
	length: number;
	/** cells that changed together */
	cells: number;
	/** bounding box of those cells, in grid units */
	box: Box;
	/** largest luminance step of any cell */
	peak: number;
}
export interface Blank {
	frame: number;
	t: number;
	kind: "black" | "flat";
}
export interface Flash {
	start: number;
	end: number;
}
/** `studio/src/lint/checks-video.ts` `flashWindows`: windows where mean luminance flashes more than 3 times a second. */
export type FlashJudge = (lum: number[], fps: number) => Flash[];
export interface Findings {
	blips: Blip[];
	blanks: Blank[];
	flashes: Flash[];
}

const mean = (g: number[]) => g.reduce((a, b) => a + b, 0) / g.length;
function std(g: number[]): number {
	const m = mean(g);
	return Math.sqrt(g.reduce((a, b) => a + (b - m) ** 2, 0) / g.length);
}

function boxOf(idx: number[]): Box {
	let x0 = GRID_W,
		y0 = GRID_H,
		x1 = 0,
		y1 = 0;
	for (const i of idx) {
		const x = i % GRID_W;
		const y = Math.floor(i / GRID_W);
		x0 = Math.min(x0, x);
		y0 = Math.min(y0, y);
		x1 = Math.max(x1, x + 1);
		y1 = Math.max(y1, y + 1);
	}
	return { x0, y0, x1, y1 };
}

/**
 * Blank frames: a run of at most `maxLen` black frames (or flat as a cleared surface) that has a lit/textured frame
 * on at least one side. Longer runs are a state the page chose (a dark splash, a closed window), and a page that is
 * dark or flat on every frame has nothing to lose: only the *departure* from content is a defect. A run that ends
 * before `skipLeading` is the window being created.
 */
export function blankFrames(frames: Frame[], o: Partial<FlickerOpts> = {}): Blank[] {
	const c = opt(o);
	const kind = (f: Frame): Blank["kind"] | null => {
		if (mean(f.grid) < c.blackMean) return "black";
		return std(f.grid) < c.flatStd ? "flat" : null;
	};
	const lit = (f?: Frame) => !!f && mean(f.grid) >= c.blackMean && std(f.grid) >= c.texturedStd;
	const out: Blank[] = [];
	for (let i = 0; i < frames.length;) {
		const k = kind(frames[i]!);
		if (!k) {
			i++;
			continue;
		}
		let end = i;
		while (end + 1 < frames.length && kind(frames[end + 1]!) === k) end++;
		const len = end - i + 1;
		// flat frames only count when BOTH sides are textured (a uniformly flat app is not "cleared"); black needs one lit side
		const bounded =
			k === "black"
				? mean(frames[i - 1]?.grid ?? [0]) >= c.blackMean || mean(frames[end + 1]?.grid ?? [0]) >= c.blackMean
				: lit(frames[i - 1]) && lit(frames[end + 1]);
		if (bounded && len <= c.maxLen && i >= c.skipLeading)
			for (let j = i; j <= end; j++) out.push({ frame: j, t: frames[j]!.t, kind: k });
		i = end + 1;
	}
	return out;
}

/**
 * Regions that left their state for 1..maxLen frames and came back. For every start frame `i` and length `L` the
 * cells that moved by >= delta away from the frame before AND away from the frame after, while the frame before
 * and after agree (< settle) on them, form the blip when there are at least `minCells` of them.
 */
export function blips(frames: Frame[], o: Partial<FlickerOpts> = {}): Blip[] {
	const c = opt(o);
	const out: Blip[] = [];
	let skipTo = 0;
	for (let i = Math.max(1, c.skipLeading); i < frames.length - 1; i++) {
		if (i < skipTo) continue;
		for (let len = 1; len <= c.maxLen; len++) {
			const after = i + len;
			if (after >= frames.length) break;
			const before = frames[i - 1]!.grid;
			const back = frames[after]!.grid;
			// every frame of the excursion must be away from both ends, in the same cells
			let cells: number[] | null = null;
			for (let k = i; k < after; k++) {
				const g = frames[k]!.grid;
				const hit: number[] = [];
				for (let n = 0; n < g.length; n++) {
					if (
						Math.abs(g[n]! - before[n]!) >= c.delta &&
						Math.abs(g[n]! - back[n]!) >= c.delta &&
						Math.abs(before[n]! - back[n]!) < c.settle
					)
						hit.push(n);
				}
				cells = cells === null ? hit : cells.filter((n) => hit.includes(n));
			}
			if (cells && cells.length >= c.minCells) {
				let peak = 0;
				for (let k = i; k < after; k++)
					for (const n of cells) peak = Math.max(peak, Math.abs(frames[k]!.grid[n]! - before[n]!));
				out.push({ frame: i, t: frames[i]!.t, length: len, cells: cells.length, box: boxOf(cells), peak });
				skipTo = after; // the frames of this blip are not new blips
				break;
			}
		}
	}
	return out;
}

/**
 * Everything wrong in one recording. A frame reported blank is not reported again as a blip: black is the more
 * precise diagnosis. Throws on fewer than 3 frames: a recording with nothing to compare must not read as clean.
 */
export function analyze(frames: Frame[], flashWindows: FlashJudge, o: Partial<FlickerOpts> = {}): Findings {
	if (frames.length < 3) throw new Error(`a flicker verdict needs at least 3 frames, got ${frames.length}`);
	const blanks = blankFrames(frames, o);
	const blank = new Set(blanks.map((b) => b.frame));
	const found = blips(frames, o).filter((b) => {
		for (let k = 0; k < b.length; k++) if (blank.has(b.frame + k)) return false;
		return true;
	});
	// flashing is judged on the mean luminance at the recording's average rate, as the studio lint does
	const span = frames[frames.length - 1]!.t - frames[0]!.t;
	const fps = span > 0 ? Math.round((frames.length - 1) / span) : 60;
	const lum = frames.map((f) => mean(f.grid));
	const flashes = flashWindows(lum, Math.max(1, fps));
	return { blips: found, blanks, flashes };
}

// ── decoding: raw frames -> the grid the detector reads ───────────────────────────────────────────────────────────

/** Linear-light value of one sRGB channel byte (WCAG 2.x), the same transfer the studio lint uses. */
const LIN: number[] = Array.from({ length: 256 }, (_, i) => {
	const c = i / 255;
	return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
});

/**
 * Mean relative luminance per `GRID_W x GRID_H` cell of a raw rgb24 image (`w x h`, row-major). The sweep decodes
 * recordings to a small even multiple of the grid, so every cell is whole pixels.
 */
export function gridOf(rgb: Uint8Array, w: number, h: number): number[] {
	if (rgb.length !== w * h * 3)
		throw new Error(`rgb24 buffer is ${rgb.length} bytes, expected ${w * h * 3} for ${w}x${h}`);
	if (w < GRID_W || h < GRID_H) throw new Error(`${w}x${h} is smaller than the ${GRID_W}x${GRID_H} grid`);
	const sum = new Float64Array(GRID_W * GRID_H);
	const n = new Uint32Array(GRID_W * GRID_H);
	for (let y = 0; y < h; y++) {
		const row = Math.min(GRID_H - 1, Math.floor((y * GRID_H) / h)) * GRID_W;
		for (let x = 0; x < w; x++) {
			const cell = row + Math.min(GRID_W - 1, Math.floor((x * GRID_W) / w));
			const o = (y * w + x) * 3;
			sum[cell]! += 0.2126 * LIN[rgb[o]!]! + 0.7152 * LIN[rgb[o + 1]!]! + 0.0722 * LIN[rgb[o + 2]!]!;
			n[cell]!++;
		}
	}
	return Array.from(sum, (s, i) => s / n[i]!);
}

/**
 * Presentation times (s) from `ffprobe -show_entries frame=pts_time -of csv=p=0`, relative to the first frame. The
 * recorder is variable frame rate, so the times come from the file, never from an `fps=` resample.
 */
export function frameStarts(csv: string): number[] {
	const ts = csv
		.split("\n")
		.map((l) => Number.parseFloat(l))
		.filter((t) => Number.isFinite(t))
		.sort((a, b) => a - b);
	return ts.map((t) => t - ts[0]!);
}

export interface PixelRect {
	x: number;
	y: number;
	w: number;
	h: number;
}

/** A finding's grid box on the recording's own pixels, padded by `pad` cells, clipped to the frame, even-sized. */
export function cropRect(box: Box, w: number, h: number, pad: number): PixelRect {
	const cw = w / GRID_W;
	const ch = h / GRID_H;
	const x0 = Math.max(0, Math.floor((box.x0 - pad) * cw));
	const y0 = Math.max(0, Math.floor((box.y0 - pad) * ch));
	const x1 = Math.min(w, Math.ceil((box.x1 + pad) * cw));
	const y1 = Math.min(h, Math.ceil((box.y1 + pad) * ch));
	return { x: x0, y: y0, w: (x1 - x0) & ~1, h: (y1 - y0) & ~1 };
}

// ── the verdict ───────────────────────────────────────────────────────────────────────────────────────────────────

export interface SurfaceResult {
	surface: string;
	/** frames the recording had: a surface that produced none was not exercised */
	frames: number;
	findings: Findings;
	/** why the surface's script could not run to the end: a surface the sweep could not reach was not exercised */
	undriven?: string;
}

/** One line per finding, naming the surface and the second it happened; exit 1 on any, and on a surface with too few frames. */
export function verdict(results: SurfaceResult[]): { code: 0 | 1; lines: string[] } {
	const lines: string[] = [];
	for (const r of results) {
		if (r.undriven) lines.push(`${r.surface}: could not be driven (${r.undriven})`);
		if (r.frames < 3)
			lines.push(
				`${r.surface}: only ${r.frames} frames recorded, nothing to judge (permission? hidden window?)`,
			);
		for (const b of r.findings.blips)
			lines.push(
				`${r.surface}: blip at ${b.t.toFixed(3)} s (${b.length} frame${b.length > 1 ? "s" : ""}, ${b.cells} cells, peak ${b.peak.toFixed(2)}, cells ${b.box.x0},${b.box.y0}-${b.box.x1},${b.box.y1})`,
			);
		for (const b of r.findings.blanks) lines.push(`${r.surface}: ${b.kind} frame at ${b.t.toFixed(3)} s`);
		for (const f of r.findings.flashes)
			lines.push(
				`${r.surface}: luminance flashing ${f.start.toFixed(2)}-${f.end.toFixed(2)} s (> 3 flashes/s)`,
			);
	}
	return { code: lines.length ? 1 : 0, lines };
}

// ── the bare app ──────────────────────────────────────────────────────────────────────────────────────────────────

/**
 * A JS expression the runner evaluates in the page between surfaces: the open dialogs (kit dialogs stay mounted when
 * closed, so only `data-state="open"` counts). Empty = the bare app. A surface that leaves a modal over the app makes
 * every later recording a recording of that modal.
 */
export const CLEAN_PROBE = `[...document.querySelectorAll("[role=dialog][data-state=open], [role=alertdialog][data-state=open]")].map(e => e.getAttribute("aria-label") ?? e.getAttribute("role"))`;
