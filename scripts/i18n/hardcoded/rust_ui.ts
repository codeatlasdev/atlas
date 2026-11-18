// Hardcoded user-facing text in Rust that `println!` and friends do not carry (the second Rust ratchet, baseline
// section `rust-ui`): the labels of native menus, tray and window text, the title and body of a native notification,
// the text of an `anyhow` context, and the text a ratatui widget draws. A text scanner on purpose, like rust.ts: it finds
// a call, takes its Nth argument and flags it when that argument starts with a string literal that has letters.
// Not counted: test code, a line carrying (or following one that carries) `// i18n-ignore`, an argument that is a
// variable or a `t!` call.
import type { Found, Violation } from "../baseline.ts";
import { lineOf, readText, rel, walk } from "../util.ts";
import { LETTERS, RUST_DIRS, isTestFile, literalAt, visibleText, withoutTestTail } from "./rust.ts";

/** A call whose `arg`-th argument (0-based) is text a person reads. */
interface Site {
	/** Source text before the `(`, as a regular expression. */
	head: RegExp;
	arg: number;
}

const site = (head: string, arg: number): Site => ({ head: new RegExp(`${head}\\s*$`), arg });

export const SITES: Site[] = [
	// Tauri menus (menu bar, tray, context menus): `MenuItem::with_id(app, id, "text", ..)`, `Submenu::with_items(app, "text", ..)`.
	site("\\b(?:MenuItem|CheckMenuItem|IconMenuItem)::with_id", 2),
	site("\\b(?:MenuItem|CheckMenuItem|IconMenuItem)::new", 1),
	site("\\bSubmenu::(?:new|with_items)", 1),
	site("\\bSubmenu::(?:with_id|with_id_and_items)", 2),
	site("\\bPredefinedMenuItem::\\w+", 1),
	site("\\bMenuItemBuilder::new", 0),
	site("\\bSubmenuBuilder::new", 1),
	site("\\bSubmenuBuilder::with_id", 2),
	// Native text set after the fact: a tray tooltip or title, a window title, a notification's title and body, a
	// menu item's text, a ratatui block title.
	site("\\.(?:set_text|set_title|set_tooltip|tooltip|title|body)", 0),
	// anyhow: the sentence a person reads under `error:`.
	site("\\.(?:with_)?context", 0),
	// ratatui: the text of a widget.
	site("\\b(?:Span::raw|Span::styled|Line::from|Text::from|Paragraph::new)", 0),
];

/** The index of the first character of argument `n` of the call whose `(` is at `open`, or undefined when the call
 * has fewer arguments. Strings and char literals are skipped whole; brackets nest. */
export function argStart(src: string, open: number, n: number): number | undefined {
	if (n === 0) return open + 1;
	let depth = 0;
	let arg = 0;
	let i = open + 1;
	while (i < src.length) {
		const c = src[i];
		if (c === '"' || (c === "r" && /^r#*"/.test(src.slice(i, i + 12)))) {
			const lit = literalAt(src, i);
			if (!lit) return undefined;
			i = lit[1];
			continue;
		}
		// A char literal ('a', '\n', '"') is skipped whole; a lifetime ('a) has no closing quote and is just a character.
		if (c === "'") {
			const ch = /^'(?:\\.[^']*|[^'\\])'/.exec(src.slice(i, i + 12));
			if (ch) {
				i += ch[0].length;
				continue;
			}
		}
		if ("([{".includes(c)) depth++;
		else if (")]}".includes(c)) {
			if (depth === 0) return undefined;
			depth--;
		} else if (c === "," && depth === 0 && ++arg === n) return i + 1;
		i++;
	}
	return undefined;
}

/** The literal an argument starts with, through the wrappers a call site puts around text (`Some(..)`, `&`,
 * `format!(..)`, `|| ..`); undefined when it is an expression of another kind. */
function leadingLiteral(src: string, at: number): string | undefined {
	const m = /^\s*(?:\|\|\s*)?&?\s*(?:Some\(\s*)?(?:&?\s*format!\(\s*)?/.exec(src.slice(at, at + 80));
	const lit = literalAt(src, at + (m?.[0].length ?? 0));
	return lit?.[0];
}

export function scanRustUiSource(source: string): Violation[] {
	const src = withoutTestTail(source);
	const lines = src.split("\n");
	const out: Violation[] = [];
	const seen = new Set<number>();
	for (const m of src.matchAll(/\(/g)) {
		const open = m.index ?? 0;
		const before = src.slice(Math.max(0, open - 60), open);
		const hit = SITES.find((s) => s.head.test(before));
		if (!hit) continue;
		const start = argStart(src, open, hit.arg);
		if (start === undefined) continue;
		const lit = leadingLiteral(src, start);
		if (lit === undefined) continue;
		if (!LETTERS.test(visibleText(lit))) continue;
		const line = lineOf(src, start);
		const lineStart = src.lastIndexOf("\n", open - 1) + 1;
		if (src.slice(lineStart, open).includes("//")) continue; // commented out
		if (/i18n-ignore/.test(lines[line - 1] ?? "") || /i18n-ignore/.test(lines[line - 2] ?? "") || /i18n-ignore/.test(lines[lineOf(src, open) - 2] ?? "")) continue;
		if (seen.has(start)) continue;
		seen.add(start);
		out.push({ line, text: lit.trim().replace(/\s+/g, " ").slice(0, 60) });
	}
	return out.sort((a, b) => a.line - b.line);
}

export function scanRustUi(root: string, dirs: string[] = RUST_DIRS): Found {
	const found: Found = new Map();
	for (const dir of dirs) {
		for (const path of walk(`${root}/${dir}`, (p) => p.endsWith(".rs"))) {
			const file = rel(root, path);
			if (isTestFile(file.slice(dir.length))) continue;
			const v = scanRustUiSource(readText(path));
			if (v.length) found.set(file, v);
		}
	}
	return found;
}
