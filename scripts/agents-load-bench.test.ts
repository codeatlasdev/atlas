// Incident (#110): "the terminal freezes out of nowhere with 7+ agents". The bench that measures it must itself be
// trustworthy: a run that measured nothing, a crashed app or a hand-edited report must never read as a pass, and each
// failure the user described must trip its own budget line. `--judge <report.json>` re-judges a saved report without
// the app; these tests run the real script as a process and check its exit code and message.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { BUDGET, check, growth, parseLoad, pct, summarize, type Load } from "./agents-load-judge.ts";

const script = join(import.meta.dir, "agents-load-bench.ts");
const dir = mkdtempSync(join(tmpdir(), "agents-load-"));

/** A healthy 7-agent minute: 30 fps terminals, a calm UI thread, flat memory, a few events a second. */
const healthy = (o: Partial<Load> = {}): Load => ({
	stallMs: Array.from({ length: 30 }, () => 4),
	frameGapMs: Array.from({ length: 300 }, () => 16.7),
	terms: Array.from({ length: 7 }, (_, i) => ({ tab: `t${i}`, framesStart: 0, framesEnd: 30 * 30, bytes: 5_000_000 })),
	rssMb: { app: [300, 310], hostd: [50, 52], web: [400, 420] },
	statusEvents: 30,
	hostEvents: 60,
	seconds: 30,
	...o,
});
const report = (load: unknown, name: string) => {
	const p = join(dir, `${name}.json`);
	writeFileSync(p, JSON.stringify({ load }));
	return p;
};
const judge = (path: string, ...extra: string[]) => {
	const r = Bun.spawnSync(["bun", script, "--judge", path, ...extra], { stderr: "pipe", stdout: "pipe" });
	return { code: r.exitCode, err: r.stderr.toString() };
};

describe("each way the app freezes trips its own line", () => {
	test("a healthy run passes", () => {
		expect(check(healthy())).toEqual([]);
		expect(judge(report(healthy(), "ok")).code).toBe(0);
	});

	test("a UI thread that stalls (one 400 ms second among calm ones) fails on the max, not only on p95", () => {
		const stallMs = Array.from({ length: 30 }, (_, i) => (i === 7 ? 400 : 4));
		const bad = check(healthy({ stallMs }));
		expect(bad).toHaveLength(1);
		expect(bad[0]).toMatch(/stall max 400 ms/);
		const r = judge(report(healthy({ stallMs }), "stall"));
		expect(r.code).toBe(1);
		expect(r.err).toMatch(/stall max/);
	});

	test("a page that paints late (p95 rAF gap over two dropped frames) fails", () => {
		const frameGapMs = Array.from({ length: 100 }, (_, i) => (i % 10 === 0 ? 120 : 16.7));
		expect(check(healthy({ frameGapMs })).join()).toMatch(/frame gap p95/);
	});

	test("a terminal whose renderer stopped while its program kept printing is named", () => {
		const terms = healthy().terms.map((t, i) => (i === 3 ? { ...t, framesEnd: t.framesStart } : t));
		const bad = check(healthy({ terms }));
		expect(bad).toHaveLength(1);
		expect(bad[0]).toMatch(/terminal t3 presented 0 fps/);
	});

	test("a terminal whose program printed nothing owes no frames (an idle tab is not a freeze)", () => {
		const terms = healthy().terms.map((t, i) => (i === 3 ? { ...t, framesEnd: t.framesStart, bytes: 0 } : t));
		expect(check(healthy({ terms }))).toEqual([]);
	});

	test("memory climbing behind the stream is named per process", () => {
		const bad = check(healthy({ rssMb: { app: [300, 310], hostd: [50, 900], web: [400, 420] } }));
		expect(bad).toEqual([expect.stringMatching(/^hostd RSS grew 850 MB/)]);
	});

	test("an event storm (page events per agent per second over the limit) fails", () => {
		const bad = check(healthy({ statusEvents: 3000, hostEvents: 3000 }));
		expect(bad.join()).toMatch(/page events per agent per second/);
	});
});

describe("a bench that did not exercise the app is not a pass", () => {
	// Incident class (selection-bench): a crashed app measured nothing and reported a spectacular pass.
	test("no terminals opened", () => {
		expect(check(healthy({ terms: [] }))).toEqual(["no terminals were opened: nothing was measured"]);
		expect(judge(report(healthy({ terms: [] }), "none")).code).toBe(1);
	});
	test("no main-thread samples", () => {
		expect(check(healthy({ stallMs: [] }))).toEqual(["no main-thread samples were taken"]);
	});
	test("--no-check reports but does not fail", () => {
		expect(judge(report(healthy({ terms: [] }), "nocheck"), "--no-check").code).toBe(0);
	});
});

describe("a saved report that is not a report is refused, never judged as NaN", () => {
	// A list holding a string sorts and compares as NaN, which is `> budget` == false for every line: a pass.
	const refused = (load: unknown, why: RegExp) => {
		const r = judge(report(load, `bad-${why.source.length}`));
		expect(r.code).toBe(1);
		expect(r.err).toMatch(why);
	};
	test("missing load", () => {
		const p = join(dir, "noload.json");
		writeFileSync(p, "{}");
		const r = judge(p);
		expect(r.code).toBe(1);
		expect(r.err).toMatch(/no `load`/);
	});
	test("a stall list with a string in it", () => refused({ ...healthy(), stallMs: [4, "x"] }, /lists of numbers/));
	test("terms without a byte count", () => refused({ ...healthy(), terms: [{ tab: "t", framesStart: 0, framesEnd: 1 }] }, /terms/));
	test("rss without hostd", () => refused({ ...healthy(), rssMb: { app: [1], web: [1] } }, /rssMb/));
	test("a missing field is named", () => {
		const { seconds: _s, ...rest } = healthy();
		refused(rest, /load\.seconds/);
	});
	test("a real report round-trips through the guard unchanged", () => {
		const l = healthy();
		expect(parseLoad({ load: l })).toEqual({ load: l });
	});
});

describe("the statistics", () => {
	test("nearest-rank percentiles; an empty list is 0 so a bench that measured nothing cannot pass", () => {
		expect(pct([], 0.95)).toBe(0);
		expect(pct([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.5)).toBe(5);
		expect(pct([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.95)).toBe(10);
		expect(pct([7], 0.95)).toBe(7);
	});
	test("growth is last minus first, and 0 for fewer than two samples", () => {
		expect(growth([300, 250, 340])).toBe(40);
		expect(growth([300])).toBe(0);
		expect(growth([])).toBe(0);
	});
	test("fps is frames over the window, per terminal", () => {
		const s = summarize(healthy());
		expect(s.fps).toHaveLength(7);
		expect(s.fps[0]).toEqual({ tab: "t0", fps: 30, bytes: 5_000_000 });
		expect(s.agents).toBe(7);
	});
	test("the budget is the one the docs state", () => {
		expect(BUDGET).toEqual({ stallP95Ms: 16, stallMaxMs: 100, frameGapP95Ms: 34, minFps: 20, rssGrowthMb: 400, eventsPerAgentPerSec: 8 });
	});
});
