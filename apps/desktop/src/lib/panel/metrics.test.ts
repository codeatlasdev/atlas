// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	LogCursor,
	SLOTS,
	STEP_MS,
	ServiceHistory,
	Track,
	bandOf,
	fmtUptime,
	stackSum,
	summarize,
	sumLatest,
	windowSum,
} from "./metrics.ts";

describe("Track", () => {
	test("last() returns oldest-first with zeros for missing history", () => {
		const t = new Track();
		t.set(10, 5);
		t.set(11, 7);
		expect(t.last(4)).toEqual([0, 0, 5, 7]);
	});

	test("skipped slots are zero-filled, or carried for gauges", () => {
		const a = new Track();
		a.set(0, 4);
		a.set(3, 9);
		expect(a.last(4)).toEqual([4, 0, 0, 9]);
		const b = new Track();
		b.set(0, 4);
		b.set(3, 9, true);
		expect(b.last(4)).toEqual([4, 4, 4, 9]);
	});

	test("a gap longer than the window never leaves stale data", () => {
		const t = new Track();
		t.set(0, 100);
		t.set(SLOTS * 3, 1);
		expect(
			t
				.last(SLOTS)
				.slice(0, -1)
				.every((v) => v === 0),
		).toBe(true);
		expect(t.last(1)).toEqual([1]);
	});

	test("old slots are overwritten once the ring wraps", () => {
		const t = new Track();
		for (let s = 0; s < SLOTS + 5; s++) t.set(s, s);
		expect(t.last(1)).toEqual([SLOTS + 4]);
		expect(t.last(SLOTS)[0]).toBe(5);
	});

	test("add() accumulates within a slot and opens new ones at zero", () => {
		const t = new Track();
		t.add(2, 1);
		t.add(2, 1);
		t.add(3, 1);
		expect(t.last(2)).toEqual([2, 1]);
	});

	test("writes to a slot older than head are ignored", () => {
		const t = new Track();
		t.set(5, 1);
		t.set(3, 99);
		expect(t.last(1)).toEqual([1]);
	});
});

describe("aggregates", () => {
	test("sumLatest ignores empty tracks", () => {
		const a = new Track();
		const b = new Track();
		a.set(1, 10);
		expect(sumLatest([a, b])).toBe(10);
	});

	test("stackSum adds tracks element-wise on a shared end", () => {
		const a = new Track();
		const b = new Track();
		a.set(1, 1);
		a.set(2, 2);
		b.set(2, 5);
		expect(stackSum([a, b], 2, 2)).toEqual([1, 7]);
	});

	test("windowSum covers the requested span only", () => {
		const t = new Track();
		for (let s = 0; s < 10; s++) t.set(s, 1);
		expect(windowSum(t, 3 * STEP_MS, 9)).toBe(3);
	});
});

describe("ServiceHistory.state", () => {
	test("merges equal consecutive bands and closes the previous one", () => {
		const h = new ServiceHistory();
		h.state("starting", 0);
		h.state("starting", 10);
		h.state("running", 20);
		expect(h.segments).toEqual([
			{ band: "starting", start: 0, end: 20 },
			{ band: "running", start: 20, end: null },
		]);
	});

	test("drops segments that fell out of the window but keeps the current one", () => {
		const h = new ServiceHistory();
		h.state("running", 0);
		h.state("failed", 1000);
		const far = SLOTS * STEP_MS * 2;
		h.state("running", far);
		expect(h.segments.map((s) => s.band)).toEqual(["failed", "running"]);
	});

	test("bandOf folds unknown states into stopped", () => {
		expect(bandOf("disabled")).toBe("stopped");
		expect(bandOf("running")).toBe("running");
	});
});

describe("LogCursor", () => {
	const ring = (items, base = 0) => ({ items, base });

	test("reads only appended lines", () => {
		const c = new LogCursor();
		const seen = [];
		const r = ring(["a", "b"]);
		expect(c.read(r, (l) => seen.push(l))).toBe(2);
		r.items.push("c");
		expect(c.read(r, (l) => seen.push(l))).toBe(1);
		expect(seen).toEqual(["a", "b", "c"]);
	});

	test("jumps over lines the ring trimmed before we saw them", () => {
		const c = new LogCursor();
		const seen = [];
		c.read(ring(["a", "b"]), () => {});
		// ring trimmed 5 lines: base 5, we were at 2
		c.read(ring(["x", "y"], 5), (l) => seen.push(l));
		expect(seen).toEqual(["x", "y"]);
		expect(c.next).toBe(7);
	});

	test("reset forgets the position after a resync replaced the ring", () => {
		const c = new LogCursor();
		c.read(ring(["a", "b", "c"]), () => {});
		c.reset();
		const seen = [];
		c.read(ring(["z"]), (l) => seen.push(l));
		expect(seen).toEqual(["z"]);
	});
});

describe("formatting and summary", () => {
	test("fmtUptime scales units", () => {
		expect(fmtUptime(5_000)).toBe("5s");
		expect(fmtUptime(125_000)).toBe("2m 5s");
		expect(fmtUptime(3_900_000)).toBe("1h 5m");
		expect(fmtUptime(90_000_000)).toBe("1d 1h");
		expect(fmtUptime(null)).toBe("—");
		expect(fmtUptime(-1)).toBe("—");
	});

	test("summarize prioritizes failures, then starting", () => {
		expect(summarize(["running", "failed", "starting"], true)).toEqual({ tone: "fail", label: "1 failed" });
		expect(summarize(["running", "starting"], true)).toEqual({ tone: "warn", label: "1 starting" });
		expect(summarize(["running", "running"], true)).toEqual({ tone: "ok", label: "All healthy" });
		expect(summarize(["running", "stopped"], true)).toEqual({ tone: "warn", label: "1 of 2 up" });
		expect(summarize(["running"], false)).toEqual({ tone: "off", label: "Stopped" });
	});
});
