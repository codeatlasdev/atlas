import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "bun:test";
import { type Baseline, type BenchReport, type CorpusReport, judge, makeBaseline } from "./term-parser-judge.ts";

const c = (corpus: string, ratio: number, mbps = ratio * 1000, load = 0.5): CorpusReport => ({ corpus, bytes: 1 << 20, best_mbps: mbps, median_mbps: mbps, ratio, load_before: load, load_after: load });
const report = (...corpora: CorpusReport[]): BenchReport => ({ cpu: "test", os: "linux", mib: 16, calibration_mbps: 1000, corpora });
const base: Baseline = {
	tolerance: 0.3,
	machine: {},
	corpora: { seq: { best_mbps: 100, median_mbps: 100, ratio: 0.1, floor_mbps: 85 }, ascii: { best_mbps: 90, median_mbps: 90, ratio: 0.09, floor_mbps: 76 } },
};

test("a ratio inside the tolerance passes, one under it fails and names the corpus", () => {
	expect(judge(base, report(c("seq", 0.1), c("ascii", 0.07))).ok).toBe(true); // -22 %
	const v = judge(base, report(c("seq", 0.1), c("ascii", 0.06))); // -33 %
	expect(v.ok).toBe(false);
	expect(v.lines.find((l) => l.startsWith("FAIL"))).toContain("ascii");
});

test("the tolerance edge is inclusive: exactly baseline * (1 - tolerance) passes", () => {
	expect(judge(base, report(c("seq", 0.07), c("ascii", 0.063))).ok).toBe(true);
});

test("a corpus the bench no longer measures fails; one the baseline lacks fails", () => {
	expect(judge(base, report(c("seq", 0.1))).lines.join("\n")).toContain("ascii: in the baseline but not measured");
	expect(judge(base, report(c("seq", 0.1), c("ascii", 0.09), c("sgr", 0.1))).ok).toBe(false);
});

test("a machine twice as fast passes because the ratio, not MB/s, is compared", () => {
	// The same parser on a CPU 2x faster: MB/s doubles, calibration doubles, ratio holds.
	expect(judge(base, report(c("seq", 0.1, 200), c("ascii", 0.09, 180))).ok).toBe(true);
});

test("--absolute enforces the MB/s floor of the reference machine", () => {
	const r = report(c("seq", 0.1, 80), c("ascii", 0.09, 90));
	expect(judge(base, r).ok).toBe(true);
	const v = judge(base, r, { absolute: true });
	expect(v.ok).toBe(false);
	expect(v.lines.join("\n")).toContain("seq: 80.0 MB/s is under its floor 85");
});

test("a noisy load is shown on the line and a faster result asks for a new baseline", () => {
	const v = judge(base, report(c("seq", 0.2, 200, 2.4), c("ascii", 0.09)));
	expect(v.ok).toBe(true);
	expect(v.lines[0]).toContain("noisy");
	expect(v.lines[0]).toContain("raise it");
});

test("makeBaseline round-trips through judge on the report it came from", () => {
	const r = report(c("seq", 0.1, 100), c("ascii", 0.09, 90));
	const b = makeBaseline(r, { driver: "none" });
	expect(b.corpora.seq!.floor_mbps).toBe(85);
	expect(judge(b, r, { absolute: true }).ok).toBe(true);
});

// The script itself, as CI runs it (`--report` skips cargo): the committed baseline passes the committed run it was made
// from, and the same run with one corpus's throughput halved exits 1 naming the corpus.
test("the gate script exits 0 on the run the baseline came from and 1 when one corpus is halved", () => {
	const root = join(import.meta.dir, "..");
	const run1 = JSON.parse(readFileSync(join(root, "docs/perf/terminal-linux-parser.jsonl"), "utf8").trim().split("\n")[2]!) as BenchReport;
	const gate = (r: BenchReport) => {
		const f = join(mkdtempSync(join(tmpdir(), "gate-")), "bench.json");
		writeFileSync(f, JSON.stringify(r));
		return Bun.spawnSync(["bun", join(root, "scripts/term-parser-gate.ts"), "--report", f], { stderr: "pipe" });
	};
	expect(gate(run1).exitCode).toBe(0);
	const slow = { ...run1, corpora: run1.corpora.map((c) => (c.corpus === "ascii" ? { ...c, best_mbps: c.best_mbps / 2, ratio: c.ratio / 2 } : c)) };
	const bad = gate(slow);
	expect(bad.exitCode).toBe(1);
	expect(bad.stderr.toString()).toMatch(/FAIL ascii:/);
});
