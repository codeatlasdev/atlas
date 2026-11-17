// i18n/hardcoded-baseline.json: today's count of offenders per file, per ratchet (`svelte`, `rust`, `shortcut`,
// `platform`, `writing`). It is a ratchet: a file may never have more than its baseline (a new violation fails),
// and a baseline that is now higher than reality fails too, so the number only goes down. `bun run i18n:fix`
// shrinks it; it never grows it.
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { BASELINE_FILE, type Finding } from "./types.ts";
import { cmp, readText } from "./util.ts";

export type Lang = "svelte" | "rust" | "shortcut" | "platform" | "writing";

export interface Violation {
	line: number;
	text: string;
	/** Finding rule when it differs from the ratchet's (writing: `writing-em-dash`). */
	rule?: string;
	/** The offending text as the message shows it, when quoting `text` is not enough (writing: the quoted hit and the fix). */
	detail?: string;
}

/** file -> violations of one ratchet, as the scanners return them. */
export type Found = Map<string, Violation[]>;

export type Baseline = Partial<Record<Lang, Record<string, number>>>;

/** How a ratchet words its findings. */
export interface RatchetText {
	lang: Lang;
	rule: string;
	/** "hardcoded user-facing text" */
	noun: string;
	/** What to do about a new violation. */
	hint: string;
}

export function readBaseline(root: string): Baseline {
	const p = join(root, BASELINE_FILE);
	if (!existsSync(p)) return {};
	const j = JSON.parse(readText(p));
	if (typeof j !== "object" || j === null || Array.isArray(j)) throw new Error(`${BASELINE_FILE}: expected { "<ratchet>": { file: count } }`);
	return j;
}

export function writeBaseline(root: string, b: Baseline) {
	const out: Baseline = {};
	for (const lang of Object.keys(b).sort(cmp) as Lang[]) {
		const section = b[lang] ?? {};
		out[lang] = Object.fromEntries(Object.entries(section).filter(([, n]) => n > 0).sort(([a], [c]) => cmp(a, c)));
	}
	writeFileSync(join(root, BASELINE_FILE), `${JSON.stringify(out, null, "\t")}\n`);
}

/** A path is under one of the scope entries (a directory, or a file named exactly). */
const within = (file: string, prefixes: string[]) => prefixes.some((p) => file === p || file.startsWith(`${p}/`));

/** Ratchet `found` (the files under `prefixes`) against the baseline section of `def.lang`. */
export function ratchet(root: string, def: RatchetText, found: Found, prefixes: string[]): Finding[] {
	const section = readBaseline(root)[def.lang] ?? {};
	const out: Finding[] = [];
	for (const [file, violations] of [...found].sort(([a], [b]) => cmp(a, b))) {
		const allowed = section[file] ?? 0;
		if (violations.length <= allowed) continue;
		for (const v of violations) {
			out.push({ rule: v.rule ?? def.rule, severity: "error", file, line: v.line, message: `${def.noun} ${v.detail ?? JSON.stringify(v.text)} (${violations.length} in this file, baseline ${allowed}); ${def.hint}` });
		}
	}
	for (const [file, allowed] of Object.entries(section).filter(([f]) => within(f, prefixes)).sort(([a], [b]) => cmp(a, b))) {
		const now = found.get(file)?.length ?? 0;
		if (now < allowed) {
			out.push({ rule: `${def.rule}-baseline`, severity: "error", file: BASELINE_FILE, key: file, message: `baseline allows ${allowed} ${def.lang} offender(s) but ${file} has ${now}; run bun run i18n:fix to shrink it` });
		}
	}
	return out;
}

/** The baseline section of `lang` after `found`: every existing entry shrunk to what is left; entries are never
 * added or raised. With `seed`, a section that does not exist yet starts from `found` (the first run). */
export function shrunk(b: Baseline, lang: Lang, found: Found, seed: boolean, prefixes: string[]): Record<string, number> {
	const old = b[lang];
	if (!old) return seed ? Object.fromEntries([...found].map(([f, v]) => [f, v.length])) : {};
	return Object.fromEntries(Object.entries(old).map(([f, n]) => [f, within(f, prefixes) ? Math.min(n, found.get(f)?.length ?? 0) : n]));
}
