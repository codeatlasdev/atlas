// Gate parity: the map (scripts/checks/map.ts) is the one list of checks, and `bun run check`, `bun run check:changed`
// and CI must agree with it. These tests fail when CI gains a job, a gated area or a check command the map does not
// know, or when the map names something CI does not run, with the line to add or remove in the message.
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { AREAS, CHECKS, CI_RUNS_ALL, EMBEDS, IGNORE, OVERLAY } from "./map.ts";

const root = join(import.meta.dir, "..", "..");
const read = (f: string) => readFileSync(join(root, f), "utf8");
interface Step {
	id?: string;
	uses?: string;
	run?: string;
	shell?: string;
	with?: Record<string, unknown>;
}
interface Job {
	if?: string;
	needs?: string[];
	outputs?: Record<string, string>;
	steps?: Step[];
}
const yaml = <T>(f: string) => Bun.YAML.parse(read(f)) as T;

const ci = yaml<{ jobs: Record<string, Job> }>(".github/workflows/ci.yml");
const jobs = ci.jobs;
const INFRA = new Set(["plan", "ci"]);
const jobNames = Object.keys(jobs).filter((j) => !INFRA.has(j));
const pkg = JSON.parse(read("package.json"));

/** Whitespace-collapsed command text with the differences between a dev machine and a runner removed. */
const norm = (s: string) => s.replace(/\s+/g, " ").replaceAll(" --profile ci", "").replaceAll("bun scripts/sh.ts ", "").trim();

/** Every command line a job runs, composite actions included, plus the actions it uses. */
function jobLines(name: string): string[] {
	const out: string[] = [];
	for (const step of jobs[name]!.steps ?? []) {
		if (step.run) out.push(...String(step.run).split("\n"));
		if (step.uses) {
			out.push(step.uses);
			if (String(step.uses).startsWith("./")) {
				const action = yaml<{ runs: { steps?: Step[] } }>(`${step.uses.slice(2)}/action.yml`);
				for (const s of action.runs.steps ?? []) if (s.run) out.push(...String(s.run).split("\n"));
			}
		}
	}
	return out.map((l) => norm(l)).filter((l) => l && !l.startsWith("#"));
}

/** What a check looks like in CI's job text. */
const needle = (k: (typeof CHECKS)[number]) => norm(k.ciRun ?? k.cmd!.replaceAll("{cargo-lib}", "--workspace").replaceAll("{cargo}", "--workspace"));
const checksOf = (job: string) => CHECKS.filter((k) => k.ci.includes(job));

describe("ci.yml and the map list the same jobs", () => {
	test("every CI job is run by a check of the map", () => {
		for (const j of jobNames) {
			expect(checksOf(j).length, `ci.yml job \`${j}\` has no check in scripts/checks/map.ts: add one with ci: ["${j}"] (no cmd + a why when a dev machine cannot run it)`).toBeGreaterThan(0);
		}
	});

	test("every job a check names exists in ci.yml", () => {
		for (const k of CHECKS) for (const j of k.ci) expect(jobNames, `check \`${k.id}\` names ci job \`${j}\`, which ci.yml does not have`).toContain(j);
	});

	test("the one required check, `CI`, waits for every job", () => {
		expect([...ci.jobs.ci!.needs!].sort()).toEqual(["plan", ...jobNames].sort());
	});
});

describe("areas and the plan job", () => {
	const used = (job: string) => [...new Set([...String(jobs[job]!.if ?? "").matchAll(/needs\.plan\.outputs\.(\w+)/g)].map((m) => m[1]!))].filter((a) => a !== "tested").sort();

	test("the plan job outputs exactly the areas of the map (and `tested`)", () => {
		const outputs = jobs.plan!.outputs!;
		expect(Object.keys(outputs).sort()).toEqual(["tested", ...Object.keys(AREAS)].sort());
		for (const [k, v] of Object.entries(outputs)) if (k !== "tested") expect(v).toBe(`\${{ steps.paths.outputs.${k} }}`);
	});

	test("a job runs when the areas of its checks changed, no more and no fewer", () => {
		for (const j of jobNames) {
			const areas = [...new Set(checksOf(j).map((k) => k.area))].sort();
			expect(used(j), `ci.yml job \`${j}\`: its \`if\` must test needs.plan.outputs.<area> for ${areas.join(", ")} (the areas of its checks in scripts/checks/map.ts)`).toEqual(areas);
		}
	});

	test("every area gates a job", () => {
		for (const a of Object.keys(AREAS)) {
			expect(CHECKS.some((k) => k.area === a), `area \`${a}\` has no check`).toBe(true);
		}
	});

	test("the plan step is scripts/ci-plan.ts, the map's reader, with pipefail so a failed plan fails the job", () => {
		const steps = jobs.plan!.steps!;
		const step = steps.find((s) => s.id === "paths")!;
		expect(step.run).toContain("bun scripts/ci-plan.ts");
		expect(step.shell).toBe("bash");
		expect(steps.some((s) => s.uses?.startsWith("oven-sh/setup-bun@"))).toBe(true);
		expect(steps.find((s) => s.uses?.startsWith("actions/checkout@"))!.with!["fetch-depth"]).toBe(0);
	});
});

describe("check commands match what CI runs", () => {
	test("each check's command is in the text of every job that runs it", () => {
		for (const k of CHECKS) {
			for (const j of k.ci) {
				const text = jobLines(j).join("\n");
				expect(text, `check \`${k.id}\` (${needle(k)}) is not run by ci.yml job \`${j}\`: change the map or the job so they agree`).toContain(needle(k));
			}
		}
	});

	// The reverse: a command added to a job without the map.
	const CHECKY = /^(bun run |bun test |bun scripts\/|scripts\/|cargo (fmt|clippy|nextest|test|deny) |sudo -E )/;
	test("every check-like command a job runs belongs to a check of the map", () => {
		for (const j of jobNames) {
			for (const line of jobLines(j)) {
				if (!CHECKY.test(line) || line.includes("--no-run")) continue; // building tests is not a check
				const known = checksOf(j).some((k) => line.includes(needle(k)));
				expect(known, `ci.yml job \`${j}\` runs \`${line}\`, which scripts/checks/map.ts does not know: add a check for it (ci: ["${j}"])`).toBe(true);
			}
		}
	});

	test("`bun run check` is the map's runner, so a check added to the map is in the gate", () => {
		expect(pkg.scripts.check).toBe("bun scripts/check.ts");
		expect(pkg.scripts["check:changed"]).toBe("bun scripts/check-changed.ts");
	});

	test("jscpd's threshold is 1.5%, the number AGENTS.md and CI promise", () => {
		expect(JSON.parse(read(".jscpd.json")).threshold).toBeLessThanOrEqual(1.5);
	});
});

describe("the map itself", () => {
	test("ids are unique, areas exist, a check without a command says why a dev machine cannot run it", () => {
		const ids = CHECKS.map((k) => k.id);
		expect(new Set(ids).size).toBe(ids.length);
		for (const k of CHECKS) {
			expect(Object.keys(AREAS), `check ${k.id}`).toContain(k.area);
			expect(k.ci.length, `check ${k.id} runs in no CI job`).toBeGreaterThan(0);
			if (!k.cmd) {
				expect(k.why?.length ?? 0, `check ${k.id} has no cmd and no why`).toBeGreaterThan(20);
				expect(k.ciRun, `check ${k.id} has no cmd: ciRun says what CI runs`).toBeTruthy();
			}
		}
		for (const a of Object.keys(OVERLAY)) expect(Object.keys(AREAS)).toContain(a);
	});

	test("every command is something that exists: a package.json script, a repo script or cargo/bun itself", () => {
		for (const k of CHECKS) {
			for (const m of (k.cmd ?? "").matchAll(/bun run ([\w:-]+)/g)) expect(Object.keys(pkg.scripts), `${k.id}: bun run ${m[1]}`).toContain(m[1]);
			for (const m of (k.cmd ?? "").matchAll(/(scripts\/[\w./-]+\.(sh|ts))/g)) expect(existsSync(join(root, m[1]!)), `${k.id}: ${m[1]}`).toBe(true);
		}
	});

	test("every area glob is syntactically usable and the crates it embeds are directories", () => {
		for (const g of [...Object.values(AREAS).flat(), ...Object.values(OVERLAY).flat(), ...IGNORE, ...CI_RUNS_ALL]) {
			expect(() => new Bun.Glob(g), g).not.toThrow();
			expect(g.startsWith("/") || g.includes("\\"), g).toBe(false);
		}
		for (const [, crate] of EMBEDS) expect(existsSync(join(root, "crates", crate, "Cargo.toml")), crate).toBe(true);
	});

	test("the dirs the old maps named are still claimed: a path in each root is known", () => {
		// A top-level directory nobody maps would run everything on every change; this lists the ones that must not.
		const known = ["crates", "apps/desktop", "apps/mobile", "services/push-relay", "site", "docs", "skills", "scripts", "packaging", "testdata"];
		for (const d of known) {
			const hit = Object.values(AREAS).flat().some((g) => g.startsWith(`${d}/`) || g === d);
			expect(hit, `${d}/ is claimed by no area`).toBe(true);
		}
	});
});
