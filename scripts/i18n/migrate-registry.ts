// bun run i18n:migrate-registry [--root <dir>] [--json]
// Converts a branch written for the old registry to the conflict-free one. Idempotent: run it again and it reports
// nothing to do. It reads old-format data from the working tree (also while a merge of main is half done: it parses
// the lines of surfaces.json even between conflict markers) and from `HEAD`, and writes:
//   - i18n/messages/<area>/area.json for every area folder that has none ({ "surface": "<s>" }, and "generated" for
//     a generated area), from the `"areas": [...]` lists and `generatedAreas` of the old surfaces.json;
//   - i18n/surfaces.json without the area lists (conflict markers gone), keeping the budget and each surface's output;
//   - i18n/hardcoded-baseline.jsonl from the old i18n/hardcoded-baseline.json: when main's file exists, the branch's
//     work is applied to it (a count the branch lowered or removed is lowered there; nothing is ever added or raised),
//     else the old file is converted whole; the old file is deleted;
//   - then `i18n:fix`, which canonicalizes and shrinks the baseline to the real counts.
// After a `git merge origin/main` with conflicts: run this, `git add -A i18n`, commit.
import { existsSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { showAt } from "../lib/base.ts";
import { type Baseline, type Lang, parseBaseline, renderBaseline } from "./baseline.ts";
import { fix } from "./fix.ts";
import { AREA_FILE, type SurfacesConfig, isCommon, renderSurfaces, writeAreaFile } from "./source.ts";
import { BASELINE_FILE, MESSAGES_DIR, SURFACES_FILE, loadCtx } from "./types.ts";
import { readText } from "./util.ts";

const OLD_BASELINE_FILE = "i18n/hardcoded-baseline.json";
const OUTPUTS = ["paraglide", "arb", "rust"] as const;

interface OldRegistry {
	budget?: { keys: number; bytes: number };
	/** surface -> output kind and directory */
	outputs: Record<string, Partial<Record<(typeof OUTPUTS)[number], string>>>;
	/** area -> surface (old `areas` lists) */
	areaSurface: Map<string, string>;
	generated: Map<string, string>;
}

/** The registry in `text`, read line by line so a file with conflict markers still yields both sides' lines. */
export function readOldRegistry(text: string): OldRegistry {
	const reg: OldRegistry = { outputs: {}, areaSurface: new Map(), generated: new Map() };
	for (const line of text.split("\n")) {
		const budget = /^\s*"budget":\s*(\{.*\})\s*,?\s*$/.exec(line);
		if (budget && !reg.budget) {
			const b = tryJson(budget[1]);
			if (typeof b?.keys === "number" && typeof b?.bytes === "number") reg.budget = { keys: b.keys, bytes: b.bytes };
			continue;
		}
		const generated = /^\s*"generatedAreas":\s*(\{.*\})\s*,?\s*$/.exec(line);
		if (generated) {
			for (const [area, cmd] of Object.entries(tryJson(generated[1]) ?? {})) if (typeof cmd === "string") reg.generated.set(area, cmd);
			continue;
		}
		const surface = /^\s*"([a-z][\w-]*)":\s*(\{.*\})\s*,?\s*$/.exec(line);
		const def = surface ? tryJson(surface[2]) : undefined;
		if (!surface || !def || !OUTPUTS.some((k) => typeof def[k] === "string")) continue;
		const [, name] = surface;
		reg.outputs[name] ??= Object.fromEntries(OUTPUTS.filter((k) => typeof def[k] === "string").map((k) => [k, String(def[k])]));
		if (Array.isArray(def.areas)) for (const a of def.areas) if (typeof a === "string" && !reg.areaSurface.has(a)) reg.areaSurface.set(a, name);
	}
	return reg;
}

function tryJson(text: string): Record<string, unknown> | undefined {
	try {
		const j = JSON.parse(text);
		return typeof j === "object" && j !== null && !Array.isArray(j) ? j : undefined;
	} catch {
		return undefined;
	}
}

/** `main`'s baseline with the branch's work from `old` applied: min per path, and a path the branch no longer lists is 0.
 * A ratchet the old file has no section for is left alone. */
export function applyBranchBaseline(main: Baseline, old: Baseline): Baseline {
	const out: Baseline = {};
	for (const lang of Object.keys(main) as Lang[]) {
		const theirs = old[lang];
		out[lang] = Object.fromEntries(Object.entries(main[lang] ?? {}).map(([file, n]) => [file, theirs ? Math.min(n, theirs[file] ?? 0) : n]));
	}
	return out;
}

export interface MigrateReport {
	areaFiles: string[];
	surfacesRewritten: boolean;
	baseline: "converted" | "applied" | "none";
	unresolved: string[];
	log: string[];
}

export function migrateRegistry(root: string): MigrateReport {
	const report: MigrateReport = { areaFiles: [], surfacesRewritten: false, baseline: "none", unresolved: [], log: [] };
	const surfacesPath = join(root, SURFACES_FILE);
	const working = existsSync(surfacesPath) ? readText(surfacesPath) : "";
	// The working file first, then the branch's own committed one: after a resolution that took main's file, HEAD still has the lists.
	const sources = [working, showAt(root, "HEAD", SURFACES_FILE) ?? ""];
	const reg = readOldRegistry(sources[0]);
	for (const text of sources.slice(1)) {
		const more = readOldRegistry(text);
		reg.budget ??= more.budget;
		for (const [k, v] of Object.entries(more.outputs)) reg.outputs[k] ??= v;
		for (const [a, s] of more.areaSurface) if (!reg.areaSurface.has(a)) reg.areaSurface.set(a, s);
		for (const [a, c] of more.generated) if (!reg.generated.has(a)) reg.generated.set(a, c);
	}

	const dir = join(root, MESSAGES_DIR);
	const areas = existsSync(dir) ? readdirSync(dir, { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name).sort() : [];
	for (const area of areas) {
		if (isCommon(area) || existsSync(join(dir, area, AREA_FILE))) continue;
		const surface = reg.areaSurface.get(area);
		if (!surface) {
			report.unresolved.push(area);
			continue;
		}
		const generated = reg.generated.get(area);
		writeAreaFile(root, area, generated ? { surface, generated } : { surface });
		report.areaFiles.push(`${MESSAGES_DIR}/${area}/${AREA_FILE}`);
	}

	if (Object.keys(reg.outputs).length) {
		const config: SurfacesConfig = { budget: reg.budget ?? { keys: 150, bytes: 24576 }, surfaces: {}, generatedAreas: {} };
		for (const [name, out] of Object.entries(reg.outputs)) config.surfaces[name] = { areas: [], ...out };
		const next = renderSurfaces(config);
		if (next !== working) {
			writeFileSync(surfacesPath, next);
			report.surfacesRewritten = true;
		}
	}

	const oldPath = join(root, OLD_BASELINE_FILE);
	if (existsSync(oldPath)) {
		const old = readOldBaseline(readText(oldPath), showAt(root, "HEAD", OLD_BASELINE_FILE));
		const newPath = join(root, BASELINE_FILE);
		if (existsSync(newPath)) {
			writeFileSync(newPath, renderBaseline(applyBranchBaseline(parseBaseline(readText(newPath)), old)));
			report.baseline = "applied";
		} else {
			writeFileSync(newPath, renderBaseline(old));
			report.baseline = "converted";
		}
		rmSync(oldPath, { force: true });
	}

	if (report.unresolved.length) {
		report.log.push(`no surface known for: ${report.unresolved.join(", ")}; write i18n/messages/<area>/area.json as { "surface": "<desktop|rust|site>" } by hand`);
		return report;
	}
	if (report.areaFiles.length || report.surfacesRewritten || report.baseline !== "none") {
		for (const line of fix(loadCtx(root))) report.log.push(line);
	}
	return report;
}

/** The old `{ "<ratchet>": { "<file>": <count> } }` file; when the working copy has conflict markers, the committed one. */
function readOldBaseline(text: string, committed: string | undefined): Baseline {
	for (const t of [text, committed]) {
		const j = t === undefined ? undefined : tryJson(t);
		if (!j) continue;
		const out: Baseline = {};
		for (const [lang, section] of Object.entries(j)) {
			if (typeof section !== "object" || section === null) continue;
			out[lang as Lang] = Object.fromEntries(Object.entries(section).filter(([, n]) => typeof n === "number" && Number.isInteger(n) && n >= 0).map(([f, n]) => [f, Number(n)]));
		}
		return out;
	}
	throw new Error(`${OLD_BASELINE_FILE} is neither valid JSON nor committed at HEAD: restore the branch's version (git checkout HEAD -- ${OLD_BASELINE_FILE}) and run again`);
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const root = args.includes("--root") ? args[args.indexOf("--root") + 1] : join(import.meta.dir, "..", "..");
	try {
		const r = migrateRegistry(root);
		if (args.includes("--json")) console.log(JSON.stringify(r));
		else if (!r.areaFiles.length && !r.surfacesRewritten && r.baseline === "none" && !r.unresolved.length) console.log("nothing to migrate: the registry is already in the new format");
		else {
			for (const f of r.areaFiles) console.log(`wrote ${f}`);
			if (r.surfacesRewritten) console.log(`rewrote ${SURFACES_FILE} (per-surface config only)`);
			if (r.baseline !== "none") console.log(`baseline ${r.baseline}: ${BASELINE_FILE} (old ${OLD_BASELINE_FILE} removed)`);
			for (const l of r.log) console.log(l);
			if (!r.unresolved.length) console.log("next: git add -A i18n && commit");
		}
		process.exit(r.unresolved.length ? 1 : 0);
	} catch (e) {
		console.error(`i18n:migrate-registry: ${(e as Error).message}`);
		process.exit(1);
	}
}
