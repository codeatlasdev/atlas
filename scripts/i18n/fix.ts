// bun run i18n:fix [--restamp <key>] [--baseline]
// The one command that writes derived files: sorts and formats Paraglide catalogs, rewrites the locks, shrinks the
// hardcoded-text baseline. It is also the conflict-resolution command after merging two branches.
//   locks      for every translated key: keep the stamp of a key whose English changed (it stays outdated), stamp a key
//              that has none, drop entries of keys that no longer exist.
//   --restamp  a typo-only English edit: record the current hash of <key> in every locale without retranslating.
//   --baseline also start a baseline section that does not exist yet from today's counts (first run); an existing
//              baseline only ever shrinks.
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type Baseline, readBaseline, shrunk, writeBaseline } from "./baseline.ts";
import { hashOf, type Surface } from "./engine.ts";
import { formatProblem } from "./formats/paraglide.ts";
import { readLocks, writeLocks } from "./lock.ts";
import { RATCHETS, scanRatchet } from "./ratchets.ts";
import { loadSurfaces } from "./surfaces.ts";
import { BASELINE_FILE, type Ctx, loadCtx } from "./types.ts";
import { canonicalJson, detectIndent, readText } from "./util.ts";

/** Rewrite the lock of `locale`: `forced` keys get the current English hash, the rest keep their stamp. */
export function rebuildLock(root: string, surface: Surface, locale: string, forced: ReadonlySet<string> = new Set()) {
	const base = surface.entries.get(surface.base) ?? new Map();
	const mine = surface.entries.get(locale) ?? new Map();
	const old = readLocks(root, surface.name, locale).byKey;
	const hashes = new Map<string, Record<string, string>>();
	for (const [key, b] of base) {
		if (!mine.has(key)) continue;
		const stamp = old.get(key)?.hash;
		const hash = forced.has(key) || !stamp || !/^[0-9a-f]{12}$/.test(stamp) ? hashOf(b) : stamp;
		hashes.set(b.area, { ...hashes.get(b.area), [key]: hash });
	}
	writeLocks(root, surface.name, locale, hashes);
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
}

export function fix(ctx: Ctx, opts: FixOptions = {}): string[] {
	const { root } = ctx;
	const log: string[] = [];
	const restamp = new Set(opts.restamp ?? []);
	const known = new Set<string>();
	for (const { surface, catalogs, descriptionsFile } of loadSurfaces(ctx)) {
		for (const path of [...(catalogs?.values() ?? []), ...(descriptionsFile ? [descriptionsFile] : [])]) {
			if (canonicalizeFile(path)) log.push(`formatted ${path.slice(root.length + 1)}`);
		}
		for (const key of restamp) if (surface.entries.get(surface.base)?.has(key)) known.add(key);
		for (const locale of surface.locales.filter((l) => l !== surface.base)) rebuildLock(root, surface, locale, restamp);
		log.push(`locks ${surface.name}: ${surface.locales.filter((l) => l !== surface.base).join(", ") || "none"}`);
	}
	for (const key of restamp) if (!known.has(key)) throw new Error(`--restamp ${key}: no such key in any English source`);

	if (existsSync(join(root, BASELINE_FILE)) || opts.baseline) {
		const before = readBaseline(root);
		const next: Baseline = {};
		for (const def of RATCHETS) {
			const { found, prefixes } = scanRatchet({ ...ctx, surface: undefined }, def);
			if (before[def.lang] || opts.baseline) next[def.lang] = shrunk(before, def.lang, found, opts.baseline === true, prefixes);
		}
		writeBaseline(root, next);
		log.push(`baseline ${JSON.stringify(readBaseline(root)) === JSON.stringify(before) ? "unchanged" : "updated"}`);
	}
	return log;
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const restamp = args.flatMap((a, i) => (args[i - 1] === "--restamp" ? [a] : []));
	if (args.includes("--restamp") && restamp.length === 0) {
		console.error("usage: bun run i18n:fix [--restamp <key>] [--baseline]");
		process.exit(2);
	}
	const root = args.includes("--root") ? args[args.indexOf("--root") + 1] : join(import.meta.dir, "..", "..");
	try {
		for (const line of fix(loadCtx(root), { restamp, baseline: args.includes("--baseline") })) console.log(line);
	} catch (e) {
		console.error(`i18n:fix: ${(e as Error).message}`);
		process.exit(1);
	}
}
