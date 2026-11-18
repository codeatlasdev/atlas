// bun run i18n:fix [--restamp <key>] [--baseline] [--prune-baseline]
// The one command that writes derived files: sorts and formats the area files, rewrites the locks, shrinks the
// ratchet baseline. It is also the conflict-resolution command after merging two branches.
//   locks      for every translated key: keep the stamp of a key whose English changed (it stays outdated), stamp a key
//              that has none, drop entries of keys that no longer exist.
//   --restamp  a typo-only English edit: record the current hash of <key> in every locale without retranslating.
//   --baseline also start a baseline section that does not exist yet from today's counts (first run); an existing
//              baseline only ever shrinks. A file whose offenders are gone keeps its line at 0 (a deleted line comes
//              back in a union merge), a path listed twice (a union merge) collapses to its smaller count.
//   --prune-baseline  also delete the lines at 0; run it alone, on a branch that does nothing else.
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type Baseline, readBaseline, renamedFiles, renderBaseline, shrunk } from "./baseline.ts";
import { hashOf } from "./engine.ts";
import type { Area } from "./model.ts";
import { readLock, writeLock } from "./lock.ts";
import { RATCHETS, scanRatchet } from "./ratchets.ts";
import { AREA_FILE, formatProblem, loadSource } from "./source.ts";
import { BASELINE_FILE, type Ctx, loadCtx } from "./types.ts";
import { canonicalJson, detectIndent, readText } from "./util.ts";

/** Rewrite the lock of one area and locale: `forced` keys get the current English hash, the rest keep their stamp. */
export function rebuildLock(root: string, area: Area, locale: string, forced: ReadonlySet<string> = new Set()) {
	const base = area.entries.get(area.base) ?? new Map();
	const mine = area.entries.get(locale) ?? new Map();
	const old = readLock(root, area.name, locale).byKey;
	const hashes: Record<string, string> = {};
	for (const [key, b] of base) {
		if (!mine.has(key)) continue;
		const stamp = old.get(key);
		hashes[key] = forced.has(key) || !stamp || !/^[0-9a-f]{12}$/.test(stamp) ? hashOf(b) : stamp;
	}
	writeLock(root, area.name, locale, hashes);
}

/** Rewrite a JSON file in canonical form if it parses and is not canonical yet; true when it changed. */
function canonicalizeFile(path: string): boolean {
	if (!existsSync(path)) return false;
	const text = readText(path);
	let obj: Record<string, unknown>;
	try {
		obj = JSON.parse(text);
	} catch {
		return false;
	}
	if (!formatProblem(text, obj)) return false;
	writeFileSync(path, canonicalJson(obj, detectIndent(text)));
	return true;
}

export interface FixOptions {
	restamp?: string[];
	baseline?: boolean;
	pruneBaseline?: boolean;
}

export function fix(ctx: Ctx, opts: FixOptions = {}): string[] {
	const { root } = ctx;
	const log: string[] = [];
	const restamp = new Set(opts.restamp ?? []);
	const known = new Set<string>();
	const source = loadSource(ctx);
	for (const area of source.areas.values()) {
		for (const f of [...area.locales.map((l) => `${area.name}/${l}.json`), `${area.name}/meta.json`, `${area.name}/${AREA_FILE}`]) {
			if (canonicalizeFile(join(root, "i18n/messages", f))) log.push(`formatted i18n/messages/${f}`);
		}
		for (const key of restamp) if (area.entries.get(area.base)?.has(key)) known.add(key);
		for (const locale of area.locales.filter((l) => l !== area.base)) rebuildLock(root, area, locale, restamp);
	}
	log.push(`locks: ${source.areas.size} area(s)`);
	for (const key of restamp) if (!known.has(key)) throw new Error(`--restamp ${key}: no such key in any English source`);

	if (existsSync(join(root, BASELINE_FILE)) || opts.baseline) {
		const before = readBaseline(root);
		const next: Baseline = {};
		const renamed = renamedFiles(ctx.base?.changed);
		for (const def of RATCHETS) {
			const { found, prefixes } = scanRatchet({ ...ctx, surface: undefined }, def);
			if (before[def.lang] || opts.baseline) next[def.lang] = shrunk(before, def.lang, found, opts.baseline === true, prefixes, renamed);
		}
		const prune = opts.pruneBaseline === true;
		const text = renderBaseline(next, prune);
		const was = existsSync(join(root, BASELINE_FILE)) ? readText(join(root, BASELINE_FILE)) : "";
		writeFileSync(join(root, BASELINE_FILE), text);
		log.push(`baseline ${text === was ? "unchanged" : "updated"}`);
	}
	return log;
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const restamp = args.flatMap((a, i) => (args[i - 1] === "--restamp" ? [a] : []));
	if (args.includes("--restamp") && restamp.length === 0) {
		console.error("usage: bun run i18n:fix [--restamp <key>] [--baseline] [--prune-baseline]");
		process.exit(2);
	}
	const root = args.includes("--root") ? args[args.indexOf("--root") + 1] : join(import.meta.dir, "..", "..");
	try {
		for (const line of fix(loadCtx(root), { restamp, baseline: args.includes("--baseline"), pruneBaseline: args.includes("--prune-baseline") })) console.log(line);
	} catch (e) {
		console.error(`i18n:fix: ${(e as Error).message}`);
		process.exit(1);
	}
}
