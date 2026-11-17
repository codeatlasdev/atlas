// i18n/lock/<surface>/<locale>/<area>.json: for every translated key, the 12-hex hash of the English value it was
// translated from. A key whose current English hash differs is outdated. Only `i18n:fix` and `i18n:translate`
// write these files; a person never does.
import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { LOCK_DIR } from "./types.ts";
import { canonicalJson, readText, rel } from "./util.ts";

export const LOCK_INDENT = "  ";

export interface LockEntry {
	hash: string;
	area: string;
	file: string;
}

export interface LockRead {
	byKey: Map<string, LockEntry>;
	/** Lock files that are unreadable, not a flat string map, or not in canonical form. */
	problems: { file: string; message: string; key?: string }[];
}

export const lockFile = (surface: string, locale: string, area: string) => `${LOCK_DIR}/${surface}/${locale}/${area}.json`;

export function readLocks(root: string, surface: string, locale: string): LockRead {
	const out: LockRead = { byKey: new Map(), problems: [] };
	const dir = join(root, LOCK_DIR, surface, locale);
	if (!existsSync(dir)) return out;
	for (const name of readdirSync(dir).filter((n) => n.endsWith(".json")).sort()) {
		const path = join(dir, name);
		const file = rel(root, path);
		const area = name.slice(0, -".json".length);
		const text = readText(path);
		let obj: unknown;
		try {
			obj = JSON.parse(text);
		} catch (e) {
			out.problems.push({ file, message: `invalid JSON: ${(e as Error).message}` });
			continue;
		}
		if (typeof obj !== "object" || obj === null || Array.isArray(obj) || Object.values(obj).some((v) => typeof v !== "string")) {
			out.problems.push({ file, message: "a lock is a flat { key: hash } object" });
			continue;
		}
		const map = obj as Record<string, string>;
		for (const [key, hash] of Object.entries(map)) {
			if (!/^[0-9a-f]{12}$/.test(hash)) out.problems.push({ file, key, message: `"${hash}" is not a 12-hex hash` });
			if (out.byKey.has(key)) out.problems.push({ file, key, message: `also locked in ${out.byKey.get(key)?.file}` });
			else out.byKey.set(key, { hash, area, file });
		}
		if (text !== canonicalJson(map, LOCK_INDENT)) out.problems.push({ file, message: "not in canonical form (sorted keys, one per line, LF, trailing newline); run bun run i18n:fix" });
	}
	return out;
}

/** Replace the lock of `locale` with `hashes` (area -> key -> hash); areas that end up empty lose their file. */
export function writeLocks(root: string, surface: string, locale: string, hashes: Map<string, Record<string, string>>) {
	const dir = join(root, LOCK_DIR, surface, locale);
	const keep = new Set<string>();
	for (const [area, map] of [...hashes].sort(([a], [b]) => (a < b ? -1 : 1))) {
		if (Object.keys(map).length === 0) continue;
		const path = join(root, lockFile(surface, locale, area));
		mkdirSync(dirname(path), { recursive: true });
		writeFileSync(path, canonicalJson(map, LOCK_INDENT));
		keep.add(`${area}.json`);
	}
	if (existsSync(dir)) for (const n of readdirSync(dir)) if (n.endsWith(".json") && !keep.has(n)) rmSync(join(dir, n));
}
