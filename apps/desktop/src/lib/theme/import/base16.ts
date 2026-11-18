import { parse as parseYaml } from "yaml";
import { m } from "#lib/i18n/m.ts";
import { ThemeImportError } from "./errors.ts";
import { ANSI_KEYS, appearanceOf, normalizeColor, putPlayer, putSyntax, type ImportResult } from "./color.ts";

const BASE_KEYS = Array.from(
	{ length: 16 },
	(_, i) => `base${i.toString(16).toUpperCase().padStart(2, "0")}`,
);

/** Classic base16 syntax mapping: role -> base slot. */
const SYNTAX: Record<string, string> = {
	keyword: "base0E",
	function: "base0D",
	string: "base0B",
	"string.escape": "base0C",
	"string.regex": "base0C",
	"string.special": "base0C",
	number: "base09",
	constant: "base09",
	boolean: "base09",
	type: "base0A",
	tag: "base08",
	attribute: "base0A",
	property: "base08",
	label: "base0A",
	comment: "base03",
	"comment.doc": "base03",
	variable: "base05",
	"variable.parameter": "base08",
	"variable.special": "base09",
	operator: "base05",
	punctuation: "base05",
	title: "base0D",
	link_uri: "base09",
	emphasis: "base0E",
	"emphasis.strong": "base0A",
};

export function parseBase16(yaml: string): ImportResult {
	let doc: unknown;
	try {
		// Legacy files write `base01: #34353e` unquoted, which YAML reads as a comment; quote it first.
		doc = parseYaml(yaml.replace(/^(\s*base[0-9a-f]{2}\s*:\s*)(#[0-9a-fA-F]{3,8})\s*$/gim, '$1"$2"'));
	} catch (e) {
		const detail = e instanceof Error ? e.message : String(e);
		throw new ThemeImportError(m.settings_theme_diag_base16_invalid({ detail }));
	}
	if (typeof doc !== "object" || doc === null || Array.isArray(doc)) {
		throw new ThemeImportError(m.settings_theme_diag_base16_not_mapping());
	}
	const root = doc as Record<string, unknown>;
	const source =
		typeof root.palette === "object" && root.palette !== null
			? (root.palette as Record<string, unknown>)
			: root;
	const warnings: string[] = [];
	const base: Record<string, string> = {};
	for (const key of BASE_KEYS) {
		// Keys are case-insensitive in the wild (base0a / base0A).
		const raw = source[key] ?? source[key.toLowerCase()];
		if (raw === undefined) continue;
		// YAML reads an unquoted all-digit value (e.g. 181818) as a number; restore leading zeros.
		const text = typeof raw === "number" ? String(raw).padStart(6, "0") : raw;
		const c = typeof text === "string" ? normalizeColor(text) : null;
		if (c) base[key] = c;
		else warnings.push(m.settings_theme_diag_invalid_color({ key, value: String(raw) }));
	}
	if (!base.base00 || !base.base05) {
		throw new ThemeImportError(m.settings_theme_diag_base16_needs_base());
	}

	const b = (k: string) => base[k] ?? base.base05;
	const style: Record<string, unknown> = {
		background: b("base00"),
		"editor.background": b("base00"),
		"surface.background": b("base01"),
		"elevated_surface.background": b("base02"),
		text: b("base05"),
		"text.muted": b("base04"),
		"text.disabled": b("base03"),
		"text.accent": b("base0D"),
		"editor.foreground": b("base05"),
		"editor.line_number": b("base03"),
		"editor.active_line_number": b("base04"),
		"editor.active_line.background": b("base01"),
		created: b("base0B"),
		warning: b("base0A"),
		info: b("base0C"),
		error: b("base08"),
		"terminal.background": b("base00"),
		"terminal.foreground": b("base05"),
		"terminal.cursor": b("base05"),
		"terminal.cursor_text": b("base00"),
		"terminal.selection.background": b("base02"),
		"terminal.selection.foreground": b("base05"),
	};
	// ANSI order: black red green yellow blue magenta cyan white (normal, then bright).
	const hues = ["base08", "base0B", "base0A", "base0D", "base0E", "base0C"];
	const normal = [b("base00"), ...hues.map(b), b("base05")];
	const bright = [b("base03"), ...hues.map(b), b("base07")];
	[...normal, ...bright].forEach((c, i) => (style[ANSI_KEYS[i]] = c));
	putPlayer(style, "cursor", b("base05"));
	putPlayer(style, "selection", `${b("base02")}`);
	for (const [role, slot] of Object.entries(SYNTAX)) {
		putSyntax(style, role, { color: b(slot), font_style: null, font_weight: null });
	}

	const variant = typeof root.variant === "string" ? root.variant.toLowerCase() : "";
	const appearance = variant === "light" || variant === "dark" ? variant : appearanceOf(base.base00);
	const rawName = root.name ?? root.scheme;
	const name = typeof rawName === "string" && rawName.trim() ? rawName.trim() : "Base16 import";
	return { name, appearance, style, warnings };
}
