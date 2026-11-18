/**
 * Command-line arguments as one editable string: `parseArgs` splits it the way a shell
 * would (single quotes, double quotes, backslash escapes), `formatArgs` is its inverse.
 * Pure; no variable or glob expansion.
 */
import { m } from "#lib/i18n/m.ts";

export type ParsedArgs = { ok: true; args: string[] } | { ok: false; error: string };

/** Characters that need quoting when an argument is shown in the text box. */
const NEEDS_QUOTES = /[\s"'\\]/;

export function parseArgs(text: string): ParsedArgs {
	const args: string[] = [];
	let cur = "";
	/** True once the current word has started, so `""` yields an empty argument. */
	let inWord = false;
	let quote: '"' | "'" | null = null;

	for (let i = 0; i < text.length; i++) {
		const c = text[i]!;
		if (quote === "'") {
			if (c === "'") quote = null;
			else cur += c;
		} else if (quote === '"') {
			if (c === '"') quote = null;
			else if (c === "\\" && (text[i + 1] === '"' || text[i + 1] === "\\")) cur += text[++i];
			else cur += c;
		} else if (c === "'" || c === '"') {
			quote = c;
			inWord = true;
		} else if (c === "\\") {
			if (i + 1 >= text.length) return { ok: false, error: m.settings_general_args_backslash() };
			cur += text[++i];
			inWord = true;
		} else if (/\s/.test(c)) {
			if (inWord) args.push(cur);
			cur = "";
			inWord = false;
		} else {
			cur += c;
			inWord = true;
		}
	}
	if (quote)
		return {
			ok: false,
			error: quote === '"' ? m.settings_general_args_quote_double() : m.settings_general_args_quote_single(),
		};
	if (inWord) args.push(cur);
	return { ok: true, args };
}

/** One argument, quoted only when it has to be. */
function quoteArg(a: string): string {
	if (a === "") return '""';
	if (!NEEDS_QUOTES.test(a)) return a;
	return `"${a.replace(/["\\]/g, "\\$&")}"`;
}

export const formatArgs = (args: readonly string[]): string => args.map(quoteArg).join(" ");
