import { m } from "#lib/i18n/m.ts";
import { ThemeImportError } from "./errors.ts";
import {
	ANSI_KEYS,
	ANSI_NAMES,
	appearanceOf,
	normalizeColor,
	putPlayer,
	putSyntax,
	type ImportResult,
} from "./color.ts";

/** Strips `//` and block comments (outside strings) and trailing commas, then JSON.parses. */
export function parseJsonc(text: string): unknown {
	let out = "";
	let i = 0;
	const src = text.replace(/^\uFEFF/, "");
	while (i < src.length) {
		const ch = src[i];
		if (ch === '"') {
			let j = i + 1;
			while (j < src.length && src[j] !== '"') j += src[j] === "\\" ? 2 : 1;
			out += src.slice(i, j + 1);
			i = j + 1;
		} else if (ch === "/" && src[i + 1] === "/") {
			while (i < src.length && src[i] !== "\n") i++;
		} else if (ch === "/" && src[i + 1] === "*") {
			const end = src.indexOf("*/", i + 2);
			i = end < 0 ? src.length : end + 2;
		} else {
			out += ch;
			i++;
		}
	}
	return JSON.parse(out.replace(/,(\s*[}\]])/g, "$1"));
}

const isObj = (v: unknown): v is Record<string, unknown> =>
	typeof v === "object" && v !== null && !Array.isArray(v);

/** VS Code `colors` key -> Atlas style key (or `players[0]` cursor/selection via the `player:` prefix). */
const COLOR_MAP: Record<string, string> = {
	"editor.background": "editor.background",
	"editor.foreground": "text",
	"editorCursor.foreground": "player:cursor",
	"editor.selectionBackground": "player:selection",
	"editor.lineHighlightBackground": "editor.active_line.background",
	"editorLineNumber.foreground": "editor.line_number",
	"editorLineNumber.activeForeground": "editor.active_line_number",
	"sideBar.background": "surface.background",
	"titleBar.activeBackground": "background",
	"activityBar.background": "background",
	"terminal.background": "terminal.background",
	"terminal.foreground": "terminal.foreground",
	"terminalCursor.foreground": "terminal.cursor",
	"terminalCursor.background": "terminal.cursor_text",
	"terminal.selectionBackground": "terminal.selection.background",
	"terminal.selectionForeground": "terminal.selection.foreground",
	focusBorder: "border.focused",
	errorForeground: "error",
	"editorError.foreground": "error",
	"editorWarning.foreground": "warning",
	"editorInfo.foreground": "info",
	"gitDecoration.addedResourceForeground": "version_control.added",
	"gitDecoration.modifiedResourceForeground": "version_control.modified",
	"gitDecoration.deletedResourceForeground": "version_control.deleted",
	"gitDecoration.renamedResourceForeground": "version_control.renamed",
	"gitDecoration.ignoredResourceForeground": "version_control.ignored",
};

/** TextMate scope prefix -> syntax role. The longest matching prefix wins. */
const SCOPE_MAP: [scope: string, role: string][] = [
	["comment", "comment"],
	["comment.block.documentation", "comment.doc"],
	["string", "string"],
	["string.regexp", "string.regex"],
	["constant.character.escape", "string.escape"],
	["constant.numeric", "number"],
	["constant.language", "boolean"],
	["constant", "constant"],
	["keyword", "keyword"],
	["keyword.operator", "operator"],
	["storage", "keyword"],
	["entity.name.function", "function"],
	["support.function", "function"],
	["entity.name.type", "type"],
	["entity.name.class", "type"],
	["support.class", "type"],
	["support.type", "type"],
	["variable", "variable"],
	["variable.parameter", "variable.parameter"],
	["variable.language", "variable.special"],
	["variable.other.property", "property"],
	["entity.name.tag", "tag"],
	["entity.other.attribute-name", "attribute"],
	["markup.heading", "title"],
	["markup.underline.link", "link_uri"],
	["markup.italic", "emphasis"],
	["markup.bold", "emphasis.strong"],
	["punctuation", "punctuation"],
];

interface Rule {
	scope?: unknown;
	settings?: { foreground?: unknown; fontStyle?: unknown };
}

export function parseVscodeTheme(text: string, fallbackName = "VS Code import"): ImportResult {
	let raw: unknown;
	try {
		raw = parseJsonc(text);
	} catch (e) {
		const detail = e instanceof Error ? e.message : String(e);
		throw new ThemeImportError(m.settings_theme_diag_vscode_invalid({ detail }));
	}
	if (!isObj(raw)) throw new ThemeImportError(m.settings_theme_diag_vscode_not_object());
	const warnings: string[] = [];
	const warn = (message: string) => {
		if (!warnings.includes(message)) warnings.push(message);
	};
	const style: Record<string, unknown> = {};

	const colors = isObj(raw.colors) ? raw.colors : {};
	// COLOR_MAP order is precedence: later entries override earlier ones writing the same style key.
	for (const [src, dst] of Object.entries(COLOR_MAP)) {
		const v = colors[src];
		if (typeof v !== "string") continue;
		const c = normalizeColor(v);
		if (!c) {
			warn(m.settings_theme_diag_invalid_color({ key: src, value: v }));
			continue;
		}
		if (dst.startsWith("player:")) putPlayer(style, dst.slice(7) as "cursor" | "selection", c);
		else style[dst] = c;
	}
	ANSI_NAMES.forEach((n, i) => {
		for (const [offset, prefix] of [
			[0, "ansi"],
			[8, "ansiBright"],
		] as const) {
			const key = `terminal.${prefix}${n[0].toUpperCase()}${n.slice(1)}`;
			const v = colors[key];
			if (typeof v !== "string") continue;
			const c = normalizeColor(v);
			if (c) style[ANSI_KEYS[i + offset]] = c;
			else warn(m.settings_theme_diag_invalid_color({ key, value: v }));
		}
	});

	const tokenColors = Array.isArray(raw.tokenColors) ? (raw.tokenColors as unknown[]) : [];
	// role -> winning rule; each scope resolves to its single longest SCOPE_MAP prefix (most specific),
	// and between rules targeting the same role the longer scope wins, later rules breaking ties.
	const best: Record<string, { len: number; rule: Rule }> = {};
	for (const item of tokenColors) {
		if (!isObj(item)) continue;
		if (typeof item.include === "string") {
			warn(m.settings_theme_diag_include_ignored({ include: item.include }));
			continue;
		}
		const rule = item as Rule;
		const scopes = Array.isArray(rule.scope)
			? rule.scope
			: typeof rule.scope === "string"
				? rule.scope.split(",")
				: [];
		for (const s of scopes) {
			if (typeof s !== "string") continue;
			const scope = s.trim();
			let match: { len: number; role: string } | undefined;
			for (const [prefix, role] of SCOPE_MAP) {
				if (scope !== prefix && !scope.startsWith(`${prefix}.`)) continue;
				if (!match || prefix.length >= match.len) match = { len: prefix.length, role };
			}
			if (match && (!best[match.role] || match.len >= best[match.role].len)) {
				best[match.role] = { len: match.len, rule };
			}
		}
	}
	for (const [role, { rule }] of Object.entries(best)) {
		const rawFg = rule.settings?.foreground;
		const fg = typeof rawFg === "string" ? normalizeColor(rawFg) : null;
		const fontStyle = typeof rule.settings?.fontStyle === "string" ? rule.settings.fontStyle : "";
		const italic = /\bitalic\b/.test(fontStyle);
		const bold = /\bbold\b/.test(fontStyle);
		if (!fg && !italic && !bold) continue;
		putSyntax(style, role, {
			...(fg ? { color: fg } : {}),
			font_style: italic ? "italic" : null,
			font_weight: bold ? 700 : null,
		});
	}

	if (Object.keys(style).length === 0) throw new ThemeImportError(m.settings_theme_diag_vscode_no_colors());

	const type = typeof raw.type === "string" ? raw.type : "";
	const bg = (style["editor.background"] ?? style["terminal.background"]) as string | undefined;
	const appearance =
		type === "light" ? "light" : type === "dark" || type === "hc" ? "dark" : appearanceOf(bg);
	const name = typeof raw.name === "string" && raw.name.trim() ? raw.name.trim() : fallbackName;
	return { name, appearance, style, warnings };
}
