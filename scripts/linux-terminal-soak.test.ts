// scripts/linux-terminal-soak.ts decides whether a closed terminal gave its memory back (cause 6 of
// docs/perf/linux-webkit-study.md). A verdict that passed a leak, or failed a flat run, would make every later "closing a
// terminal frees its memory" claim false, so the two ways to fail are pinned against hand-made series.
import { describe, expect, test } from "bun:test";
import { judgeSoak, type Point } from "./linux-terminal-soak.ts";

const clean = { xterm: 0, canvas: 0 };
const base: Point = { label: "baseline", webPssMb: 205 };

describe("judgeSoak", () => {
	test("a run that ends within the budget of the idle baseline and holds no xterm object passes", () => {
		const pts: Point[] = [base, { label: "closed-1", webPssMb: 320, live: clean }, { label: "closed-2", webPssMb: 300, live: clean }, { label: "final", webPssMb: 290, live: clean }];
		expect(judgeSoak(pts, 100)).toEqual([]);
	});

	test("memory still over the baseline by more than the budget after the last close fails, and says by how much", () => {
		const pts: Point[] = [base, { label: "closed-1", webPssMb: 400, live: clean }, { label: "final", webPssMb: 725, live: clean }];
		const bad = judgeSoak(pts, 100);
		expect(bad).toHaveLength(1);
		expect(bad[0]).toContain("520 MB over the 205 MB idle baseline");
	});

	test("the budget is inclusive: exactly baseline + budget passes", () => {
		expect(judgeSoak([base, { label: "final", webPssMb: 305, live: clean }], 100)).toEqual([]);
	});

	test("a peak in the middle does not fail the run: only the end is held to the budget", () => {
		const pts: Point[] = [base, { label: "closed-1", webPssMb: 900, live: clean }, { label: "final", webPssMb: 250, live: clean }];
		expect(judgeSoak(pts, 100)).toEqual([]);
	});

	test("an xterm element or a canvas left in the page after a close fails even when the memory looks fine", () => {
		const pts: Point[] = [base, { label: "closed-3", webPssMb: 210, live: { xterm: 1, canvas: 2 } }, { label: "final", webPssMb: 210, live: clean }];
		const bad = judgeSoak(pts, 100);
		expect(bad).toHaveLength(1);
		expect(bad[0]).toContain("closed-3");
		expect(bad[0]).toContain("1 .xterm and 2 canvas");
	});

	test("a run with no cycle is a failure, not a pass", () => {
		expect(judgeSoak([base], 100)).toEqual(["no cycle was sampled"]);
		expect(judgeSoak([], 100)).toEqual(["no cycle was sampled"]);
	});
});
