// The one source tree: i18n/messages/<area>/{en.json,pt-BR.json,meta.json,area.json,lock.json}, and i18n/surfaces.json
// (per-surface config only). Which surface consumes an area is declared by the area itself, in its own area.json, so
// two branches that add areas never touch the same line. Everything that reads or generates messages goes through
// `loadSource`.
import { existsSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type Area, type Entry, type MetaEntry, PLATFORMS } from "./model.ts";
import { type MsgValue, MessageError, inlineRefs } from "./resolve.ts";
import { type ParaglideValue, paraglideProblem, paraglideShape, paraglideTexts } from "./shape.ts";
import { type Ctx, type Finding, LOCALES_FILE, MESSAGES_DIR, SURFACES_FILE } from "./types.ts";
import { canonicalJson, cmp, detectIndent, readText } from "./util.ts";

export const KEY_PATTERN = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/;
export const AREA_PATTERN = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$/;

/** The three areas every surface consumes: shared actions and states, OS nouns, key names. */
export const COMMON_AREAS = ["common", "common_keys", "common_os"];
export const isCommon = (area: string) => COMMON_AREAS.includes(area);

/** The file in an area's folder that says which surface consumes it (and whether its English is generated). */
export const AREA_FILE = "area.json";

export interface SurfaceDef {
	/** The areas whose area.json names this surface (derived by `loadSurfacesConfig`; surfaces.json never lists them). */
	areas: string[];
	/** Directory the Paraglide inputs (`<locale>.json`) are generated into (gitignored). */
	paraglide?: string;
	/** Directory the Flutter ARB files are generated into at build time (gitignored): the mobile surface. */
	arb?: string;
	/** Root of the Rust sources. Rust reads the JSON tree itself at build time (crates/atlas-i18n/build.rs): nothing is generated. */
	rust?: string;
}

export interface SurfacesConfig {
	budget: { keys: number; bytes: number };
	surfaces: Record<string, SurfaceDef>;
	/** Areas whose English side is itself generated: area -> the command that regenerates it (their area.json `generated`). */
	generatedAreas: Record<string, string>;
}

/** The content of `i18n/messages/<area>/area.json`. */
export interface AreaFile {
	surface: string;
	/** Set when the English side of the area is generated: the command that regenerates it. */
	generated?: string;
}

export interface Source {
	root: string;
	/** Every parsed value of every area for one locale (a reference is inlined from here, falling back to English). */
	lookup(locale: string): (key: string) => MsgValue | undefined;
	config: SurfacesConfig;
	areas: Map<string, Area>;
	/** Problems of surfaces.json and of the tree as a whole (an unknown locale file, a bad area name). */
	problems: Finding[];
}

const problem = (rule: string, file: string, message: string, key?: string, line?: number): Finding => ({ rule, severity: "error", file, line, key, message });

const OUTPUTS = ["paraglide", "arb", "rust"] as const;

/** surfaces.json (budget and the output of each surface) plus the area.json of every area folder: who consumes what. */
export function loadSurfacesConfig(root: string): { config: SurfacesConfig; problems: Finding[] } {
	const config: SurfacesConfig = { budget: { keys: 150, bytes: 24576 }, surfaces: {}, generatedAreas: {} };
	const problems: Finding[] = [];
	const path = join(root, SURFACES_FILE);
	if (!existsSync(path)) return { config, problems };
	let j: unknown;
	try {
		j = JSON.parse(readText(path));
	} catch (e) {
		return { config, problems: [problem("config", SURFACES_FILE, `invalid JSON: ${(e as Error).message}`)] };
	}
	const bad = (m: string) => problems.push(problem("config", SURFACES_FILE, m));
	if (typeof j !== "object" || j === null || !("surfaces" in j) || typeof j.surfaces !== "object" || j.surfaces === null) {
		bad('expected { "budget": {...}, "surfaces": { "<name>": { "paraglide" | "arb" | "rust": "<dir>" } } }');
		return { config, problems };
	}
	if ("budget" in j && typeof j.budget === "object" && j.budget !== null) {
		const b: Record<string, unknown> = Object.fromEntries(Object.entries(j.budget));
		if (typeof b.keys === "number" && typeof b.bytes === "number") config.budget = { keys: b.keys, bytes: b.bytes };
		else bad("budget needs numbers `keys` and `bytes`");
	}
	if ("generatedAreas" in j) bad(`"generatedAreas" moved into the area.json of each generated area ({ "surface": "...", "generated": "<command>" }): run bun run i18n:migrate-registry`);
	for (const [name, def] of Object.entries(j.surfaces)) {
		const d: Record<string, unknown> = typeof def === "object" && def !== null ? Object.fromEntries(Object.entries(def)) : {};
		if ("areas" in d) bad(`surface "${name}" lists areas: the area names moved into i18n/messages/<area>/area.json ({ "surface": "${name}" }); run bun run i18n:migrate-registry`);
		const outputs = OUTPUTS.filter((k) => typeof d[k] === "string");
		if (outputs.length !== 1) bad(`surface "${name}" needs exactly one of paraglide, arb, rust (a directory)`);
		const surface: SurfaceDef = { areas: [] };
		for (const k of OUTPUTS) if (typeof d[k] === "string") surface[k] = d[k];
		config.surfaces[name] = surface;
	}
	problems.push(...loadAreaFiles(root, config));
	return { config, problems };
}

/** Read every area's area.json into `config` (areas of each surface, generated commands); the problems are an area with
 * no declaration, an unknown surface, a malformed file, or a common area that declares one. */
function loadAreaFiles(root: string, config: SurfacesConfig): Finding[] {
	const out: Finding[] = [];
	const dir = join(root, MESSAGES_DIR);
	if (!existsSync(dir)) return out;
	const names = Object.keys(config.surfaces).sort(cmp).join("|");
	const fixHint = (area: string) => `write ${MESSAGES_DIR}/${area}/${AREA_FILE} as { "surface": "<${names}>" } (bun run i18n:add --new-area --surface <s> creates it)`;
	for (const area of readdirSync(dir, { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name).sort()) {
		const file = `${MESSAGES_DIR}/${area}/${AREA_FILE}`;
		const path = join(root, file);
		if (!existsSync(path)) {
			if (!isCommon(area)) out.push(problem("area-surface", `${MESSAGES_DIR}/${area}`, `area "${area}" declares no surface, so nothing consumes it: ${fixHint(area)}`));
			continue;
		}
		let j: unknown;
		try {
			j = JSON.parse(readText(path));
		} catch (e) {
			out.push(problem("area-surface", file, `invalid JSON: ${(e as Error).message}`));
			continue;
		}
		const o: Record<string, unknown> = typeof j === "object" && j !== null && !Array.isArray(j) ? Object.fromEntries(Object.entries(j)) : {};
		const unknown = Object.keys(o).filter((k) => k !== "surface" && k !== "generated");
		if (unknown.length) out.push(problem("area-surface", file, `unknown field(s): ${unknown.join(", ")}; an area.json holds "surface" and, for a generated area, "generated"`));
		if (isCommon(area)) {
			out.push(problem("area-surface", file, `"${area}" is consumed by every surface already; delete this file`));
			continue;
		}
		const surface = o.surface;
		if (typeof surface !== "string") out.push(problem("area-surface", file, `area "${area}" needs a string "surface": ${fixHint(area)}`));
		else if (!config.surfaces[surface]) out.push(problem("area-surface", file, `area "${area}" names surface "${surface}", which i18n/surfaces.json does not define (${names}): ${fixHint(area)}`));
		else config.surfaces[surface].areas.push(area);
		if (o.generated !== undefined) {
			if (typeof o.generated === "string" && o.generated.trim()) config.generatedAreas[area] = o.generated;
			else out.push(problem("area-surface", file, '"generated" is the command that regenerates the English of this area'));
		}
	}
	return out;
}

/** Create (or rewrite) the area.json of an area folder. */
export function writeAreaFile(root: string, area: string, a: AreaFile) {
	writeFileSync(join(root, MESSAGES_DIR, area, AREA_FILE), canonicalJson({ ...a }));
}

/** surfaces.json in the layout it is kept in: one line per surface. */
export function renderSurfaces(c: SurfacesConfig): string {
	const surfaces = Object.keys(c.surfaces).sort(cmp).map((name) => {
		const d = c.surfaces[name];
		const output = OUTPUTS.filter((k) => d[k] !== undefined).map((k) => `${JSON.stringify(k)}: ${JSON.stringify(d[k])}`);
		return `\t\t${JSON.stringify(name)}: { ${output.join(", ")} }`;
	});
	return `{\n\t"budget": { "keys": ${c.budget.keys}, "bytes": ${c.budget.bytes} },\n\t"surfaces": {\n${surfaces.join(",\n")}\n\t}\n}\n`;
}

/** The areas a surface consumes: the common ones, then its own, without duplicates. */
export const consumedAreas = (config: SurfacesConfig, surface: string): string[] => [...new Set([...COMMON_AREAS, ...(config.surfaces[surface]?.areas ?? [])])];

/** Surfaces that consume `area`. */
export const consumers = (config: SurfacesConfig, area: string): string[] => Object.keys(config.surfaces).filter((s) => consumedAreas(config, s).includes(area));

/** Why a JSON file is not in canonical form (sorted keys, one per line, LF, trailing newline), or undefined. */
export function formatProblem(text: string, obj: Record<string, unknown>): string | undefined {
	if (text.includes("\r")) return "uses CRLF line endings";
	if (!text.endsWith("\n")) return "has no trailing newline";
	const keys = Object.keys(obj);
	if (keys.some((k, i) => i > 0 && cmp(keys[i - 1], k) > 0)) return "keys are not sorted";
	if (text !== canonicalJson(obj, detectIndent(text))) return "is not one key per line with consistent indentation";
	return undefined;
}

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

interface RawFile {
	file: string;
	text: string;
	obj: Record<string, unknown>;
	lines: Map<string, number>;
}

function readObject(root: string, file: string, problems: Finding[]): RawFile | undefined {
	const path = join(root, file);
	if (!existsSync(path)) return undefined;
	const text = readText(path);
	try {
		const obj = JSON.parse(text);
		if (typeof obj !== "object" || obj === null || Array.isArray(obj)) throw new Error("expected a JSON object");
		const why = formatProblem(text, obj);
		if (why) problems.push(problem("format", file, `${why}; run bun run i18n:fix`));
		return { file, text, obj, lines: keyLines(text, detectIndent(text)) };
	} catch (e) {
		problems.push(problem("catalog", file, `invalid JSON: ${(e as Error).message}`));
		return undefined;
	}
}

/** The description and flags of one meta.json entry, or why it is malformed. */
function metaEntry(v: unknown): MetaEntry | string {
	if (typeof v === "string") return { description: v };
	if (typeof v !== "object" || v === null || Array.isArray(v)) return "a meta entry is a description string or an object";
	const o: Record<string, unknown> = Object.fromEntries(Object.entries(v));
	const unknown = Object.keys(o).filter((k) => !["description", "platform", "distinct", "maxLength", "sentence"].includes(k));
	if (unknown.length) return `unknown meta field(s): ${unknown.join(", ")}`;
	if (typeof o.description !== "string") return "an object meta entry needs a string `description`";
	const out: MetaEntry = { description: o.description };
	if (o.platform !== undefined) {
		const p = PLATFORMS.find((x) => x === o.platform);
		if (!p) return `platform must be one of ${PLATFORMS.join(", ")}`;
		out.platform = p;
	}
	if (o.distinct !== undefined) {
		if (typeof o.distinct !== "string" || o.distinct.trim() === "") return "`distinct` needs a reason";
		out.distinct = o.distinct;
	}
	if (o.sentence !== undefined) {
		if (typeof o.sentence !== "string" || o.sentence.trim() === "") return "`sentence` needs a reason";
		out.sentence = o.sentence;
	}
	if (o.maxLength !== undefined) {
		if (typeof o.maxLength !== "number" || !Number.isInteger(o.maxLength) || o.maxLength < 1) return "`maxLength` must be a positive integer";
		out.maxLength = o.maxLength;
	}
	return out;
}

/** The `i18n-distinct: reason` marker of a description. */
const distinctOf = (m: MetaEntry) => m.distinct ?? /i18n-distinct:\s*(\S[^\n]*)/.exec(m.description)?.[1]?.trim();

/** An Entry for one value: references inlined through `lookup`, shape and texts taken from the resolved message. */
export function makeEntry(o: { key: string; area: string; file: string; line?: number; value: MsgValue; lookup: (key: string) => MsgValue | undefined; meta?: MetaEntry }): Entry {
	let resolved: MsgValue = o.value;
	let whole: MsgValue = o.value;
	let refError: string | undefined;
	try {
		resolved = inlineRefs(o.value, o.lookup, [o.key]);
		whole = inlineRefs(o.value, o.lookup, [o.key], false);
	} catch (e) {
		if (!(e instanceof MessageError)) throw e;
		refError = e.message;
	}
	return {
		key: o.key,
		area: o.area,
		file: o.file,
		line: o.line,
		value: o.value,
		source: JSON.stringify(o.value),
		raw: JSON.stringify(o.value),
		resolved,
		refError,
		shape: paraglideShape(whole),
		texts: paraglideTexts(whole),
		description: o.meta?.description,
		platform: o.meta?.platform,
		distinct: o.meta ? distinctOf(o.meta) : undefined,
		maxLength: o.meta?.maxLength,
		sentence: o.meta?.sentence,
	};
}

export function loadSource(ctx: Ctx): Source {
	const { root } = ctx;
	const { config, problems } = loadSurfacesConfig(root);
	const base = ctx.locales.default;
	const locales = [base, ...Object.keys(ctx.locales.locales).filter((l) => l !== base)];
	const dir = join(root, MESSAGES_DIR);
	const names = existsSync(dir) ? readdirSync(dir, { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name).sort() : [];
	const areas = new Map<string, Area>();
	const raw = new Map<string, Map<string, RawFile>>();

	for (const name of names) {
		const adir = `${MESSAGES_DIR}/${name}`;
		const aproblems: Finding[] = [];
		if (!AREA_PATTERN.test(name)) aproblems.push(problem("config", adir, "an area directory is snake_case (lowercase words joined by underscores)"));
		for (const f of readdirSync(join(dir, name))) {
			const m = /^(.+)\.json$/.exec(f);
			if (!m || m[1] === "meta" || m[1] === "lock" || m[1] === "area" || locales.includes(m[1])) continue;
			aproblems.push(problem("config", `${adir}/${f}`, `not a locale of ${LOCALES_FILE}; an area holds <locale>.json, meta.json, area.json and lock.json (add a language with bun run i18n:add-locale <tag>)`));
		}
		const files = new Map<string, RawFile>();
		for (const l of locales) {
			const r = readObject(root, `${adir}/${l}.json`, aproblems);
			if (r) files.set(l, r);
		}
		raw.set(name, files);
		const meta = new Map<string, MetaEntry>();
		const mraw = readObject(root, `${adir}/meta.json`, aproblems);
		for (const [k, v] of Object.entries(mraw?.obj ?? {})) {
			const e = metaEntry(v);
			if (typeof e === "string") aproblems.push(problem("description", `${adir}/meta.json`, e, k, mraw?.lines.get(k)));
			else meta.set(k, e);
		}
		areas.set(name, { name, dir: adir, base, locales, entries: new Map(locales.map((l) => [l, new Map()])), meta, problems: aproblems, generated: config.generatedAreas[name] });
	}

	// Every value of every area per locale, so a reference can be inlined from any area.
	const values = new Map<string, Map<string, ParaglideValue>>(locales.map((l) => [l, new Map()]));
	for (const [name, files] of raw) {
		const area = areas.get(name);
		for (const [l, f] of files) {
			for (const [key, v] of Object.entries(f.obj)) {
				const bad = paraglideProblem(v);
				if (bad) area?.problems.push(problem("catalog", f.file, bad, key, f.lines.get(key)));
				else values.get(l)?.set(key, v as ParaglideValue);
			}
		}
	}
	const lookup = (l: string) => (key: string): MsgValue | undefined => values.get(l)?.get(key) ?? values.get(base)?.get(key);

	for (const [name, files] of raw) {
		const area = areas.get(name);
		if (!area) continue;
		for (const [l, f] of files) {
			const map = area.entries.get(l);
			for (const key of Object.keys(f.obj)) {
				const value = values.get(l)?.get(key);
				if (value === undefined) continue;
				const entry = makeEntry({ key, area: name, file: f.file, line: f.lines.get(key), value, lookup: lookup(l), meta: l === base ? area.meta.get(key) : undefined });
				map?.set(key, entry);
			}
		}
		for (const k of area.meta.keys()) {
			if (!area.entries.get(base)?.has(k)) area.problems.push(problem("description", `${area.dir}/meta.json`, "describes a key that is not in the English catalog of this area", k));
		}
	}
	return { root, lookup, config, areas, problems };
}

/** The areas of `source` a run covers: those `surface` consumes, or every area when there is no filter. */
export function areasFor(source: Source, surface?: string): Area[] {
	if (!surface) return [...source.areas.values()];
	const wanted = new Set(consumedAreas(source.config, surface));
	return [...source.areas.values()].filter((a) => wanted.has(a.name));
}

/** Longest area name that prefixes `key` (`common_os_trash` is in `common_os`, not `common`). */
export function areaOfKey(key: string, areaNames: string[]): string | undefined {
	return [...areaNames].sort((a, b) => b.length - a.length).find((a) => key.startsWith(`${a}_`));
}
