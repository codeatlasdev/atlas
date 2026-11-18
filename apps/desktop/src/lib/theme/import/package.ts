// Scans an unpacked theme package (zip / vsix / folder) and turns every theme it holds into an
// ImportResult. Pure: the caller lists the files and injects `read`.
import { m } from "#lib/i18n/m.ts";
import { complete } from "../derive.ts";
import { parseFamily, type ThemeStyle } from "../schema.ts";
import { parseAlacritty } from "./alacritty.ts";
import { parseBase16 } from "./base16.ts";
import { appearanceOf, type ImportResult } from "./color.ts";
import { detectFormat } from "./detect.ts";
import { parseGhosttyConfig } from "./ghostty.ts";
import { parseItermColors } from "./iterm.ts";
import { parseJsonc, parseVscodeTheme } from "./vscode.ts";

export interface PackageFile {
	rel: string;
	size: number;
}

export interface PackageCtx {
	/** UTF-8 text of a file, or null when it cannot be read. */
	read(rel: string): Promise<string | null>;
}

export type FoundFormat = "vscode" | "ghostty" | "iterm" | "alacritty" | "base16" | "zed";

export interface FoundTheme {
	/** Stable across scans: the group key plus the appearance. */
	id: string;
	name: string;
	appearance: "dark" | "light";
	/** Every file merged into `result`, base first. */
	sources: string[];
	result: ImportResult;
	/** 8 hex colors: background, foreground, red, green, yellow, blue, magenta, cyan. */
	swatches: string[];
	format: FoundFormat;
}

export interface ScanResult {
	themes: FoundTheme[];
	skipped: string[];
	warnings: string[];
}

/** Files bigger than this are never themes (the Rust read cap is 2 MB too). */
const MAX_BYTES = 2 * 1024 * 1024;

const NOISE = /(^|\/)(__MACOSX|node_modules|\.git)\//;
const IGNORED_EXT: Record<string, true> = {
	md: true,
	txt: true,
	png: true,
	jpg: true,
	jpeg: true,
	gif: true,
	svg: true,
	webp: true,
	ico: true,
	icns: true,
	ttf: true,
	otf: true,
	woff: true,
	woff2: true,
	pdf: true,
	html: true,
	css: true,
	js: true,
	mjs: true,
	ts: true,
	lock: true,
	vsixmanifest: true,
};
/** Extension-less files that are never themes. */
const IGNORED_BARE = /^(license|licence|readme|changelog|notice|authors|copying|contributing|\.ds_store)/i;

/** Merge-base preference: VS Code carries syntax + UI, then the other families, Ghostty last. */
const RANK: Record<FoundFormat, number> = {
	vscode: 0,
	zed: 1,
	base16: 2,
	iterm: 3,
	alacritty: 4,
	ghostty: 5,
};

interface Candidate {
	rel: string;
	format: FoundFormat;
	result: ImportResult;
	/** Name from the package manifest, preferred for display. */
	label?: string;
}

interface Contributed {
	label: string;
	uiTheme?: "dark" | "light";
}

const isObj = (v: unknown): v is Record<string, unknown> =>
	typeof v === "object" && v !== null && !Array.isArray(v);

const extOf = (rel: string) => {
	const base = rel.split("/").pop() ?? rel;
	const i = base.lastIndexOf(".");
	return i <= 0 ? "" : base.slice(i + 1).toLowerCase();
};

/** `./themes/Foo.json`, `themes\Foo.json` -> `themes/foo.json` for matching manifest paths. */
const normPath = (p: string) =>
	p
		.replace(/\\/g, "/")
		.split("/")
		.filter((seg) => seg !== "" && seg !== ".")
		.join("/")
		.toLowerCase();

/** Generic grouping key: lowercase alphanumerics, minus the words theme / color(s). */
export function groupKey(name: string): string {
	const words = name
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, " ")
		.split(" ")
		.filter((w) => w && w !== "theme" && w !== "color" && w !== "colors");
	return words.join("") || name.toLowerCase().replace(/[^a-z0-9]+/g, "");
}

/** `package.json` `contributes.themes` -> normalized theme path -> label + uiTheme. */
function readManifest(text: string, dir: string): Record<string, Contributed> {
	const out: Record<string, Contributed> = {};
	let json: unknown;
	try {
		json = parseJsonc(text);
	} catch {
		return out;
	}
	if (!isObj(json) || !isObj(json.contributes) || !Array.isArray(json.contributes.themes)) return out;
	for (const t of json.contributes.themes) {
		if (!isObj(t) || typeof t.path !== "string") continue;
		const label = typeof t.label === "string" ? t.label.trim() : "";
		if (!label) continue;
		const uiTheme = t.uiTheme === "vs" || t.uiTheme === "hc-light" ? "light" : t.uiTheme ? "dark" : undefined;
		out[normPath(dir ? `${dir}/${t.path}` : t.path)] = { label, uiTheme };
	}
	return out;
}

/** bg, fg and six accents of the style as the app would resolve it. */
function swatchesOf(style: Record<string, unknown>, appearance: "dark" | "light"): string[] {
	const s = complete(style as ThemeStyle, appearance);
	return [
		"terminal.background",
		"terminal.foreground",
		"terminal.ansi.red",
		"terminal.ansi.green",
		"terminal.ansi.yellow",
		"terminal.ansi.blue",
		"terminal.ansi.magenta",
		"terminal.ansi.cyan",
	].map((k) => String(s[k] ?? "#000000").slice(0, 7));
}

/** Classifies one file into zero or more themes. Throws on a file that looks like a theme but is broken. */
async function classify(
	rel: string,
	text: string,
	manifest: Record<string, Contributed>,
): Promise<Candidate[]> {
	const ext = extOf(rel);
	const file = rel.split("/").pop() ?? rel;
	const stem = file.replace(/\.[^.]+$/, "");

	if (ext === "toml") {
		if (!/^\s*\[colors[.\]]/m.test(text)) return [];
		return [{ rel, format: "alacritty", result: parseAlacritty(text, rel) }];
	}
	if (ext === "yml" || ext === "yaml") {
		// Alacritty legacy has a top-level `colors:`; Base16 has base00.. keys (optionally under `palette:`).
		if (/^colors\s*:/m.test(text)) return [{ rel, format: "alacritty", result: parseAlacritty(text, rel) }];
		if (/^\s*("?base00"?|palette)\s*:/im.test(text)) {
			return [{ rel, format: "base16", result: parseBase16(text) }];
		}
		return [];
	}

	if (ext === "json" || ext === "jsonc") parseJsonc(text); // throws on broken JSON: reported by the caller

	switch (detectFormat(rel, text)) {
		case "iterm":
			return [{ rel, format: "iterm", result: parseItermColors(text, stem) }];
		case "vscode": {
			const contributed = manifest[normPath(rel)];
			const result = parseVscodeTheme(text, contributed?.label ?? stem);
			const json = parseJsonc(text);
			if (isObj(json)) {
				// The manifest only fills what the theme file does not say itself.
				if (typeof json.type !== "string" && contributed?.uiTheme) result.appearance = contributed.uiTheme;
				if (typeof json.name !== "string" || !json.name.trim())
					result.name = contributed?.label ?? result.name;
			}
			return [{ rel, format: "vscode", result, label: contributed?.label }];
		}
		case "zed":
		case "atlas": {
			const { themes, issues } = parseFamily(parseJsonc(text));
			if (themes.length === 0) {
				throw new Error(
					issues.find((i) => i.level === "error")?.message ?? m.settings_theme_diag_no_themes(),
				);
			}
			const warnings = issues.filter((i) => i.level !== "error").map((i) => `${i.path}: ${i.message}`);
			return themes.map((t) => ({
				rel,
				format: "zed" as const,
				result: { name: t.name, appearance: t.appearance, style: t.style, font: t.font, warnings },
			}));
		}
		case "base16":
			return [{ rel, format: "base16", result: parseBase16(text) }];
		case "ghostty": {
			const result = await parseGhosttyConfig(text, { home: "/", readFile: async () => null, themeDirs: [] });
			// The importer names it after a local config; inside a package the file name is the name.
			const name = stem
				.replace(/[-_.]+/g, " ")
				.trim()
				.replace(/\b[a-z]/g, (c) => c.toUpperCase());
			return [{ rel, format: "ghostty", result: { ...result, name } }];
		}
		default:
			// A `.json` that is no theme (tsconfig, a lockfile, …) is simply not for us.
			return [];
	}
}

export async function scanPackage(files: PackageFile[], ctx: PackageCtx): Promise<ScanResult> {
	const skipped: string[] = [];
	const warnings: string[] = [];
	const list = files
		.map((f) => ({ ...f, rel: f.rel.replace(/\\/g, "/") }))
		.filter((f) => {
			if (NOISE.test(`/${f.rel}`) || f.size > MAX_BYTES) return false;
			const base = f.rel.split("/").pop() ?? f.rel;
			if (base.startsWith("._")) return false;
			const ext = extOf(f.rel);
			return ext === "" ? !IGNORED_BARE.test(base) : !IGNORED_EXT[ext];
		});

	// package.json manifests first: they name and classify the VS Code themes.
	const manifest: Record<string, Contributed> = {};
	const isManifest = (rel: string) => (rel.split("/").pop() ?? rel).toLowerCase() === "package.json";
	for (const f of list.filter((f) => isManifest(f.rel))) {
		const text = await ctx.read(f.rel);
		if (text !== null) Object.assign(manifest, readManifest(text, f.rel.split("/").slice(0, -1).join("/")));
	}

	const candidates: Candidate[] = [];
	for (const f of list.filter((f) => !isManifest(f.rel))) {
		const text = await ctx.read(f.rel);
		if (text === null) {
			// Only complain about unreadable files that could have been themes.
			if (["json", "jsonc", "itermcolors", "toml", "yml", "yaml"].includes(extOf(f.rel))) {
				skipped.push(f.rel);
				warnings.push(m.settings_theme_diag_file_unreadable({ file: f.rel }));
			}
			continue;
		}
		try {
			candidates.push(...(await classify(f.rel, text, manifest)));
		} catch (e) {
			skipped.push(f.rel);
			warnings.push(`${f.rel}: ${e instanceof Error ? e.message : String(e)}`);
		}
	}

	// Group the same variant across formats. Zed/Atlas family members are kept as they are.
	const groups = new Map<string, Candidate[]>();
	for (const c of candidates) {
		const key =
			c.format === "zed"
				? `zed:${c.rel}:${c.result.name}`
				: `${groupKey(c.result.name)}:${c.result.appearance}`;
		groups.set(key, [...(groups.get(key) ?? []), c]);
	}

	const found: FoundTheme[] = [];
	for (const [key, group] of groups) {
		group.sort((a, b) => RANK[a.format] - RANK[b.format] || a.rel.localeCompare(b.rel));
		const [base, ...rest] = group;
		const style: Record<string, unknown> = structuredClone(base.result.style);
		const sources = [base.rel];
		const merged = [...base.result.warnings];
		let { font, terminal } = base.result;
		for (const other of rest) {
			// Fill what the base lacks: the terminal palette, cursor and selection.
			for (const [k, v] of Object.entries(other.result.style)) {
				if (k.startsWith("terminal.") && style[k] === undefined) style[k] = v;
			}
			sources.push(other.rel);
			font ??= other.result.font;
			terminal ??= other.result.terminal;
			for (const w of other.result.warnings) if (!merged.includes(w)) merged.push(w);
		}
		const appearance =
			base.result.appearance ??
			appearanceOf((style["editor.background"] ?? style["terminal.background"]) as string | undefined);
		const name = base.label ?? base.result.name;
		const result: ImportResult = { name, appearance, style, warnings: merged };
		if (font) result.font = font;
		if (terminal) result.terminal = terminal;
		found.push({
			id: key,
			name,
			appearance,
			sources,
			result,
			swatches: swatchesOf(style, appearance),
			format: base.format,
		});
	}

	// Identical names (per appearance) across folders: keep the one that merged the most files.
	const byName = new Map<string, FoundTheme>();
	for (const t of found) {
		const k = `${t.appearance}:${t.name.toLowerCase()}`;
		const prev = byName.get(k);
		if (!prev || t.sources.length > prev.sources.length) byName.set(k, t);
	}
	const themes = [...byName.values()].sort(
		(a, b) =>
			(a.appearance === b.appearance ? 0 : a.appearance === "dark" ? -1 : 1) || a.name.localeCompare(b.name),
	);
	return { themes, skipped, warnings };
}
