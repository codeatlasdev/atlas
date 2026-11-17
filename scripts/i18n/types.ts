// The contract between the runner (check.ts) and its checkers (checks/*.ts), plus the shared repo context.
//
// A checker is a file `scripts/i18n/checks/<name>.ts` whose default export is `{ name, run(ctx), status?(ctx) }`.
// The runner globs the directory, so a later surface (Dart `arb`, ...) is one new file and no edit to a shared one.
import { existsSync } from "node:fs";
import { join } from "node:path";
import { type DiffBase, resolveBase } from "./base.ts";
import { type Glossary, loadGlossary } from "./glossary.ts";
import { readText } from "./util.ts";

/** `info` findings are not printed unless asked for (`--info`); they never fail the run. */
export type Severity = "error" | "warn" | "info";

export interface Finding {
	rule: string;
	severity: Severity;
	/** Repo-relative path, forward slashes. */
	file: string;
	line?: number;
	key?: string;
	message: string;
}

export interface LocaleInfo {
	/** `source` = the language strings are written in; `required` = missing/outdated keys fail the gate;
	 * `community` = they warn and the UI falls back to the source. */
	tier: "source" | "required" | "community";
	owners?: string[];
}

export interface LocalesConfig {
	default: string;
	locales: Record<string, LocaleInfo>;
}

export interface Ctx {
	/** Repo root every path is relative to; tests pass a temp dir. */
	root: string;
	/** `--surface <name>`: a checker reports only this surface (`desktop`, `site`, `rust`, `docs`, ...). */
	surface?: string;
	locales: LocalesConfig;
	glossary: Glossary;
	/** What the change under check is compared with, when the repo is a git checkout with a usable base (see base.ts);
	 * the ratchets then fail only on offenders the change adds. Undefined: compare with the committed baseline. */
	base?: DiffBase;
}

/** Freshness of one locale's area of one surface (`--status`). */
export interface StatusRow {
	surface: string;
	locale: string;
	area: string;
	total: number;
	fresh: number;
	outdated: number;
	missing: number;
}

export interface Checker {
	name: string;
	run(ctx: Ctx): Promise<Finding[]>;
	status?(ctx: Ctx): Promise<StatusRow[]>;
}

export const LOCALES_FILE = "i18n/locales.json";
export const MESSAGES_DIR = "i18n/messages";
export const SURFACES_FILE = "i18n/surfaces.json";
export const BASELINE_FILE = "i18n/hardcoded-baseline.json";

/** `i18n/locales.json`; without the file only the default `en` exists. */
export function loadLocales(root: string): LocalesConfig {
	const p = join(root, LOCALES_FILE);
	if (!existsSync(p)) return { default: "en", locales: { en: { tier: "source" } } };
	const j = JSON.parse(readText(p)) as LocalesConfig;
	if (typeof j.default !== "string" || typeof j.locales !== "object" || j.locales === null) {
		throw new Error(`${LOCALES_FILE}: expected { "default": "<tag>", "locales": { "<tag>": { "tier": ... } } }`);
	}
	if (j.locales[j.default]?.tier !== "source") throw new Error(`${LOCALES_FILE}: the default locale must have tier "source"`);
	for (const [tag, info] of Object.entries(j.locales)) {
		if (!["source", "required", "community"].includes(info?.tier)) throw new Error(`${LOCALES_FILE}: ${tag} has no valid tier`);
	}
	return j;
}

export function loadCtx(root: string, surface?: string): Ctx {
	return { root, surface, locales: loadLocales(root), glossary: loadGlossary(root), base: resolveBase(root) };
}

/** `file:line rule key message`, the line a human or an agent reads. */
export function formatFinding(f: Finding): string {
	const where = f.line ? `${f.file}:${f.line}` : f.file;
	return `${where} ${f.rule} ${f.key ?? "-"} ${f.message}`;
}

/** Severity of a missing/outdated key for `locale`: required locales fail, community ones warn. */
export function tierSeverity(ctx: Ctx, locale: string): Severity {
	return ctx.locales.locales[locale]?.tier === "community" ? "warn" : "error";
}
