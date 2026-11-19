import { m } from "#lib/i18n/m.ts";
import { ThemeImportError } from "./errors.ts";
import { ANSI_KEYS, appearanceOf, normalizeColor, type ImportResult } from "./color.ts";

export interface GhosttyContext {
	home: string;
	readFile(path: string): Promise<string | null>;
	/** Searched in order for a theme file named exactly `<name>`. */
	themeDirs: string[];
	appearance?: "dark" | "light";
}

type Entry = [key: string, value: string];

const MAX_INCLUDE_DEPTH = 10;

const COLOR_KEYS: Record<string, string> = {
	background: "terminal.background",
	foreground: "terminal.foreground",
	"cursor-color": "terminal.cursor",
	"cursor-text": "terminal.cursor_text",
	"selection-background": "terminal.selection.background",
	"selection-foreground": "terminal.selection.foreground",
};

const unquote = (v: string) => v.trim().replace(/^"(.*)"$/s, "$1");

function dirname(path: string): string {
	const i = path.lastIndexOf("/");
	return i <= 0 ? "/" : path.slice(0, i);
}

function resolvePath(path: string, base: string, home: string): string {
	let full = path;
	if (path === "~") full = home;
	else if (path.startsWith("~/")) full = `${home}/${path.slice(2)}`;
	else if (!path.startsWith("/")) full = `${base}/${path}`;
	// Collapse `.` / `..` so include cycles are detected by path identity.
	const parts: string[] = [];
	for (const seg of full.split("/")) {
		if (seg === "..") parts.pop();
		else if (seg && seg !== ".") parts.push(seg);
	}
	return `/${parts.join("/")}`;
}

/** Splits `key = value` lines; blank lines and `#` comment lines are skipped. */
function parseLines(text: string): Entry[] {
	const out: Entry[] = [];
	for (const line of text.split(/\r?\n/)) {
		const t = line.trim();
		if (!t || t.startsWith("#")) continue;
		const eq = t.indexOf("=");
		if (eq < 0) continue;
		const key = t.slice(0, eq).trim();
		if (key) out.push([key, unquote(t.slice(eq + 1))]);
	}
	return out;
}

/** Entries of `text` with `config-file` includes expanded in place (depth + cycle guarded). */
async function expand(
	text: string,
	file: string,
	ctx: GhosttyContext,
	seen: Set<string>,
	warn: (message: string) => void,
	depth = 0,
): Promise<Entry[]> {
	const out: Entry[] = [];
	for (const entry of parseLines(text)) {
		if (entry[0] !== "config-file") {
			out.push(entry);
			continue;
		}
		const optional = entry[1].startsWith("?");
		const path = resolvePath(unquote(optional ? entry[1].slice(1) : entry[1]), dirname(file), ctx.home);
		if (seen.has(path)) {
			warn(m.settings_theme_diag_config_cycle({ path }));
			continue;
		}
		if (depth >= MAX_INCLUDE_DEPTH) {
			warn(m.settings_theme_diag_config_deep({ path }));
			continue;
		}
		const body = await ctx.readFile(path);
		if (body === null) {
			if (!optional) warn(m.settings_theme_diag_config_missing({ path }));
			continue;
		}
		out.push(...(await expand(body, path, ctx, new Set(seen).add(path), warn, depth + 1)));
	}
	return out;
}

/** `light:A,dark:B` -> the pick for `appearance`; plain names pass through. */
function pickTheme(value: string, appearance: "dark" | "light"): string {
	if (!/(^|,)\s*(light|dark)\s*:/.test(value)) return value;
	const pairs = new Map<string, string>();
	for (const part of value.split(",")) {
		const i = part.indexOf(":");
		if (i > 0) pairs.set(part.slice(0, i).trim(), part.slice(i + 1).trim());
	}
	return pairs.get(appearance) ?? pairs.get("dark") ?? pairs.get("light") ?? value;
}

interface Collected {
	colors: Map<string, string>;
	palette: Map<number, string>;
	fontFamilies: string[];
	fontSize?: number;
	fontThicken?: boolean;
	terminal: NonNullable<ImportResult["terminal"]>;
}

/** Applies entries in order onto `acc` (later wins; `font-family` / `palette` repeat). */
function apply(entries: Entry[], acc: Collected, warn: (message: string) => void): void {
	for (const [key, value] of entries) {
		const colorKey = COLOR_KEYS[key];
		if (colorKey) {
			const c = normalizeColor(value, true);
			if (c) acc.colors.set(colorKey, c);
			else warn(m.settings_theme_diag_invalid_color({ key, value }));
			continue;
		}
		switch (key) {
			case "palette": {
				const pair = /^(\d+)\s*=\s*(.+)$/.exec(value);
				const c = pair ? normalizeColor(pair[2], true) : null;
				if (!pair || !c) warn(m.settings_theme_diag_palette_entry({ value }));
				else if (Number(pair[1]) > 15) warn(m.settings_theme_diag_palette_range());
				else acc.palette.set(Number(pair[1]), c);
				break;
			}
			case "font-family":
				// An empty value resets the list (Ghostty semantics).
				if (value === "") acc.fontFamilies = [];
				else acc.fontFamilies.push(value);
				break;
			case "font-size": {
				const n = Number(value);
				if (Number.isFinite(n) && n > 0) acc.fontSize = n;
				else warn(m.settings_theme_diag_invalid_option({ key, value }));
				break;
			}
			case "font-thicken":
				if (value === "true" || value === "false") acc.fontThicken = value === "true";
				else warn(m.settings_theme_diag_invalid_option({ key, value }));
				break;
			case "cursor-style": {
				const shape = value === "block_hollow" ? "hollow" : value;
				if (shape === "block" || shape === "bar" || shape === "underline" || shape === "hollow") {
					acc.terminal.cursor_shape = shape;
				} else warn(m.settings_theme_diag_invalid_option({ key, value }));
				break;
			}
			case "cursor-style-blink":
				// Ghostty also accepts "" (auto); only explicit booleans are mapped.
				if (value === "true" || value === "false") acc.terminal.blinking = value === "true";
				break;
			case "window-padding-x":
			case "window-padding-y": {
				const n = Number(value.split(",")[0]);
				if (Number.isFinite(n) && n >= 0) {
					acc.terminal[key === "window-padding-x" ? "padding_x" : "padding_y"] = n;
				} else warn(m.settings_theme_diag_invalid_option({ key, value }));
				break;
			}
			case "copy-on-select":
				if (value === "true" || value === "clipboard") acc.terminal.copy_on_select = true;
				else if (value === "false") acc.terminal.copy_on_select = false;
				break;
			case "macos-option-as-alt":
				if (value === "true") acc.terminal.option_as_alt = "both";
				else if (value === "left" || value === "right") acc.terminal.option_as_alt = value;
				else if (value === "false") acc.terminal.option_as_alt = "none";
				break;
			default:
				// Unknown keys are ignored silently.
				break;
		}
	}
}

/** Reads a theme by name (absolute path or file named exactly `name` in `themeDirs`). */
async function loadTheme(
	name: string,
	ctx: GhosttyContext,
	warn: (message: string) => void,
): Promise<Entry[] | null> {
	const candidates =
		name.startsWith("/") || name.startsWith("~")
			? [resolvePath(name, "/", ctx.home)]
			: ctx.themeDirs.map((d) => `${d}/${name}`);
	for (const path of candidates) {
		const body = await ctx.readFile(path);
		if (body !== null) return expand(body, path, ctx, new Set([path]), warn);
	}
	warn(m.settings_theme_diag_theme_not_found({ name }));
	return null;
}

export async function parseGhosttyConfig(text: string, ctx: GhosttyContext): Promise<ImportResult> {
	const warnings: string[] = [];
	const warn = (message: string) => {
		if (!warnings.includes(message)) warnings.push(message);
	};
	const entries = await expand(text, `${ctx.home}/.config/ghostty/config`, ctx, new Set(), warn);

	const acc: Collected = { colors: new Map(), palette: new Map(), fontFamilies: [], terminal: {} };
	let themeName: string | undefined;
	for (const [key, value] of entries) if (key === "theme") themeName = value;

	// Named after the source, never after a product: this is the user's own local config.
	let name = "Ghostty: my config";
	if (themeName) {
		const picked = pickTheme(themeName, ctx.appearance ?? "dark");
		const themeEntries = await loadTheme(picked, ctx, warn);
		if (themeEntries) {
			apply(themeEntries, acc, warn);
			const base = picked.split("/").pop();
			if (base) name = `Ghostty: ${base}`;
		}
	}
	apply(entries, acc, warn);

	const style: Record<string, unknown> = {};
	for (const [k, v] of acc.colors) style[k] = v;
	for (const [i, v] of acc.palette) style[ANSI_KEYS[i]] = v;
	const font: NonNullable<ImportResult["font"]> = {};
	if (acc.fontFamilies[0]) font.family = acc.fontFamilies[0];
	if (acc.fontSize !== undefined) font.size = acc.fontSize;
	if (acc.fontThicken !== undefined) font.thicken = acc.fontThicken;

	const hasTerminal = Object.keys(acc.terminal).length > 0;
	if (Object.keys(style).length === 0 && !themeName && Object.keys(font).length === 0 && !hasTerminal) {
		throw new ThemeImportError(m.settings_theme_diag_ghostty_no_settings());
	}
	const result: ImportResult = {
		name,
		appearance: appearanceOf(acc.colors.get("terminal.background"), ctx.appearance ?? "dark"),
		style,
		warnings,
	};
	if (Object.keys(font).length) result.font = font;
	if (Object.keys(acc.terminal).length) result.terminal = acc.terminal;
	return result;
}
