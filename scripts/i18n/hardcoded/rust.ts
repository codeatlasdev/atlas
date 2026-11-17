// Hardcoded user-facing text in Rust: the format string of println!/eprintln!/print!/eprint!/bail!/anyhow!
// when it has letters outside `{..}` placeholders. A text scanner on purpose (see i18n/README.md, "Why not clippy").
// Not counted: test code (`mod tests` tail, tests.rs, tests/), machine output (`println!("{}", json)`, `step:` lines),
// and a line carrying (or following one that carries) `// i18n-ignore`.
import type { Found, Violation } from "../baseline.ts";
import { lineOf, readText, rel, walk } from "../util.ts";

/** User-facing crates; the baseline and the ratchet cover these `src` dirs. */
export const RUST_DIRS = ["crates/atlas-cli/src", "crates/atlas-tui/src", "crates/atlas-desktop/src", "crates/atlas-host/src"];

const MACRO = /\b(?:println|eprintln|print|eprint|bail|anyhow)!\s*\(\s*/g;
const LETTERS = /\p{L}/u;

/** The string literal at `i` (a `"` or a raw `r#"`), as [content, index after it]; undefined if `i` is not one. */
function literalAt(src: string, i: number): [string, number] | undefined {
	const raw = /^r(#*)"/.exec(src.slice(i, i + 12));
	if (raw) {
		const close = `"${raw[1]}`;
		const start = i + raw[0].length;
		const end = src.indexOf(close, start);
		return end < 0 ? undefined : [src.slice(start, end), end + close.length];
	}
	if (src[i] !== '"') return undefined;
	let j = i + 1;
	let out = "";
	while (j < src.length && src[j] !== '"') {
		if (src[j] === "\\") {
			// `\` + newline eats the next line's indentation; any other escape is one char of text.
			if (src[j + 1] === "\n") {
				j += 2;
				while (/[ \t]/.test(src[j] ?? "")) j++;
				continue;
			}
			// `\x1b` and `\u{2713}` are the code point; `\n`, `\t`, `\r` whitespace; `\"` a quote.
			const esc = /^(?:x([0-9a-fA-F]{2})|u\{([0-9a-fA-F_]+)\})/.exec(src.slice(j + 1, j + 12));
			if (esc) out += String.fromCodePoint(Number.parseInt((esc[1] ?? esc[2]).replaceAll("_", ""), 16));
			else out += "ntr".includes(src[j + 1]) ? " " : src[j + 1];
			j += 1 + (esc ? esc[0].length : 1);
			continue;
		}
		out += src[j++];
	}
	return [out, j + 1];
}

/** Text of a format string that a user would read: ANSI colors, placeholders and `{{` `}}` escapes removed. */
const visibleText = (fmt: string) => fmt.replace(/\u001b\[[0-9;]*[A-Za-z]/g, "").replace(/\{\{|\}\}/g, "").replace(/\{[^{}]*\}/g, "");

/** Source with everything from the first inline `#[cfg(test)] mod x {` on is dropped (the repo's test layout). */
export function withoutTestTail(src: string): string {
	const m = /^#\[cfg\(test\)\]\s*\n\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{/m.exec(src);
	return m ? src.slice(0, m.index) : src;
}

export function scanRustSource(source: string): Violation[] {
	const src = withoutTestTail(source);
	const lines = src.split("\n");
	const out: Violation[] = [];
	for (const m of src.matchAll(MACRO)) {
		const at = m.index ?? 0;
		const line = lineOf(src, at);
		const prefix = src.slice(src.lastIndexOf("\n", at - 1) + 1, at);
		if (prefix.includes("//")) continue; // commented out
		if (/i18n-ignore/.test(lines[line - 1] ?? "") || /i18n-ignore/.test(lines[line - 2] ?? "")) continue;
		const lit = literalAt(src, at + m[0].length);
		if (!lit) continue; // `bail!(err)`, `println!()`: nothing literal to translate
		const text = visibleText(lit[0]);
		if (!LETTERS.test(text) || /^\s*step:/.test(text)) continue;
		out.push({ line, text: lit[0].trim().replace(/\s+/g, " ").slice(0, 60) });
	}
	return out;
}

const isTestFile = (p: string) => /(?:^|\/)tests?(?:\/|\.rs$)/.test(p) || /_tests?\.rs$/.test(p);

export function scanRust(root: string, dirs: string[] = RUST_DIRS): Found {
	const found: Found = new Map();
	for (const dir of dirs) {
		for (const path of walk(`${root}/${dir}`, (p) => p.endsWith(".rs"))) {
			const file = rel(root, path);
			if (isTestFile(file.slice(dir.length))) continue;
			const v = scanRustSource(readText(path));
			if (v.length) found.set(file, v);
		}
	}
	return found;
}
