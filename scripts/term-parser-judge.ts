// The verdict of `scripts/term-parser-gate.ts`, pure so `bun test scripts` covers it and `--report <file>` needs no cargo.
//
// The bench (`crates/atlas-term/benches/vt_parse.rs`) reports, per corpus, `Vt::feed` MB/s and `ratio` = MB/s divided by the
// MB/s of a fixed scalar workload timed in the same process. A CI runner is not the machine that wrote the baseline, so the
// gate compares the ratio: a change of CPU moves both numbers, a change of the parser moves only the first. What the gate
// cannot see: a regression that also slows the calibration workload (a machine under load) is invisible, and a CPU whose
// branch predictor or store buffer differ in proportion from the baseline machine's shifts every ratio. The tolerance
// absorbs the second; the loads recorded per corpus show the first.

export interface CorpusReport {
	corpus: string;
	bytes: number;
	best_mbps: number;
	median_mbps: number;
	ratio: number;
	load_before: number;
	load_after: number;
}

export interface BenchReport {
	cpu: string;
	os: string;
	mib: number;
	calibration_mbps: number;
	corpora: CorpusReport[];
}

export interface BaselineCorpus {
	best_mbps: number;
	median_mbps: number;
	ratio: number;
	/** MB/s this corpus must not fall under on the reference machine (`--absolute`): replaces the roadmap's 500 MB/s. */
	floor_mbps: number;
}

export interface Baseline {
	/** Fraction a ratio may fall under the baseline's before the gate fails. */
	tolerance: number;
	machine: Record<string, unknown>;
	corpora: Record<string, BaselineCorpus>;
}

export interface Verdict {
	ok: boolean;
	lines: string[];
}

/** Loads at or over this make a measurement unfit as evidence (recorded, not judged: the gate still compares). */
export const NOISY_LOAD = 2;
export const DISCARD_LOAD = 4;

export function judge(baseline: Baseline, report: BenchReport, opts: { absolute?: boolean } = {}): Verdict {
	const lines: string[] = [];
	let ok = true;
	const seen = new Set<string>();
	for (const c of report.corpora) {
		seen.add(c.corpus);
		const b = baseline.corpora[c.corpus];
		if (!b) {
			lines.push(`FAIL ${c.corpus}: not in the baseline (add it with --write-baseline)`);
			ok = false;
			continue;
		}
		const min = b.ratio * (1 - baseline.tolerance);
		const load = Math.max(c.load_before, c.load_after);
		const note = load >= NOISY_LOAD ? ` [load ${load.toFixed(1)}: noisy]` : "";
		const rel = `${c.best_mbps.toFixed(1)} MB/s, ratio ${c.ratio.toFixed(4)} vs baseline ${b.ratio.toFixed(4)} (${(((c.ratio / b.ratio) - 1) * 100).toFixed(1)} %)`;
		if (c.ratio < min) {
			ok = false;
			lines.push(`FAIL ${c.corpus}: ${rel}, under the ${(baseline.tolerance * 100).toFixed(0)} % tolerance (min ${min.toFixed(4)})${note}`);
		} else if (c.ratio > b.ratio * (1 + baseline.tolerance)) {
			lines.push(`ok   ${c.corpus}: ${rel}, faster than the baseline allows for: raise it (--write-baseline)${note}`);
		} else lines.push(`ok   ${c.corpus}: ${rel}${note}`);
		if (opts.absolute && c.best_mbps < b.floor_mbps) {
			ok = false;
			lines.push(`FAIL ${c.corpus}: ${c.best_mbps.toFixed(1)} MB/s is under its floor ${b.floor_mbps} MB/s`);
		}
	}
	for (const name of Object.keys(baseline.corpora)) {
		if (!seen.has(name)) {
			ok = false;
			lines.push(`FAIL ${name}: in the baseline but not measured`);
		}
	}
	return { ok, lines };
}

/** A baseline from a report: ratios as measured, floors at `floorFraction` of the best MB/s (the margin under which a change is a regression on that machine). */
export function makeBaseline(report: BenchReport, machine: Record<string, unknown>, tolerance = 0.3, floorFraction = 0.85): Baseline {
	const corpora: Record<string, BaselineCorpus> = {};
	for (const c of report.corpora) {
		corpora[c.corpus] = {
			best_mbps: c.best_mbps,
			median_mbps: c.median_mbps,
			ratio: c.ratio,
			floor_mbps: Math.floor(c.best_mbps * floorFraction),
		};
	}
	return { tolerance, machine: { cpu: report.cpu, calibration_mbps: report.calibration_mbps, ...machine }, corpora };
}
