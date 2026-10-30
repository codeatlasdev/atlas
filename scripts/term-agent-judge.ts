// The verdict of `scripts/term-agent-bench.ts`, pure so it is unit-tested (`bun test scripts`) and `--judge <report.json>`
// needs no app. Agent TUIs (Claude Code, omp, codex) flicker in a terminal for four different reasons and each has its
// own number here:
//  - a half-drawn frame reached the screen (`torn`: the rows of one capture came from two producer frames),
//  - the picture blipped, went blank or flashed (`flicker`: the findings of `flickerSweep.ts` on the recording),
//  - the renderer drew faster than frames arrived or slower than the display (`pacing`: `term_stats` interval / frames),
//  - scrolling lost or reordered something while output streamed (`scroll`: rows moved against rows expected, reports
//    counted by a mouse-reporting program against the notches sent).

/** Colors the generator gives the first cell of every row of one frame; a capture whose rows disagree is torn. */
export const FRAME_COLORS = [
	[220, 40, 40],
	[40, 200, 60],
	[50, 80, 230],
	[230, 200, 30],
	[200, 50, 200],
	[40, 210, 210],
] as const;

/** Pixel RGB → index in FRAME_COLORS (nearest, within `tol` per channel), or -1 for anything else (text, background). */
export function colorIndex(rgb: readonly [number, number, number], tol = 60): number {
	let best = -1;
	let bestD = Infinity;
	FRAME_COLORS.forEach((c, i) => {
		const d = Math.max(Math.abs(c[0] - rgb[0]), Math.abs(c[1] - rgb[1]), Math.abs(c[2] - rgb[2]));
		if (d < bestD) {
			bestD = d;
			best = i;
		}
	});
	return bestD <= tol ? best : -1;
}

/**
 * The producer's frame ids seen down one capture's probe column (one entry per terminal row; -1 = not a frame color,
 * e.g. the row is empty). A complete frame shows one id on every row that shows one at all.
 */
export function isTorn(rows: readonly number[]): boolean {
	const seen = new Set(rows.filter((r) => r >= 0));
	return seen.size > 1;
}

/** Captures (by index) that are torn, from each capture's probe column. */
export function tornFrames(captures: readonly (readonly number[])[]): number[] {
	return captures.flatMap((c, i) => (isTorn(c) ? [i] : []));
}

/** One pattern's measurement. */
export interface Pattern {
	id: string;
	/** What it imitates, for the report. */
	covers: string;
	/** Frames of the recording of the terminal's crop (a pattern with none was not exercised). */
	captures: number;
	/** Captures whose rows came from two producer frames (`tornFrames`); `null` = the pattern has no per-frame oracle. */
	torn: number[] | null;
	/** `flickerSweep.ts` findings on the same recording. */
	blips: number;
	blanks: number;
	flashes: number;
	/** `term_stats` over the pattern's window. */
	frames: number;
	seconds: number;
	intervalP95Us: number;
	/** Bytes the producer printed in the window (0 = it never ran). */
	bytes: number;
	/** Frames the producer declared complete (one 2026 update, or one redraw). */
	produced: number;
}

/** What the scroll and mouse legs measured. */
export interface Scroll {
	/** Rows the wheel events asked for in total (Σ rows) and rows the viewport moved (`term_stats.vt.display_offset` deltas). */
	rowsAsked: number;
	rowsMoved: number;
	/** Wheel notches sent to a program that owns the mouse, and reports that program counted, in order. */
	notchesSent: number;
	reportsSeen: number;
	reportsInOrder: boolean;
	/** Frame intervals (µs) of the terminal while a recorded trackpad stream replayed (momentum included). */
	momentumIntervalsUs: number[];
	/** Output bytes that streamed during the scroll (the #97 case: scrolling up while `cat` runs). */
	streamedBytes: number;
	/** The viewport stayed where the user left it while output streamed (no yank to the bottom). */
	viewportHeld: boolean;
}

export interface Run {
	/** Refresh rate the pacing is held to. */
	hz: number;
	patterns: Pattern[];
	scroll: Scroll | null;
	/** Why a leg could not run: a leg that did not run is not a pass. */
	undriven: string[];
}

export interface Budget {
	/** interval p95 of the renderer ≤ this × the refresh interval while flooding (study §4.3). */
	intervalP95: number;
	/** frames presented per second ≤ this × the refresh rate (coalescing: bursts must not each draw). */
	fpsOver: number;
	/** frames presented per second ≥ this fraction of the refresh rate while the producer printed (it must not stall). */
	fpsUnder: number;
	/** rows moved / rows asked must be within this of 1 (momentum and the sub-row snap round at most a row). */
	scrollRowsTolerance: number;
	/** momentum interval p95 ≤ this × the refresh interval. */
	momentumP95: number;
}

export const BUDGET: Budget = {
	intervalP95: 1.1,
	fpsOver: 1.05,
	fpsUnder: 0.5,
	scrollRowsTolerance: 1,
	momentumP95: 1.5,
};

const pct = (xs: readonly number[], p: number) => {
	if (!xs.length) return 0;
	const s = [...xs].sort((a, b) => a - b);
	return s[Math.min(s.length - 1, Math.ceil(p * s.length) - 1)]!;
};

/** Violations of `budget`, one readable line each; empty = within budget. A run that measured nothing is itself one. */
export function check(run: Run, budget: Budget = BUDGET): string[] {
	const bad: string[] = [];
	const frameMs = 1000 / run.hz;
	for (const u of run.undriven) bad.push(`could not run: ${u}`);
	if (!run.patterns.length) bad.push("no pattern was measured (a crashed app is not a pass)");
	for (const p of run.patterns) {
		if (p.bytes <= 0) bad.push(`${p.id}: the producer printed nothing`);
		if (p.captures < 3) bad.push(`${p.id}: only ${p.captures} captures recorded, nothing to judge`);
		if (p.torn?.length) bad.push(`${p.id}: ${p.torn.length} torn frames (rows from two producer frames on one capture, first at capture ${p.torn[0]})`);
		if (p.blips) bad.push(`${p.id}: ${p.blips} blips`);
		if (p.blanks) bad.push(`${p.id}: ${p.blanks} blank frames`);
		if (p.flashes) bad.push(`${p.id}: ${p.flashes} flashing windows`);
		if (p.seconds > 0 && p.bytes > 0) {
			const fps = p.frames / p.seconds;
			if (fps > run.hz * budget.fpsOver) bad.push(`${p.id}: drew ${fps.toFixed(1)} fps on a ${run.hz} Hz display (coalescing: renders per second must not pass the refresh rate)`);
			if (fps < run.hz * budget.fpsUnder) bad.push(`${p.id}: drew ${fps.toFixed(1)} fps while the producer printed (under ${Math.round(budget.fpsUnder * 100)}% of ${run.hz} Hz)`);
			if (p.intervalP95Us / 1000 > frameMs * budget.intervalP95) bad.push(`${p.id}: frame interval p95 ${(p.intervalP95Us / 1000).toFixed(2)} ms > ${(frameMs * budget.intervalP95).toFixed(2)} ms (${budget.intervalP95}x refresh)`);
		}
	}
	const s = run.scroll;
	if (!s) bad.push("the scroll leg did not run");
	else {
		if (s.rowsAsked > 0 && Math.abs(s.rowsMoved - s.rowsAsked) > budget.scrollRowsTolerance)
			bad.push(`scroll moved ${s.rowsMoved} rows for ${s.rowsAsked} asked (tolerance ${budget.scrollRowsTolerance})`);
		if (s.rowsAsked <= 0) bad.push("the scroll leg asked for no rows");
		if (s.reportsSeen !== s.notchesSent) bad.push(`the mouse program counted ${s.reportsSeen} wheel reports for ${s.notchesSent} notches sent (dropped or duplicated)`);
		if (!s.reportsInOrder) bad.push("wheel reports reached the program out of order");
		if (!s.viewportHeld) bad.push(`the viewport was pulled back while ${s.streamedBytes} bytes streamed (#97)`);
		if (!s.momentumIntervalsUs.length) bad.push("no frame intervals were recorded during the momentum replay");
		else if (pct(s.momentumIntervalsUs, 0.95) / 1000 > frameMs * budget.momentumP95)
			bad.push(`momentum frame interval p95 ${(pct(s.momentumIntervalsUs, 0.95) / 1000).toFixed(2)} ms > ${(frameMs * budget.momentumP95).toFixed(2)} ms`);
	}
	return bad;
}

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object";
const isNum = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);

/** The `run` of a saved report, or the reason it is not one: a hand-edited or truncated file must not pass every budget. */
export function parseRun(raw: unknown): { run: Run } | { error: string } {
	if (!isObj(raw)) return { error: "not an object" };
	const run = (isObj(raw.run) ? raw.run : raw) as Record<string, unknown>;
	if (!isNum(run.hz) || run.hz <= 0) return { error: "hz is not a positive number" };
	if (!Array.isArray(run.patterns)) return { error: "patterns is not a list" };
	for (const p of run.patterns) {
		if (!isObj(p) || typeof p.id !== "string") return { error: "a pattern has no id" };
		for (const k of ["captures", "blips", "blanks", "flashes", "frames", "seconds", "intervalP95Us", "bytes", "produced"])
			if (!isNum(p[k])) return { error: `pattern ${p.id}: ${k} is not a number` };
		if (p.torn !== null && !(Array.isArray(p.torn) && p.torn.every(isNum))) return { error: `pattern ${p.id}: torn is not a list of numbers or null` };
	}
	const s = run.scroll;
	if (s !== null) {
		if (!isObj(s)) return { error: "scroll is neither an object nor null" };
		for (const k of ["rowsAsked", "rowsMoved", "notchesSent", "reportsSeen", "streamedBytes"]) if (!isNum(s[k])) return { error: `scroll.${k} is not a number` };
		if (!Array.isArray(s.momentumIntervalsUs) || !s.momentumIntervalsUs.every(isNum)) return { error: "scroll.momentumIntervalsUs is not a list of numbers" };
		if (typeof s.reportsInOrder !== "boolean" || typeof s.viewportHeld !== "boolean") return { error: "scroll.reportsInOrder/viewportHeld are not booleans" };
	}
	if (!Array.isArray(run.undriven) || !run.undriven.every((u) => typeof u === "string")) return { error: "undriven is not a list of strings" };
	return { run: run as unknown as Run };
}

/** The numbers worth a line in the report, per pattern. */
export function summarize(run: Run) {
	return run.patterns.map((p) => ({
		id: p.id,
		fps: p.seconds > 0 ? Math.round((p.frames / p.seconds) * 10) / 10 : 0,
		intervalP95Ms: Math.round(p.intervalP95Us / 100) / 10,
		torn: p.torn?.length ?? null,
		blips: p.blips,
		blanks: p.blanks,
		flashes: p.flashes,
	}));
}
