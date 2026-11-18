import { m } from "#lib/i18n/m.ts";
import { ThemeImportError } from "./errors.ts";
import { ANSI_KEYS, appearanceOf, rgbFloatToHex, type ImportResult } from "./color.ts";

type Plist = string | number | Plist[] | { [key: string]: Plist };

/** Minimal tolerant XML plist reader: dict/array/key/string/real/integer/true/false; comments and DOCTYPE skipped. */
function readPlist(xml: string): Plist | null {
	const tokens = xml
		.replace(/<!--[\s\S]*?-->/g, "")
		.replace(/<\?[\s\S]*?\?>/g, "")
		.replace(/<!DOCTYPE[^>]*>/gi, "")
		.match(/<[^>]+>|[^<]+/g);
	if (!tokens) return null;
	let i = 0;

	const text = (): string => {
		let s = "";
		while (i < tokens.length && !tokens[i].startsWith("<")) s += tokens[i++];
		return s
			.replace(/&lt;/g, "<")
			.replace(/&gt;/g, ">")
			.replace(/&quot;/g, '"')
			.replace(/&apos;/g, "'")
			.replace(/&amp;/g, "&")
			.trim();
	};

	const value = (): Plist | null => {
		while (i < tokens.length && !tokens[i].startsWith("<") && !tokens[i].trim()) i++;
		const tag = tokens[i++];
		if (!tag) return null;
		const parts = /^<\s*(\/?)\s*([A-Za-z]+)[^>]*?(\/?)>$/.exec(tag);
		if (!parts || parts[1]) return null;
		const [, , name, selfClose] = parts;
		switch (name) {
			case "dict": {
				const dict: { [key: string]: Plist } = {};
				if (selfClose) return dict;
				for (;;) {
					while (i < tokens.length && !tokens[i].startsWith("<")) i++;
					if (i >= tokens.length || /^<\s*\/\s*dict/.test(tokens[i])) {
						i++;
						return dict;
					}
					if (!/^<\s*key\s*>/.test(tokens[i])) {
						i++;
						continue;
					}
					i++;
					const key = text();
					i++; // </key>
					const v = value();
					if (v !== null) dict[key] = v;
				}
			}
			case "array": {
				const arr: Plist[] = [];
				if (selfClose) return arr;
				for (;;) {
					while (i < tokens.length && !tokens[i].startsWith("<")) i++;
					if (i >= tokens.length || /^<\s*\/\s*array/.test(tokens[i])) {
						i++;
						return arr;
					}
					const v = value();
					if (v !== null) arr.push(v);
				}
			}
			case "real":
			case "integer": {
				if (selfClose) return 0;
				const n = Number(text());
				i++;
				return Number.isFinite(n) ? n : 0;
			}
			case "true":
			case "false":
				return name === "true" ? 1 : 0;
			default: {
				// string, date, data, ...
				if (selfClose) return "";
				const s = text();
				i++;
				return s;
			}
		}
	};

	// Descend to the first <dict> (skips <plist>).
	while (i < tokens.length && !/^<\s*dict/.test(tokens[i])) i++;
	return value();
}

const isDict = (v: Plist | undefined): v is { [key: string]: Plist } =>
	typeof v === "object" && v !== null && !Array.isArray(v);

function readColor(v: Plist | undefined): string | null {
	if (!isDict(v)) return null;
	const c = (k: string) => (typeof v[`${k} Component`] === "number" ? (v[`${k} Component`] as number) : null);
	const r = c("Red");
	const g = c("Green");
	const b = c("Blue");
	return r === null || g === null || b === null ? null : rgbFloatToHex(r, g, b);
}

const NAMED_KEYS: [plistKey: string, styleKey: string][] = [
	["Background Color", "terminal.background"],
	["Foreground Color", "terminal.foreground"],
	["Cursor Color", "terminal.cursor"],
	["Cursor Text Color", "terminal.cursor_text"],
	["Selection Color", "terminal.selection.background"],
	["Selected Text Color", "terminal.selection.foreground"],
];

export function parseItermColors(plistXml: string, name: string): ImportResult {
	const root = readPlist(plistXml) ?? undefined;
	if (!isDict(root)) throw new ThemeImportError(m.settings_theme_diag_iterm_invalid());
	const warnings: string[] = [];
	const style: Record<string, unknown> = {};

	for (const [plistKey, styleKey] of NAMED_KEYS) {
		if (!(plistKey in root)) continue;
		const c = readColor(root[plistKey]);
		if (c) style[styleKey] = c;
		else warnings.push(m.settings_theme_diag_invalid_color_key({ key: plistKey }));
	}
	for (let n = 0; n < 16; n++) {
		const entry = root[`Ansi ${n} Color`];
		if (entry === undefined) continue;
		const c = readColor(entry);
		if (c) style[ANSI_KEYS[n]] = c;
		else warnings.push(m.settings_theme_diag_invalid_color_key({ key: `Ansi ${n} Color` }));
	}
	if (Object.keys(style).length === 0) throw new ThemeImportError(m.settings_theme_diag_iterm_no_colors());

	const bg = style["terminal.background"] as string | undefined;
	return { name, appearance: appearanceOf(bg), style, warnings };
}
