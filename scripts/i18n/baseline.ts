// i18n/hardcoded-baseline.jsonl: today's count of offenders per file, per ratchet (`svelte`, `rust`, `shortcut`,
// `platform`, `writing`). It is a ratchet: a file may never have more than its baseline (a new violation fails),
// and a baseline that is now higher than reality fails too, so the number only goes down. `bun run i18n:fix`
// shrinks it; it never grows it.
//
// Built so parallel branches never conflict on it (`.gitattributes`: `merge=union`): one self-contained JSON line per
// (ratchet, file), `{"ratchet":"rust","file":"crates/x.rs","count":3}`. A branch only ever LOWERS a count and never
// deletes a line (a fixed file keeps its line at 0: a union merge resurrects a deleted line, but keeps both versions of
// a lowered one). A path that appears twice, which is what a union merge of two branches lowering the same or adjacent
// lines produces, counts as the smaller number. Lines at 0 are dead weight only until `i18n:fix --prune-baseline`.
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

/** The baseline of `text`: blank lines skipped, a (ratchet, file) listed twice counts as its smaller number. */
export function parseBaseline(text: string, source = BASELINE_FILE): Baseline {
	const out: Baseline = {};
	text.split("\n").forEach((raw, i) => {
		if (raw.trim() === "") return;
		let j: unknown;
		try {
			j = JSON.parse(raw);
		} catch (e) {
			throw new Error(`${source}:${i + 1}: not JSON (${(e as Error).message}); one {"ratchet","file","count"} object per line`);
		}
		const o: Record<string, unknown> = typeof j === "object" && j !== null ? Object.fromEntries(Object.entries(j)) : {};
		if (typeof o.ratchet !== "string" || typeof o.file !== "string" || typeof o.count !== "number" || !Number.isInteger(o.count) || o.count < 0) {
			throw new Error(`${source}:${i + 1}: expected {"ratchet": "<name>", "file": "<path>", "count": <integer >= 0>}`);
		}
		const lang = o.ratchet as Lang;
		const section = (out[lang] ??= {});
		section[o.file] = Math.min(section[o.file] ?? o.count, o.count);
	});
	return out;
}

export function readBaseline(root: string): Baseline {
	const p = join(root, BASELINE_FILE);
	return existsSync(p) ? parseBaseline(readText(p)) : {};
}

/** The file text of `b`, sorted, one line per (ratchet, file). Zero counts are written (see the header) unless `prune`. */
export function renderBaseline(b: Baseline, prune = false): string {
	const lines: string[] = [];
	for (const lang of Object.keys(b).sort(cmp)) {
		for (const [file, count] of Object.entries(b[lang as Lang] ?? {}).sort(([a], [c]) => cmp(a, c))) {
			if (prune && count === 0) continue;
			lines.push(JSON.stringify({ ratchet: lang, file, count }));
		}
	}
	return lines.length ? `${lines.join("\n")}\n` : "";
}

export function writeBaseline(root: string, b: Baseline, prune = false) {
	writeFileSync(join(root, BASELINE_FILE), renderBaseline(b, prune));
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
