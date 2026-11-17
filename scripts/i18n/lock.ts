// i18n/messages/<area>/lock.json: for every translated key of the area, the 12-hex hash of the English value it was
// translated from, per locale: `{ "pt-BR": { "<key>": "3fa1c2d09b7e" } }`. A key whose current English hash differs is
// outdated. Only `i18n:fix`, `i18n:translate`, `i18n:add` and `i18n:mv` write this file; a person never does.
import { existsSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { MESSAGES_DIR } from "./types.ts";
import { cmp, readText } from "./util.ts";

export const lockFile = (area: string) => `${MESSAGES_DIR}/${area}/lock.json`;

type LockFile = Record<string, Record<string, string>>;

/** Keys sorted at both levels, two-space indent, LF, trailing newline: the one form a lock is written in. */
function render(lock: LockFile): string {
	const sorted: LockFile = {};
	for (const l of Object.keys(lock).sort(cmp)) {
		sorted[l] = Object.fromEntries(Object.entries(lock[l]).sort(([a], [b]) => cmp(a, b)));
	}
	return `${JSON.stringify(sorted, null, "  ")}\n`;
}

export interface LockRead {
	byKey: Map<string, string>;
	/** A lock that is unreadable, not `{ locale: { key: hash } }`, holds a bad hash, or is not in canonical form. */
	problems: { message: string; key?: string }[];
}

/** Parse the whole file; `problems` are about the file, not one locale. */
function readAll(root: string, area: string): { lock: LockFile; problems: LockRead["problems"] } {
	const path = join(root, lockFile(area));
	const out: { lock: LockFile; problems: LockRead["problems"] } = { lock: {}, problems: [] };
	if (!existsSync(path)) return out;
	const text = readText(path);
	let obj: unknown;
	try {
		obj = JSON.parse(text);
	} catch (e) {
		out.problems.push({ message: `invalid JSON: ${(e as Error).message}` });
		return out;
	}
	const ok = typeof obj === "object" && obj !== null && !Array.isArray(obj) && Object.values(obj).every((v) => typeof v === "object" && v !== null && !Array.isArray(v) && Object.values(v).every((h) => typeof h === "string"));
	if (!ok) {
		out.problems.push({ message: 'a lock is { "<locale>": { "<key>": "<hash>" } }' });
		return out;
	}
	out.lock = Object.fromEntries(Object.entries(obj as object).map(([l, m]) => [l, Object.fromEntries(Object.entries(m as object))])) as LockFile;
	if (text !== render(out.lock)) out.problems.push({ message: "not in canonical form (sorted keys, one per line, LF, trailing newline); run bun run i18n:fix" });
	return out;
}

export function readLock(root: string, area: string, locale: string): LockRead {
	const { lock, problems } = readAll(root, area);
	const out: LockRead = { byKey: new Map(), problems: [...problems] };
	for (const [key, hash] of Object.entries(lock[locale] ?? {})) {
		if (!/^[0-9a-f]{12}$/.test(hash)) out.problems.push({ key, message: `"${hash}" is not a 12-hex hash` });
		out.byKey.set(key, hash);
	}
	return out;
}

/** Replace the lock of one locale of an area, keeping the other locales; an empty file is deleted. */
export function writeLock(root: string, area: string, locale: string, hashes: Record<string, string>) {
	const { lock } = readAll(root, area);
	if (Object.keys(hashes).length) lock[locale] = hashes;
	else delete lock[locale];
	const path = join(root, lockFile(area));
	if (Object.keys(lock).length === 0) rmSync(path, { force: true });
	else writeFileSync(path, render(lock));
}
