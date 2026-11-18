import { parse as parseToml } from "smol-toml";
import { parse as parseYaml } from "yaml";
import { m } from "#lib/i18n/m.ts";
import { ANSI_NAMES, appearanceOf, normalizeColor, type ImportResult } from "./color.ts";
import { ThemeImportError } from "./errors.ts";

type Obj = Record<string, unknown>;

const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

/** `#rrggbb`, `0xrrggbb` (unquoted YAML ints are tolerated too) -> `#rrggbb`, else null. */
function hex(v: unknown): string | null {
	if (typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 0xffffff) {
		return `#${v.toString(16).padStart(6, "0")}`;
	}
	if (typeof v !== "string") return null;
	return normalizeColor(v.trim().replace(/^0x/i, "#"));
}

function readDoc(text: string, fileName: string): Obj {
	const isYaml = /\.ya?ml$/i.test(fileName);
	let doc: unknown;
	try {
		doc = isYaml ? parseYaml(text) : parseToml(text);
	} catch (e) {
		const kind = isYaml ? "YAML" : "TOML";
		const detail = e instanceof Error ? e.message : String(e);
		throw new ThemeImportError(m.settings_theme_diag_alacritty_invalid({ kind, detail }));
	}
	if (!isObj(doc)) throw new ThemeImportError(m.settings_theme_diag_alacritty_not_mapping());
	return doc;
}

/** Alacritty color scheme (TOML `[colors.*]` or legacy YAML `colors:`) -> ImportResult. */
export function parseAlacritty(text: string, fileName: string): ImportResult {
	const doc = readDoc(text, fileName);
	const colors = doc.colors;
	if (!isObj(colors)) throw new ThemeImportError(m.settings_theme_diag_alacritty_no_colors_section());

	const warnings: string[] = [];
	const style: Record<string, unknown> = {};
	const put = (section: string, key: string, styleKey: string, v: unknown) => {
		if (v === undefined) return;
		const c = hex(v);
		if (c) style[styleKey] = c;
		else
			warnings.push(
				m.settings_theme_diag_invalid_color({ key: `colors.${section}.${key}`, value: String(v) }),
			);
	};

	const section = (name: string): Obj => (isObj(colors[name]) ? colors[name] : {});
	const primary = section("primary");
	put("primary", "background", "terminal.background", primary.background);
	put("primary", "foreground", "terminal.foreground", primary.foreground);

	const sel = section("selection");
	put("selection", "background", "terminal.selection.background", sel.background);
	put("selection", "text", "terminal.selection.foreground", sel.text);

	const cursor = section("cursor");
	put("cursor", "cursor", "terminal.cursor", cursor.cursor);
	put("cursor", "text", "terminal.cursor_text", cursor.text);

	for (const [name, prefix] of [
		["normal", "terminal.ansi."],
		["bright", "terminal.ansi.bright_"],
	] as const) {
		const s = section(name);
		for (const n of ANSI_NAMES) put(name, n, `${prefix}${n}`, s[n]);
	}

	if (Object.keys(style).length === 0)
		throw new ThemeImportError(m.settings_theme_diag_alacritty_no_colors());

	const base = fileName.split("/").pop() ?? fileName;
	const name = base.replace(/\.[^.]+$/, "") || "Alacritty import";
	const bg = style["terminal.background"] as string | undefined;
	return { name, appearance: appearanceOf(bg), style, warnings };
}
