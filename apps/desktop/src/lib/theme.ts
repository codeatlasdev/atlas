/** Zed theme family (https://zed.dev/schema/themes/v0.2.0.json) → CSS vars, terminal palette, editor syntax. */

import { ANSI_VARS } from "#lib/ansi.ts";

export type Appearance = "dark" | "light";

export interface SyntaxStyle {
	color?: string | null;
	font_style?: string | null;
	font_weight?: number | null;
}

export interface ThemeStyle {
	syntax?: Record<string, SyntaxStyle>;
	players?: { cursor?: string; selection?: string }[];
	[key: string]: unknown;
}

export interface Theme {
	name: string;
	appearance: Appearance;
	style: ThemeStyle;
}

export interface ThemeSelection {
	mode: "system" | "light" | "dark";
	light: string;
	dark: string;
}

export const DEFAULT_SELECTION: ThemeSelection = { mode: "system", light: "Atlas Light", dark: "Atlas Dark" };

const HEX = /^#([0-9a-f]{6}|[0-9a-f]{8})$/i;

/** `#rrggbb`/`#rrggbbaa` only; anything else is ignored rather than written into CSS. */
export function color(v: unknown): string | undefined {
	return typeof v === "string" && HEX.test(v) ? v.toLowerCase() : undefined;
}

/** Zed style key → kit custom properties it drives. Unlisted tokens keep kit defaults. */
export const CSS_MAP: Record<string, string[]> = {
	background: ["--ca-surface-sunken"],
	"editor.background": ["--ca-surface-base"],
	"surface.background": ["--ca-surface-raised"],
	"elevated_surface.background": ["--ca-surface-overlay"],
	text: ["--ca-text-primary"],
	"text.muted": ["--ca-text-secondary", "--ca-text-hint"],
	"text.disabled": ["--ca-text-disabled"],
	"text.accent": [
		"--ca-brand-main",
		"--ca-brand-light",
		"--ca-brand-dark",
		"--ca-brand-fill",
		"--ca-brand-fill-hover",
		"--ca-brand-text",
	],
	"border.variant": ["--ca-outline-subtle"],
	border: ["--ca-outline-default"],
	"border.focused": ["--ca-outline-strong"],
	"element.hover": ["--ca-action-hover"],
	"element.selected": ["--ca-action-selected"],
	created: ["--ca-semantic-success"],
	warning: ["--ca-semantic-warning"],
	info: ["--ca-semantic-info"],
	error: ["--ca-semantic-error"],
};

/** Editor chrome keys → `--cm-*` vars read by cm.ts (which falls back to the kit tokens). */
const EDITOR_MAP: Record<string, string> = {
	"editor.foreground": "--cm-fg",
	"editor.line_number": "--cm-gutter",
	"editor.active_line_number": "--cm-gutter-active",
	"editor.active_line.background": "--cm-active-line",
};

/** Syntax keys cm.ts styles; `--cm-syn-<key>`, plus `-italic`/`-weight` companions. */
export const SYNTAX_KEYS = [
	"property",
	"string",
	"string.special",
	"number",
	"boolean",
	"constant",
	"comment",
	"punctuation",
	"keyword",
	"type",
	"label",
	"function",
	"title",
	"variable",
	"operator",
	"attribute",
	"tag",
];

const synVar = (key: string) => `--cm-syn-${key.replace(".", "-")}`;

export function cssVars(style: ThemeStyle): Record<string, string> {
	const out: Record<string, string> = {};
	// Service logs and the terminal share one palette: slots the theme leaves out keep app.css's defaults.
	ansi16(style).forEach((c, i) => {
		if (c) out[ANSI_VARS[i]] = c;
	});
	for (const [key, vars] of Object.entries(CSS_MAP)) {
		const c = color(style[key]);
		if (c) for (const v of vars) out[v] = c;
	}
	for (const [key, v] of Object.entries(EDITOR_MAP)) {
		const c = color(style[key]);
		if (c) out[v] = c;
	}
	const player = style.players?.[0];
	const caret = color(player?.cursor);
	if (caret) out["--cm-caret"] = caret;
	const selection = color(player?.selection);
	if (selection) out["--cm-selection"] = selection;
	for (const [key, tok] of Object.entries(syntaxTokens(style))) {
		if (!SYNTAX_KEYS.includes(key)) continue;
		if (tok.color) out[synVar(key)] = tok.color;
		if (tok.italic) out[`${synVar(key)}-italic`] = "italic";
		if (tok.weight) out[`${synVar(key)}-weight`] = tok.weight;
	}
	return out;
}

const ANSI_NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

/** ANSI order 0..15: normal black…white, then bright black…white. `null` = use the terminal default. */
export function ansi16(style: ThemeStyle): (string | null)[] {
	const names = [...ANSI_NAMES, ...ANSI_NAMES.map((n) => `bright_${n}`)];
	return names.map((n) => color(style[`terminal.ansi.${n}`])?.slice(0, 7) ?? null);
}

export interface TerminalColors {
	/** `null` = fall back to the surface/text CSS variables. */
	fg: string | null;
	bg: string | null;
	ansi16: (string | null)[];
	cursor: string | null;
}

export function terminalColors(style: ThemeStyle): TerminalColors {
	return {
		fg: color(style["terminal.foreground"])?.slice(0, 7) ?? null,
		bg: color(style["terminal.background"])?.slice(0, 7) ?? null,
		ansi16: ansi16(style),
		cursor: color(style.players?.[0]?.cursor)?.slice(0, 7) ?? null,
	};
}

/** xterm.js theme; fields stay undefined when the theme says nothing so the caller's defaults apply. */
export function xtermTheme(style: ThemeStyle): Record<string, string | undefined> {
	const t = terminalColors(style);
	const names = [...ANSI_NAMES, ...ANSI_NAMES.map((n) => `bright${n[0].toUpperCase()}${n.slice(1)}`)];
	const out: Record<string, string | undefined> = {
		background: t.bg ?? undefined,
		foreground: t.fg ?? undefined,
		cursor: t.cursor ?? t.fg ?? undefined,
	};
	names.forEach((n, i) => (out[n] = t.ansi16[i] ?? undefined));
	return out;
}

/** Syntax token color/style by Zed syntax key (`keyword`, `string`, `function`, …). */
export interface SyntaxToken {
	color?: string;
	italic?: boolean;
	weight?: string;
}

export function syntaxTokens(style: ThemeStyle): Record<string, SyntaxToken> {
	const out: Record<string, SyntaxToken> = {};
	for (const [key, s] of Object.entries(style.syntax ?? {})) {
		if (!s || typeof s !== "object") continue;
		const tok: SyntaxToken = {};
		const c = color(s.color);
		if (c) tok.color = c;
		if (s.font_style === "italic") tok.italic = true;
		if (typeof s.font_weight === "number") tok.weight = String(s.font_weight);
		if (Object.keys(tok).length) out[key] = tok;
	}
	return out;
}

/** Throws on a structurally invalid family; skips invalid members so one bad theme never hides the rest. */
export function parseThemeFamily(json: unknown): Theme[] {
	if (!json || typeof json !== "object") throw new Error("theme family must be an object");
	if (!("themes" in json) || !Array.isArray(json.themes))
		throw new Error("theme family needs a `themes` array");
	const out: Theme[] = [];
	for (const t of json.themes) {
		if (!t || typeof t !== "object") continue;
		if (!("name" in t) || typeof t.name !== "string" || !t.name) continue;
		if (!("appearance" in t) || (t.appearance !== "dark" && t.appearance !== "light")) continue;
		if (!("style" in t) || !t.style || typeof t.style !== "object") continue;
		// Colors and syntax entries are validated per key when read (`color`, `syntaxTokens`).
		const style: ThemeStyle = t.style;
		out.push({ name: t.name, appearance: t.appearance, style });
	}
	return out;
}

/** Settings value → selection; unknown fields fall back to the defaults. */
export function parseSelection(v: unknown): ThemeSelection {
	const sel = { ...DEFAULT_SELECTION };
	if (!v || typeof v !== "object") return sel;
	if ("mode" in v && (v.mode === "light" || v.mode === "dark" || v.mode === "system")) sel.mode = v.mode;
	if ("light" in v && typeof v.light === "string" && v.light) sel.light = v.light;
	if ("dark" in v && typeof v.dark === "string" && v.dark) sel.dark = v.dark;
	return sel;
}

/** Pick the active theme; a missing name falls back to the default of that appearance, then any of it. */
export function resolveTheme(themes: Theme[], sel: ThemeSelection, systemDark: boolean): Theme | undefined {
	const appearance: Appearance = sel.mode === "system" ? (systemDark ? "dark" : "light") : sel.mode;
	const named = themes.find((t) => t.name === sel[appearance] && t.appearance === appearance);
	return (
		named ??
		themes.find((t) => t.name === DEFAULT_SELECTION[appearance]) ??
		themes.find((t) => t.appearance === appearance)
	);
}
