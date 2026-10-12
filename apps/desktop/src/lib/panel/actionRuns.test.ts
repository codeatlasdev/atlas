// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	MAX_RUNS,
	MAX_RUN_LINES,
	abandon,
	activeRun,
	duration,
	emptyRuns,
	finish,
	mergeSnapshot,
	pushLine,
} from "./actionRuns.ts";

const T0 = 1_700_000_000_000;
const texts = (run) => run.lines.items.map((l) => l.text);

describe("buffering", () => {
	test("the first line starts a run, later lines of the label append to it", () => {
		const runs = emptyRuns();
		const a = pushLine(runs, "Migrate", "step 1", T0);
		const b = pushLine(runs, "Migrate", "step 2", T0 + 10);
		expect(a.started).toBe(true);
		expect(b.started).toBe(false);
		expect(runs.list).toHaveLength(1);
		expect(texts(runs.list[0])).toEqual(["step 1", "step 2"]);
		expect(runs.list[0]).toMatchObject({ label: "Migrate", status: "running", start: T0, end: null });
	});

	test("two labels in flight keep separate buffers", () => {
		const runs = emptyRuns();
		pushLine(runs, "A", "a1", T0);
		pushLine(runs, "B", "b1", T0 + 1);
		pushLine(runs, "A", "a2", T0 + 2);
		expect(runs.list.map((r) => [r.label, texts(r)])).toEqual([
			["A", ["a1", "a2"]],
			["B", ["b1"]],
		]);
	});

	test("every change bumps rev", () => {
		const runs = emptyRuns();
		const r0 = runs.rev;
		pushLine(runs, "A", "x", T0);
		const r1 = runs.rev;
		finish(runs, "A", true, T0 + 1);
		expect(r1).toBeGreaterThan(r0);
		expect(runs.rev).toBeGreaterThan(r1);
	});

	test("a run keeps only its last MAX_RUN_LINES lines", () => {
		const runs = emptyRuns();
		const n = MAX_RUN_LINES + 1000;
		for (let i = 0; i < n; i++) pushLine(runs, "Big", `l${i}`, T0);
		const lines = runs.list[0].lines;
		// The ring trims in slabs: it never holds more than cap + slack and always ends at the newest line.
		expect(lines.items.length).toBeLessThanOrEqual(MAX_RUN_LINES + lines.slack);
		expect(lines.items.length).toBeGreaterThanOrEqual(MAX_RUN_LINES);
		expect(lines.items.at(-1).text).toBe(`l${n - 1}`);
		expect(lines.base + lines.items.length).toBe(n);
	});
});

describe("status transitions", () => {
	test("running → ok / failed with an end time", () => {
		const runs = emptyRuns();
		pushLine(runs, "A", "x", T0);
		pushLine(runs, "B", "y", T0);
		expect(finish(runs, "A", true, T0 + 5000)).toMatchObject({ status: "ok", end: T0 + 5000 });
		expect(finish(runs, "B", false, T0 + 7000)).toMatchObject({ status: "failed", end: T0 + 7000 });
		expect(activeRun(runs, "A")).toBeUndefined();
	});

	test("the same label after completion is a new run", () => {
		const runs = emptyRuns();
		pushLine(runs, "A", "first", T0);
		finish(runs, "A", false, T0 + 1);
		const second = pushLine(runs, "A", "second", T0 + 2);
		expect(second.started).toBe(true);
		expect(runs.list.map((r) => [r.status, texts(r)])).toEqual([
			["failed", ["first"]],
			["running", ["second"]],
		]);
		expect(runs.list[1].id).toBeGreaterThan(runs.list[0].id);
	});

	test("a silent action still produces a finished run", () => {
		const runs = emptyRuns();
		const run = finish(runs, "Quiet", true, T0);
		expect(run).toMatchObject({ label: "Quiet", status: "ok", start: T0, end: T0 });
		expect(runs.list).toHaveLength(1);
	});

	test("the empty-label resume ping is not a run", () => {
		const runs = emptyRuns();
		expect(finish(runs, "", true, T0)).toBeNull();
		expect(runs.list).toHaveLength(0);
	});

	test("abandon fails only the runs still in flight", () => {
		const runs = emptyRuns();
		pushLine(runs, "Done", "x", T0);
		finish(runs, "Done", true, T0 + 1);
		pushLine(runs, "Live", "y", T0 + 2);
		abandon(runs, T0 + 9);
		expect(runs.list.map((r) => [r.label, r.status, r.end])).toEqual([
			["Done", "ok", T0 + 1],
			["Live", "failed", T0 + 9],
		]);
	});
});

describe("rotation", () => {
	test("only the newest MAX_RUNS runs are kept, oldest dropped first", () => {
		const runs = emptyRuns();
		for (let i = 0; i < MAX_RUNS + 4; i++) {
			pushLine(runs, `run${i}`, "x", T0 + i);
			finish(runs, `run${i}`, true, T0 + i);
		}
		expect(runs.list).toHaveLength(MAX_RUNS);
		expect(runs.list[0].label).toBe("run4");
		expect(runs.list.at(-1).label).toBe(`run${MAX_RUNS + 3}`);
	});

	test("ids keep ascending across rotation", () => {
		const runs = emptyRuns();
		for (let i = 0; i < MAX_RUNS + 2; i++) pushLine(runs, `r${i}`, "x", T0 + i);
		const ids = runs.list.map((r) => r.id);
		expect(ids).toEqual([...ids].sort((a, b) => a - b));
		expect(new Set(ids).size).toBe(ids.length);
	});
});

describe("snapshot replay merge", () => {
	test("an older runner (no action_runs) leaves local state untouched", () => {
		const runs = emptyRuns();
		pushLine(runs, "A", "x", T0);
		mergeSnapshot(runs, undefined, T0 + 1);
		expect(runs.list).toHaveLength(1);
		expect(runs.list[0].status).toBe("running");
	});

	test("attaching mid-run shows the tail the runner kept, then live lines continue it", () => {
		const runs = emptyRuns();
		mergeSnapshot(
			runs,
			[{ label: "Migrate", start: T0, status: "running", end: null, lines: ["a", "b", "c"], skipped: 40 }],
			T0 + 1000,
		);
		expect(runs.list).toHaveLength(1);
		expect(runs.list[0]).toMatchObject({ label: "Migrate", status: "running", start: T0, truncated: 40 });
		expect(texts(runs.list[0])).toEqual(["a", "b", "c"]);

		// A live line after the snapshot joins the same in-flight run.
		const live = pushLine(runs, "Migrate", "d", T0 + 2000);
		expect(live.started).toBe(false);
		expect(texts(runs.list[0])).toEqual(["a", "b", "c", "d"]);
		expect(finish(runs, "Migrate", true, T0 + 3000)).toMatchObject({ status: "ok" });
	});

	test("finished runs from the snapshot keep their status and end", () => {
		const runs = emptyRuns();
		mergeSnapshot(
			runs,
			[
				{ label: "Seed", start: T0, status: "failed", end: T0 + 4000, lines: ["boom"] },
				{ label: "Build", start: T0 + 10_000, status: "ok", end: T0 + 12_000, lines: [] },
			],
			T0 + 20_000,
		);
		expect(runs.list.map((r) => [r.label, r.status, r.end])).toEqual([
			["Seed", "failed", T0 + 4000],
			["Build", "ok", T0 + 12_000],
		]);
	});

	test("a resync does not duplicate a run already held and never shrinks its buffer", () => {
		const runs = emptyRuns();
		for (const l of ["1", "2", "3", "4"]) pushLine(runs, "A", l, T0);
		mergeSnapshot(runs, [{ label: "A", start: T0 + 500, status: "running", lines: ["3", "4"] }], T0 + 1000);
		expect(runs.list).toHaveLength(1);
		expect(texts(runs.list[0])).toEqual(["1", "2", "3", "4"]);
	});

	test("a held run that finished while away adopts the snapshot result", () => {
		const runs = emptyRuns();
		pushLine(runs, "A", "1", T0);
		mergeSnapshot(
			runs,
			[{ label: "A", start: T0, status: "ok", end: T0 + 3000, lines: ["1", "2"] }],
			T0 + 9000,
		);
		expect(runs.list[0]).toMatchObject({ status: "ok", end: T0 + 3000 });
		expect(texts(runs.list[0])).toEqual(["1", "2"]);
	});

	test("a local run the runner no longer lists is closed as failed", () => {
		const runs = emptyRuns();
		pushLine(runs, "Gone", "x", T0);
		mergeSnapshot(runs, [], T0 + 5000);
		expect(runs.list[0]).toMatchObject({ status: "failed", end: T0 + 5000 });
	});

	test("snapshot-only runs go before local ones in start order, capped at MAX_RUNS", () => {
		const runs = emptyRuns();
		pushLine(runs, "Local", "x", T0 + 100_000);
		const snap = Array.from({ length: MAX_RUNS + 2 }, (_, i) => ({
			label: `s${i}`,
			start: T0 + i * 1000,
			status: "ok",
			end: T0 + i * 1000 + 1,
			lines: [],
		}));
		mergeSnapshot(runs, snap, T0 + 200_000);
		expect(runs.list).toHaveLength(MAX_RUNS);
		expect(runs.list.at(-1).label).toBe("Local");
		const starts = runs.list.map((r) => r.start);
		expect(starts).toEqual([...starts].sort((a, b) => a - b));
		const ids = runs.list.map((r) => r.id);
		expect(new Set(ids).size).toBe(ids.length);
		expect(ids.every((i) => i > 0)).toBe(true);
	});
});

describe("duration", () => {
	test("seconds, minutes and hours; running runs count to now", () => {
		expect(duration({ start: T0, end: T0 + 12_000 }, T0)).toBe("12s");
		expect(duration({ start: T0, end: T0 + 184_000 }, T0)).toBe("3m 04s");
		expect(duration({ start: T0, end: T0 + 3_720_000 }, T0)).toBe("1h 02m");
		expect(duration({ start: T0, end: null }, T0 + 5000)).toBe("5s");
		expect(duration({ start: T0, end: null }, T0 - 5000)).toBe("0s");
	});
});
