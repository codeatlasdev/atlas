/** GitHub Actions workflow model: triggers, jobs (with the `needs` graph) and steps. */
import { asRecord, asString, asStringArray, isRecord } from "#lib/viewers/rich/data.ts";
import { topoOrder } from "#lib/viewers/rich/compose.ts";
import { m } from "#lib/i18n/m.ts";

export interface Step {
	name: string;
	/** `uses:` action reference, else null. */
	uses: string | null;
	run: string | null;
	cond: string | null;
	with: Record<string, string>;
}

export interface Job {
	id: string;
	name: string;
	runsOn: string;
	needs: string[];
	cond: string | null;
	steps: Step[];
	matrix: Record<string, string[]>;
	timeout: number | null;
	/** Depth in the `needs` graph (0 = no dependencies). */
	level: number;
}

export interface Workflow {
	name: string | null;
	triggers: { event: string; detail: string }[];
	jobs: Job[];
	env: Record<string, string>;
	/** `needs` entries that name no job. */
	unknownNeeds: { job: string; need: string }[];
	/** `uses:` that is not pinned to a SHA or version tag. */
	unpinned: { job: string; uses: string }[];
}

const str = (v: unknown) => (v === undefined || v === null ? "" : String(v));

function triggers(on: unknown): Workflow["triggers"] {
	if (typeof on === "string") return [{ event: on, detail: "" }];
	if (Array.isArray(on)) return on.map((e) => ({ event: String(e), detail: "" }));
	return Object.entries(asRecord(on)).map(([event, cfg]) => {
		const c = asRecord(cfg);
		const bits = [
			c.branches ? `branches ${asStringArray(c.branches).join(", ")}` : "",
			c.tags ? `tags ${asStringArray(c.tags).join(", ")}` : "",
			c.paths ? `paths ${asStringArray(c.paths).join(", ")}` : "",
			c.types ? asStringArray(c.types).join(", ") : "",
			Array.isArray(cfg) ? asStringArray(cfg).join(", ") : "",
			event === "schedule" && Array.isArray(cfg)
				? (cfg as { cron?: string }[]).map((s) => s.cron).join(", ")
				: "",
		].filter(Boolean);
		return { event, detail: bits.join(" · ") };
	});
}

export function workflowModel(value: unknown): Workflow {
	const root = asRecord(value);
	// YAML 1.1 parsers turn the key `on` into `true`; `yaml` keeps "on", but be tolerant.
	const on = root.on ?? (root as Record<string, unknown>)["true"];
	const rawJobs = asRecord(root.jobs);
	const jobs: Job[] = Object.entries(rawJobs).map(([id, raw]) => {
		const j = asRecord(raw);
		const matrix = asRecord(asRecord(asRecord(j.strategy).matrix));
		const ro = j["runs-on"];
		const needs = typeof j.needs === "string" ? [j.needs] : asStringArray(j.needs);
		return {
			id,
			name: asString(j.name) ?? id,
			runsOn: Array.isArray(ro)
				? ro.join(", ")
				: isRecord(ro)
					? str(ro.group ?? ro.labels)
					: str(ro) || (j.uses ? m.viewers_files_workflow_reusable() : ""),
			needs,
			cond: asString(j.if) ?? null,
			steps: (Array.isArray(j.steps) ? j.steps : []).map((s, i) => {
				const st = asRecord(s);
				const uses = asString(st.uses) ?? null;
				const run = asString(st.run) ?? null;
				return {
					name:
						asString(st.name) ??
						uses ??
						(run ? run.split("\n")[0] : m.viewers_files_workflow_step_n({ n: i + 1 })),
					uses,
					run,
					cond: asString(st.if) ?? null,
					with: Object.fromEntries(Object.entries(asRecord(st.with)).map(([k, v]) => [k, str(v)])),
				};
			}),
			matrix: Object.fromEntries(
				Object.entries(matrix)
					.filter(([, v]) => Array.isArray(v))
					.map(([k, v]) => [k, (v as unknown[]).map(str)]),
			),
			timeout: typeof j["timeout-minutes"] === "number" ? j["timeout-minutes"] : null,
			level: 0,
		};
	});
	const ids = new Set(jobs.map((j) => j.id));
	const byId = new Map(jobs.map((j) => [j.id, j]));
	const levelOf = (j: Job, seen: Set<string>): number => {
		if (seen.has(j.id)) return 0;
		seen.add(j.id);
		return j.needs.reduce(
			(deepest, n) => (byId.has(n) ? Math.max(deepest, levelOf(byId.get(n)!, seen) + 1) : deepest),
			0,
		);
	};
	for (const j of jobs) j.level = levelOf(j, new Set());
	const order = topoOrder(jobs.map((j) => ({ name: j.id, deps: j.needs })));
	jobs.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
	const pin = /^(\.\/|docker:\/\/)|@([0-9a-f]{40}|v?\d+(\.\d+){0,2}[\w.-]*)$/;
	return {
		name: asString(root.name) ?? null,
		triggers: triggers(on),
		jobs,
		env: Object.fromEntries(Object.entries(asRecord(root.env)).map(([k, v]) => [k, str(v)])),
		unknownNeeds: jobs.flatMap((j) =>
			j.needs.filter((n) => !ids.has(n)).map((need) => ({ job: j.id, need })),
		),
		unpinned: jobs.flatMap((j) =>
			j.steps.flatMap((s) => (s.uses && !pin.test(s.uses) ? [{ job: j.id, uses: s.uses }] : [])),
		),
	};
}
