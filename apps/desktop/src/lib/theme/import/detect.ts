import { m } from "#lib/i18n/m.ts";
import { parseBase16 } from "./base16.ts";
import type { ImportResult } from "./color.ts";
import { ThemeImportError } from "./errors.ts";
import { parseGhosttyConfig, type GhosttyContext } from "./ghostty.ts";
import { parseItermColors } from "./iterm.ts";
import { parseJsonc, parseVscodeTheme } from "./vscode.ts";

export { parseAlacritty } from "./alacritty.ts";
export type ThemeFormat = "ghostty" | "iterm" | "vscode" | "base16" | "zed" | "atlas";

export type ImportOutcome =
	| { kind: "ghostty" | "iterm" | "vscode" | "base16"; result: ImportResult }
	/** Zed/Atlas family JSON is not converted: the caller copies the file as is. */
	| { kind: "zed" | "atlas" };

function parseJsonObject(text: string): Record<string, unknown> | null {
	try {
		const v = parseJsonc(text);
		return typeof v === "object" && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
	} catch {
		return null;
	}
}

export function detectFormat(fileName: string, text: string): ThemeFormat | null {
	const lower = fileName.toLowerCase();
	const head = text.trimStart();
	const looksXml = head.startsWith("<?xml") || head.startsWith("<plist");
	if (lower.endsWith(".itermcolors") || (looksXml && /Ansi 0 Color|Background Color/.test(text))) {
		return "iterm";
	}
	if (lower.endsWith(".yaml") || lower.endsWith(".yml")) return "base16";
	if (lower.endsWith(".json") || lower.endsWith(".jsonc") || head.startsWith("{")) {
		const obj = parseJsonObject(text);
		if (obj) {
			const themes = obj.themes;
			if (Array.isArray(themes)) {
				const atlas = themes.some((t) => typeof t === "object" && t !== null && "atlas" in t);
				return atlas ? "atlas" : "zed";
			}
			if ("colors" in obj || "tokenColors" in obj || "semanticTokenColors" in obj) return "vscode";
		}
		if (lower.endsWith(".json") || lower.endsWith(".jsonc")) return null;
	}
	// Base16 without a .yaml extension: base00..base0F keys.
	if (/^\s*(base00|"base00")\s*:/im.test(text)) return "base16";
	if (/^\s*(palette|background|foreground|theme|font-family|cursor-color)\s*=/m.test(text)) return "ghostty";
	return null;
}

export async function importAny(
	fileName: string,
	text: string,
	ctx?: GhosttyContext,
): Promise<ImportOutcome> {
	const kind = detectFormat(fileName, text);
	const base = fileName.split("/").pop() ?? fileName;
	const stem = base.replace(/\.[^.]+$/, "");
	switch (kind) {
		case "ghostty": {
			if (!ctx) throw new ThemeImportError("Ghostty import needs a context");
			return { kind, result: await parseGhosttyConfig(text, ctx) };
		}
		case "iterm":
			return { kind, result: parseItermColors(text, stem) };
		case "vscode":
			return { kind, result: parseVscodeTheme(text, stem) };
		case "base16":
			return { kind, result: parseBase16(text) };
		case "zed":
		case "atlas":
			return { kind };
		default:
			throw new ThemeImportError(m.settings_theme_diag_unrecognized_format({ name: base }));
	}
}
