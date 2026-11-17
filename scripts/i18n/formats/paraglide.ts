// Paraglide JS / inlang message-format catalogs: <app>/project.inlang/settings.json + <app>/messages/{locale}.json
// (one flat file per locale) + <app>/messages/descriptions.json (English-only translator descriptions).
import { existsSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { type Entry, type Platform, PLATFORMS, type Surface } from "../engine.ts";
import { type ParaglideValue, paraglideProblem, paraglideShape, paraglideTexts } from "../shape.ts";
import type { Ctx, Finding } from "../types.ts";
import { LOCALES_FILE } from "../types.ts";
import { canonicalJson, cmp, detectIndent, readText, rel, walk } from "../util.ts";

export const PLATFORM_KEYS_FILE = "i18n/platform-keys.json";

/** i18n/platform-keys.json: `{ "key": "mac" }` marks a Paraglide key as being about one platform, so it may name that
 * platform's nouns (Finder, Windows) without an `os` select. Fluent keys say it in a `# platform: mac` comment. */
export function loadPlatformKeys(root: string): { map: Map<string, Platform>; problems: Finding[] } {
	const map = new Map<string, Platform>();
	const problems: Finding[] = [];
	const path = join(root, PLATFORM_KEYS_FILE);
	if (!existsSync(path)) return { map, problems };
	const bad = (message: string, key?: string) => problems.push({ rule: "platform-keys", severity: "error", file: PLATFORM_KEYS_FILE, key, message });
	let obj: unknown;
	try {
		obj = JSON.parse(readText(path));
	} catch (e) {
		bad(`invalid JSON: ${(e as Error).message}`);
		return { map, problems };
	}
	if (typeof obj !== "object" || obj === null || Array.isArray(obj)) {
		bad('expected { "<key>": "mac" | "windows" | "linux" }');
		return { map, problems };
	}
	for (const [key, v] of Object.entries(obj)) {
		const p = PLATFORMS.find((x) => x === v);
		if (p) map.set(key, p);
		else bad(`"${String(v)}" is not one of ${PLATFORMS.join(", ")}`, key);
	}
	return { map, problems };
}

export const KEY_PATTERN = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/;
export const DESCRIPTIONS_FILE = "descriptions.json";

export interface ParaglideProject {
	/** Directory that holds `project.inlang` (the app: `apps/desktop`, `site`). */
	dir: string;
	settingsFile: string;
	surface: Surface;
	/** Catalog path (absolute) per locale. */
	catalogs: Map<string, string>;
	descriptionsFile: string;
}

/** An Entry for one catalog value; `i18n:translate` builds candidates with it too. */
export function paraglideEntry(key: string, v: ParaglideValue, file: string, line?: number, description?: string, platform?: Platform): Entry {
	const json = JSON.stringify(v);
	return { key, area: key.split("_")[0], file, line, source: json, raw: json, shape: paraglideShape(v), texts: paraglideTexts(v), description, platform };
}

const fileProblem = (root: string, path: string, rule: string, message: string, line?: number): Finding => ({ rule, severity: "error", file: rel(root, path), line, message });

/** The 1-based line of each top-level key of a catalog, found by its indentation. */
function keyLines(text: string, indent: string): Map<string, number> {
	const out = new Map<string, number>();
	const re = new RegExp(`^${indent}"((?:[^"\\\\]|\\\\.)*)"\\s*:`);
	text.split("\n").forEach((line, i) => {
		const m = re.exec(line);
		if (m && !out.has(m[1])) out.set(JSON.parse(`"${m[1]}"`), i + 1);
	});
	return out;
}

/** Why a catalog is not in canonical form, or undefined when it is. */
export function formatProblem(text: string, obj: Record<string, unknown>): string | undefined {
	if (text.includes("\r")) return "uses CRLF line endings";
	if (!text.endsWith("\n")) return "has no trailing newline";
	const keys = Object.keys(obj).filter((k) => k !== "$schema");
	if (keys.some((k, i) => i > 0 && cmp(keys[i - 1], k) > 0)) return "keys are not sorted";
	if (text !== canonicalJson(obj, detectIndent(text))) return "is not one key per line with consistent indentation";
	return undefined;
}

function readJsonObject(root: string, path: string, problems: Finding[]): { text: string; obj: Record<string, unknown> } | undefined {
	const text = readText(path);
	try {
		const obj = JSON.parse(text);
		if (typeof obj !== "object" || obj === null || Array.isArray(obj)) throw new Error("expected a JSON object");
		return { text, obj };
	} catch (e) {
		problems.push(fileProblem(root, path, "catalog", `invalid JSON: ${(e as Error).message}`));
		return undefined;
	}
}

export function loadParaglide(ctx: Ctx): ParaglideProject[] {
	const { root } = ctx;
	const settingsFiles = walk(root, (p) => p.replaceAll("\\", "/").endsWith("/project.inlang/settings.json"));
	const known = existsSync(join(root, LOCALES_FILE));
	const platformKeys = loadPlatformKeys(root).map;
	return settingsFiles.map((settingsFile) => {
		const dir = dirname(dirname(settingsFile));
		const name = basename(dir);
		const problems: Finding[] = [];
		let settings: { baseLocale?: string; locales?: string[]; [k: string]: unknown } = {};
		try {
			settings = JSON.parse(readText(settingsFile));
		} catch (e) {
			problems.push(fileProblem(root, settingsFile, "config", `invalid JSON: ${(e as Error).message}`));
		}
		const base = settings.baseLocale ?? ctx.locales.default;
		const locales = Array.isArray(settings.locales) && settings.locales.length ? settings.locales : [base];
		if (!locales.includes(base)) problems.push(fileProblem(root, settingsFile, "config", `baseLocale "${base}" is not in locales`));
		if (known) {
			for (const l of locales) if (!ctx.locales.locales[l]) problems.push(fileProblem(root, settingsFile, "config", `locale "${l}" is not in ${LOCALES_FILE}`));
			for (const [l, info] of Object.entries(ctx.locales.locales)) {
				if (info.tier === "required" && !locales.includes(l)) problems.push(fileProblem(root, settingsFile, "config", `required locale "${l}" of ${LOCALES_FILE} is not in locales`));
			}
		}
		const plugin = settings["plugin.inlang.messageFormat"];
		const pattern = typeof plugin === "object" && plugin !== null && "pathPattern" in plugin ? plugin.pathPattern : undefined;
		const template = typeof pattern === "string" ? pattern : "./messages/{locale}.json";
		const catalogs = new Map(locales.map((l) => [l, resolve(dir, template.replaceAll("{locale}", l))]));
		const baseCatalog = catalogs.get(base) ?? join(dir, "messages", `${base}.json`);
		const descriptionsFile = join(dirname(baseCatalog), DESCRIPTIONS_FILE);

		const descriptions = new Map<string, string>();
		if (existsSync(descriptionsFile)) {
			const d = readJsonObject(root, descriptionsFile, problems);
			if (d) {
				for (const [k, v] of Object.entries(d.obj)) {
					if (typeof v === "string") descriptions.set(k, v);
					else problems.push({ ...fileProblem(root, descriptionsFile, "description", "a description must be a string"), key: k });
				}
				const why = formatProblem(d.text, d.obj);
				if (why) problems.push(fileProblem(root, descriptionsFile, "format", `${why}; run bun run i18n:fix`));
			}
		}

		const entries = new Map<string, Map<string, Entry>>();
		for (const [locale, path] of catalogs) {
			const map = new Map<string, Entry>();
			entries.set(locale, map);
			if (!existsSync(path)) continue;
			const c = readJsonObject(root, path, problems);
			if (!c) continue;
			const why = formatProblem(c.text, c.obj);
			if (why) problems.push(fileProblem(root, path, "format", `${why}; run bun run i18n:fix`));
			const lines = keyLines(c.text, detectIndent(c.text));
			for (const [key, value] of Object.entries(c.obj)) {
				if (key === "$schema") continue;
				const bad = paraglideProblem(value);
				if (bad) {
					problems.push({ ...fileProblem(root, path, "catalog", bad, lines.get(key)), key });
					continue;
				}
				const v = value as ParaglideValue;
				map.set(key, paraglideEntry(key, v, rel(root, path), lines.get(key), locale === base ? descriptions.get(key) : undefined, locale === base ? platformKeys.get(key) : undefined));
			}
		}
		for (const k of descriptions.keys()) {
			if (!entries.get(base)?.has(k)) problems.push({ ...fileProblem(root, descriptionsFile, "description", "describes a key that is not in the source catalog"), key: k });
		}

		const surface: Surface = {
			name,
			kind: "paraglide",
			base,
			locales,
			entries,
			problems,
			keyPattern: KEY_PATTERN,
			keyHint: "area_snake_name (lowercase, an area, an underscore, a name)",
			catalogFile: (l) => rel(root, catalogs.get(l) ?? join(dir, "messages", `${l}.json`)),
		};
		return { dir, settingsFile, surface, catalogs, descriptionsFile };
	});
}
