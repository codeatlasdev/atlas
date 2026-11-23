// bun scripts/term-parser-gate.ts [--baseline docs/perf/terminal-linux-baseline.json] [--absolute] [--passes 5] [--mib 16]
//                                 [--report <bench.json>] [--write-baseline] [--save <bench.json>]
//
// The CPU-only parser gate (design: docs/terminal-linux-design.md section 7.6, numbers: docs/perf/terminal-linux.md). Runs
// `cargo bench -p atlas-term --bench vt_parse` (release, needs bun for the corpora), compares each corpus's
// MB/s-over-calibration ratio with the committed baseline (scripts/term-parser-judge.ts) and exits 1 on a regression.
// `--report` judges a saved bench output with no cargo. `--absolute` also enforces the per-corpus MB/s floors, which only
// mean something on the machine that wrote the baseline (run it there, with the load under 2). `--write-baseline` writes
// a new baseline from this run (the machine fields come from the environment: TERM_BASELINE_NOTE).
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { type BenchReport, judge, makeBaseline } from "./term-parser-judge.ts";

const root = dirname(import.meta.dir);
const arg = (k: string, d?: string) => {
	const i = process.argv.indexOf(`--${k}`);
	return i >= 0 ? process.argv[i + 1] : d;
};
const flag = (k: string) => process.argv.includes(`--${k}`);

const baselinePath = arg("baseline", join(root, "docs/perf/terminal-linux-baseline.json"))!;

let report: BenchReport;
const given = arg("report");
if (given) report = JSON.parse(readFileSync(given, "utf8"));
else {
	const out = arg("save", join(process.env.TMPDIR ?? "/tmp", `term-parser-bench-${process.pid}.json`))!;
	const p = Bun.spawnSync(["cargo", "bench", "-p", "atlas-term", "--bench", "vt_parse", "--locked", "--", "--json", out, "--passes", arg("passes", "5")!, "--mib", arg("mib", "16")!], {
		cwd: root,
		stdout: "inherit",
		stderr: "inherit",
	});
	if (p.exitCode !== 0) {
		console.error("term-parser-gate: the bench did not run");
		process.exit(2);
	}
	report = JSON.parse(readFileSync(out, "utf8"));
}

if (flag("write-baseline")) {
	const b = makeBaseline(report, { note: process.env.TERM_BASELINE_NOTE ?? "", os: report.os, mib: report.mib });
	writeFileSync(baselinePath, `${JSON.stringify(b, null, "\t")}\n`);
	console.error(`wrote ${baselinePath}`);
	process.exit(0);
}

const v = judge(JSON.parse(readFileSync(baselinePath, "utf8")), report, { absolute: flag("absolute") });
console.error(v.lines.join("\n"));
console.error(v.ok ? "term-parser-gate: ok" : "term-parser-gate: FAILED");
process.exit(v.ok ? 0 : 1);
