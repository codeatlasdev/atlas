// Incident class: agent TUIs (Claude Code, omp, codex) flicker or lose scroll in the GPU terminal, and nothing measured it.
// The bench is only worth having if it cannot pass by accident: a run that measured nothing, a producer that never
// printed, a torn capture, a renderer drawing 400 fps or 5 fps, a lost wheel report and a hand-edited report must each
// trip their own line, and the generator's frames must be what the oracle reads. `--judge <report.json>` re-judges a
// saved report without the app; these tests run the real script as a process and check its exit code and message.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { BUDGET, FRAME_COLORS, check, colorIndex, isTorn, parseRun, summarize, tornFrames, type Pattern, type Run } from "./term-agent-judge.ts";
import { PATTERNS, frameBytes } from "./term-agent-gen.ts";

const script = join(import.meta.dir, "term-agent-bench.ts");
const dir = mkdtempSync(join(tmpdir(), "term-agent-"));

const pattern = (id: string, o: Partial<Pattern> = {}): Pattern => ({
	id,
	covers: id,
	captures: 600,
	torn: [],
	blips: 0,
	blanks: 0,
	flashes: 0,
	frames: 60 * 10,
	seconds: 10,
	intervalP95Us: 16_700,
	bytes: 5_000_000,
	produced: 600,
	...o,
});
/** A healthy run on a 60 Hz display: every pattern at the refresh rate, a scroll that adds up. */
const healthy = (o: Partial<Run> = {}): Run => ({
	hz: 60,
	patterns: PATTERNS.map((p) => pattern(p.id)),
	scroll: {
		rowsAsked: 120,
		rowsMoved: 120,
		notchesSent: 40,
		reportsSeen: 40,
		reportsInOrder: true,
		momentumIntervalsUs: Array.from({ length: 100 }, () => 16_700),
		streamedBytes: 4_000_000,
		viewportHeld: true,
	},
	undriven: [],
	...o,
});
const report = (run: unknown, name: string) => {
	const p = join(dir, `${name}.json`);
	writeFileSync(p, JSON.stringify({ run }));
	return p;
};
const judge = (path: string, ...extra: string[]) => {
	const r = Bun.spawnSync(["bun", script, "--judge", path, ...extra], { stderr: "pipe", stdout: "pipe" });
	return { code: r.exitCode, err: r.stderr.toString() };
};

describe("each way an agent TUI breaks in the terminal trips its own line", () => {
	test("a healthy run passes, as a library call and as the script", () => {
		expect(check(healthy())).toEqual([]);
		expect(judge(report(healthy(), "ok")).code).toBe(0);
	});

	test("one torn capture among 600 fails and names the pattern and the capture", () => {
		const run = healthy({ patterns: [pattern("ink-no-2026", { torn: [212] }), ...PATTERNS.slice(1).map((p) => pattern(p.id))] });
		const bad = check(run);
		expect(bad).toHaveLength(1);
		expect(bad[0]).toMatch(/ink-no-2026: 1 torn frames.*capture 212/);
		const r = judge(report(run, "torn"));
		expect(r.code).toBe(1);
		expect(r.err).toMatch(/torn frames/);
	});

	test("blips, blank frames and flashing are each reported for their pattern", () => {
		const ps = [pattern(PATTERNS[0]!.id, { blips: 2, blanks: 1, flashes: 3 }), ...PATTERNS.slice(1).map((p) => pattern(p.id))];
		const bad = check(healthy({ patterns: ps })).join("\n");
		expect(bad).toMatch(/2 blips/);
		expect(bad).toMatch(/1 blank frames/);
		expect(bad).toMatch(/3 flashing windows/);
	});

	test("a renderer that draws once per read (400 fps on a 60 Hz display) fails the coalescing line", () => {
		const ps = [pattern("ratatui-diff", { frames: 4000, seconds: 10 }), ...PATTERNS.slice(1).map((p) => pattern(p.id))];
		expect(check(healthy({ patterns: ps }))[0]).toMatch(/ratatui-diff: drew 400\.0 fps on a 60 Hz display/);
	});

	test("a renderer that stalls under the flood (5 fps) fails, and so does a late p95 interval", () => {
		const stalled = [pattern("ink-2026", { frames: 50, seconds: 10 }), ...PATTERNS.slice(1).map((p) => pattern(p.id))];
		expect(check(healthy({ patterns: stalled })).join()).toMatch(/ink-2026: drew 5\.0 fps/);
		const late = [pattern("ink-2026", { intervalP95Us: 40_000 }), ...PATTERNS.slice(1).map((p) => pattern(p.id))];
		expect(check(healthy({ patterns: late })).join()).toMatch(/interval p95 40\.00 ms > 18\.33 ms/);
	});

	test("a producer that printed nothing, or a recording with no captures, is not a pass", () => {
		const ps = [pattern("ink-2026", { bytes: 0 }), pattern("ratatui-diff", { captures: 2 }), ...PATTERNS.slice(2).map((p) => pattern(p.id))];
		const bad = check(healthy({ patterns: ps })).join("\n");
		expect(bad).toMatch(/ink-2026: the producer printed nothing/);
		expect(bad).toMatch(/ratatui-diff: only 2 captures/);
		expect(check(healthy({ patterns: [] })).join()).toMatch(/no pattern was measured/);
	});

	test("a scroll that moved the wrong number of rows, lost a wheel report, reordered or yanked the viewport is named", () => {
		const s = healthy().scroll!;
		expect(check(healthy({ scroll: { ...s, rowsMoved: 90 } })).join()).toMatch(/scroll moved 90 rows for 120 asked/);
		expect(check(healthy({ scroll: { ...s, reportsSeen: 38 } })).join()).toMatch(/counted 38 wheel reports for 40 notches/);
		expect(check(healthy({ scroll: { ...s, reportsInOrder: false } })).join()).toMatch(/out of order/);
		expect(check(healthy({ scroll: { ...s, viewportHeld: false } })).join()).toMatch(/pulled back.*#97/);
		const slow = { ...s, momentumIntervalsUs: Array.from({ length: 100 }, (_, i) => (i % 5 === 0 ? 60_000 : 16_700)) };
		expect(check(healthy({ scroll: slow })).join()).toMatch(/momentum frame interval p95/);
	});

	test("a scroll leg that never ran, an app that could not be driven, and one that asked for no rows fail", () => {
		expect(check(healthy({ scroll: null })).join()).toMatch(/scroll leg did not run/);
		expect(check(healthy({ undriven: ["ink-2026: no terminal opened"] })).join()).toMatch(/could not run: ink-2026/);
		const s = healthy().scroll!;
		expect(check(healthy({ scroll: { ...s, rowsAsked: 0, rowsMoved: 0 } })).join()).toMatch(/asked for no rows/);
	});

	test("the refresh rate sets the budget: 8.4 ms intervals pass on 120 Hz and fail the same run judged at 60 Hz as a coalescing breach", () => {
		const s = healthy().scroll!;
		const fast = healthy({
			hz: 120,
			patterns: PATTERNS.map((p) => pattern(p.id, { frames: 1200, intervalP95Us: 8_400 })),
			scroll: { ...s, momentumIntervalsUs: Array.from({ length: 100 }, () => 8_400) },
		});
		expect(check(fast)).toEqual([]);
		// The same measurements held to a 60 Hz display: 120 fps is twice what the display can show.
		expect(check({ ...fast, hz: 60 }).join()).toMatch(/drew 120\.0 fps on a 60 Hz display/);
		expect(BUDGET.intervalP95).toBe(1.1);
	});
});

describe("a report is a file anyone can edit or truncate", () => {
	test("a report that is not a report, or whose numbers are not numbers, is rejected rather than passing", () => {
		for (const [name, raw] of [
			["array", []],
			["no-hz", { patterns: [], scroll: null, undriven: [] }],
			["nan", { hz: 60, patterns: [{ ...pattern("a"), frames: "x" }], scroll: null, undriven: [] }],
			["torn-str", { hz: 60, patterns: [{ ...pattern("a"), torn: "none" }], scroll: null, undriven: [] }],
			["no-undriven", { hz: 60, patterns: [], scroll: null }],
		] as const) {
			expect("error" in parseRun(raw)).toBe(true);
			const r = judge(report(raw, name));
			expect(r.code).toBe(1);
		}
	});

	test("a saved run round-trips through parseRun and summarize", () => {
		const parsed = parseRun(JSON.parse(JSON.stringify({ run: healthy() })));
		expect("run" in parsed).toBe(true);
		expect(summarize(healthy())).toHaveLength(PATTERNS.length);
		expect(summarize(healthy())[0]).toMatchObject({ fps: 60, intervalP95Ms: 16.7, torn: 0 });
	});

	test("--no-check re-judges without failing the exit code", () => {
		const run = healthy({ scroll: null });
		expect(judge(report(run, "nocheck")).code).toBe(1);
		expect(judge(report(run, "nocheck"), "--no-check").code).toBe(0);
	});
});

describe("the torn-frame oracle reads what the generator paints", () => {
	test("every frame colour maps back to its own index and text or background maps to none", () => {
		FRAME_COLORS.forEach((c, i) => expect(colorIndex(c)).toBe(i));
		expect(colorIndex([20, 20, 24])).toBe(-1);
		expect(colorIndex([235, 235, 235])).toBe(-1);
	});

	test("one id on every row is a whole frame; two ids on one capture is a torn one; empty rows are ignored", () => {
		expect(isTorn([2, 2, 2, 2])).toBe(false);
		expect(isTorn([2, -1, 2, -1])).toBe(false);
		expect(isTorn([3, 3, 4, 4])).toBe(true);
		expect(isTorn([-1, -1])).toBe(false);
		expect(tornFrames([[1, 1], [1, 2], [3, 3], [4, 3]])).toEqual([1, 3]);
	});
});

describe("the six patterns", () => {
	test("the list covers what the study names, each with a frame generator and a stated reason", () => {
		expect(PATTERNS.map((p) => p.id)).toEqual(["ink-2026", "ink-no-2026", "ratatui-diff", "omp-boxes", "slow-2026", "scroll-stream"]);
		for (const p of PATTERNS) expect(p.covers.length).toBeGreaterThan(10);
	});

	test("2026 patterns wrap each frame in BSU/ESU, non-2026 ones do not, and the slow one splits its frame across a gap", () => {
		const has = (id: string, s: string) => frameBytes(id, 0, 40, 10).includes(s);
		expect(has("ink-2026", "\x1b[?2026h") && has("ink-2026", "\x1b[?2026l")).toBe(true);
		expect(has("ink-no-2026", "\x1b[?2026h")).toBe(false);
		expect(has("ratatui-diff", "\x1b[?2026h")).toBe(true);
		expect(has("omp-boxes", "\x1b[?2026h")).toBe(true);
		const slow = frameBytes("slow-2026", 0, 40, 10);
		expect(slow).toContain("\x1b[?2026h");
		expect(slow).toContain("\x00GAP"); // a marker the generator turns into a >150 ms pause between halves of one frame
	});

	test("an ink redraw erases and rewrites the last N lines in place (CUU + EL), the shape that tears without 2026", () => {
		const f = frameBytes("ink-no-2026", 3, 40, 10);
		expect(f).toMatch(/\x1b\[\d+A/);
		expect(f).toContain("\x1b[2K");
	});
});
