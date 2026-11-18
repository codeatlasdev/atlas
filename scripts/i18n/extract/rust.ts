// The Rust half of `bun run i18n:extract`: a per-file worklist, never a codemod. A rewrite of
// `println!("Deleted {} files", n)` into `println!("{}", t!("area_key", count = n))` is mechanical for a text with no
// placeholder and for plain `{}` / `{name}`, but most of the 575 call sites are not that: width/debug specs (`{:>8}`,
// `{:?}`), ANSI escapes wrapped around the text, strings split over several calls, arguments whose type is not a
// `Value` (`path.display()`), error messages some caller may match, daemon log lines that are not UI at all. A wrong
// rewrite compiles and changes what a person reads, and the only check is a heavy build and a manual run. So the tool
// finds the sites with the SAME detector rules as the ratchet (a test keeps the two equal), classifies each, and for the
// mechanical ones writes the suggested key, English and call; an agent applies them by hand.
//
// Machine output stays untouched: `println!("{}", json)` is not a site (the ratchet skips it), JSON written as a
// format string is classified `machine`, and the surfaces crates/atlas-cli/tests/machine_output.rs pins are listed in
// MACHINE_SURFACES so the batch notes can name them.
import { withoutTestTail } from "../hardcoded/rust.ts";
import { lineOf } from "../util.ts";

export type RustKind =
	/** No placeholder: `t!("key")`. */
	| "text"
	/** Plain `{}` / `{name}` placeholders with simple arguments: `t!("key", name = value)`. */
	| "vars"
	/** Needs a decision, see `reasons`. */
	| "judgement"
	/** JSON or a protocol line written as a format string: leave it, mark `// i18n-ignore` with the reason. */
	| "machine"
	/** `eprintln!` in a daemon or the desktop shell: a log line for a maintainer, probably `// i18n-ignore`. */
	| "log"
	/** Text the ratchet does not count (`.context("...")`, ratatui `Span::raw("...")`) in a file being migrated: look at it. */
	| "uncounted";

export interface RustItem {
	line: number;
	macro: string;
	/** The format string as the source spells it (escapes kept). */
	format: string;
	args: string[];
	kind: RustKind;
	/** Why it is `judgement`. */
	reasons: string[];
	/** Things to check even when it is mechanical. */
	notes: string[];
	/** Suggested catalog entry and call (`vars` and `text` only). */
	english?: string;
	call?: string;
}

/** The commands whose output a program reads (crates/atlas-cli/tests/machine_output.rs): never translated. */
export const MACHINE_SURFACES = ["atlas config schema", "atlas heavy status --json", "atlas mcp (JSON-RPC)"];

const MACRO = /\b(println|eprintln|print|eprint|bail|anyhow)!\s*\(\s*/g;
const LETTERS = /\p{L}/u;
const UNCOUNTED = [/\.(?:with_)?context\(\s*(?:\|\|\s*)?(?:format!\(\s*)?"/, /\b(?:Span::raw|Span::styled|Line::from|Paragraph::new|Text::from)\(\s*"/];

/** The string literal at `i`: its source text (escapes kept), its decoded text and the index after it. */
function literalAt(src: string, i: number): { raw: string; text: string; end: number } | undefined {
	const raw = /^r(#*)"/.exec(src.slice(i, i + 12));
	if (raw) {
		const close = `"${raw[1]}`;
		const start = i + raw[0].length;
		const end = src.indexOf(close, start);
		return end < 0 ? undefined : { raw: src.slice(start, end), text: src.slice(start, end), end: end + close.length };
	}
	if (src[i] !== '"') return undefined;
	let j = i + 1;
	let text = "";
	while (j < src.length && src[j] !== '"') {
		if (src[j] === "\\") {
			if (src[j + 1] === "\n") {
				j += 2;
				while (/[ \t]/.test(src[j] ?? "")) j++;
				continue;
			}
			const esc = /^(?:x([0-9a-fA-F]{2})|u\{([0-9a-fA-F_]+)\})/.exec(src.slice(j + 1, j + 12));
			if (esc) text += String.fromCodePoint(Number.parseInt((esc[1] ?? esc[2]).replaceAll("_", ""), 16));
			else text += "ntr".includes(src[j + 1]) ? " " : src[j + 1];
			j += 1 + (esc ? esc[0].length : 1);
			continue;
		}
		text += src[j++];
	}
	return { raw: src.slice(i + 1, j), text, end: j + 1 };
}

/** The top-level, comma-separated arguments after the format string, up to the call's closing parenthesis. */
function argsFrom(src: string, from: number): string[] {
	const out: string[] = [];
	let depth = 0;
	let cur = "";
	for (let i = from; i < src.length; i++) {
		const c = src[i];
		if (c === '"') {
			const lit = literalAt(src, i);
			if (!lit) break;
			cur += src.slice(i, lit.end);
			i = lit.end - 1;
			continue;
		}
		if (c === "'" && /^'(?:\\.[^']*|[^\\'])'/.test(src.slice(i, i + 12))) {
			const len = /^'(?:\\.[^']*|[^\\'])'/.exec(src.slice(i, i + 12))?.[0].length ?? 1;
			cur += src.slice(i, i + len);
			i += len - 1;
			continue;
		}
		if ("([{".includes(c)) depth++;
		if (")]}".includes(c)) {
			if (depth === 0) break;
			depth--;
		}
		if (c === "," && depth === 0) {
			if (cur.trim()) out.push(cur.trim());
			cur = "";
			continue;
		}
		cur += c;
	}
	if (cur.trim()) out.push(cur.trim());
	return out;
}

interface Placeholder {
	/** `""` for `{}`, an index, or a name. */
	id: string;
	spec: string;
}

function placeholdersOf(format: string): Placeholder[] {
	const out: Placeholder[] = [];
	for (const m of format.replace(/\{\{|\}\}/g, "").matchAll(/\{([^{}:]*)(?::([^{}]*))?\}/g)) out.push({ id: m[1].trim(), spec: (m[2] ?? "").trim() });
	return out;
}

/** `item.name` -> "name", `count` -> "count"; undefined for anything else (a call, an index, a reference to a call). */
const nameOfArg = (a: string) => /^&?(?:[A-Za-z_]\w*\.)*([A-Za-z_]\w*)$/.exec(a.trim())?.[1];

const slug = (s: string) =>
	s
		.replace(/\{[^{}]*\}/g, " ")
		.toLowerCase()
		.split(/[^a-z0-9]+/)
		.filter(Boolean)
		.slice(0, 5)
		.join("_") || "text";

const WRAPS: Record<string, (call: string) => string> = {
	println: (c) => `println!("{}", ${c})`,
	eprintln: (c) => `eprintln!("{}", ${c})`,
	print: (c) => `print!("{}", ${c})`,
	eprint: (c) => `eprint!("{}", ${c})`,
	bail: (c) => `bail!("{}", ${c})`,
	anyhow: (c) => `anyhow!("{}", ${c})`,
};

/** Every site of one Rust file (the ratchet's sites, classified) and the uncounted candidates. `area` prefixes the suggested keys. */
export function rustWorklist(file: string, source: string, area: string): RustItem[] {
	const src = withoutTestTail(source);
	const lines = src.split("\n");
	const crate = /^crates\/([^/]+)\//.exec(file)?.[1] ?? "";
	const out: RustItem[] = [];
	for (const m of src.matchAll(MACRO)) {
		const at = m.index ?? 0;
		const line = lineOf(src, at);
		const prefix = src.slice(src.lastIndexOf("\n", at - 1) + 1, at);
		if (prefix.includes("//")) continue;
		if (/i18n-ignore/.test(lines[line - 1] ?? "") || /i18n-ignore/.test(lines[line - 2] ?? "")) continue;
		const lit = literalAt(src, at + m[0].length);
		if (!lit) continue;
		const visible = lit.text.replace(/\u001b\[[0-9;]*[A-Za-z]/g, "").replace(/\{\{|\}\}/g, "").replace(/\{[^{}]*\}/g, "");
		if (!LETTERS.test(visible) || /^\s*step:/.test(visible)) continue;

		const args = argsFrom(src, lit.end);
		const named = args.filter((a) => /^[A-Za-z_]\w*\s*=[^=]/.test(a));
		const positional = args.filter((a) => !named.includes(a));
		const ph = placeholdersOf(lit.text);
		const macro = m[1];
		const item: RustItem = { line, macro, format: lit.raw.replace(/\s+/g, " ").trim(), args, kind: "text", reasons: [], notes: [] };
		const reasons = item.reasons;

		if (/^\s*\{\{.*\}\}\s*$/s.test(lit.raw) || /\{\{\s*\\?"[A-Za-z_]+\\?"\s*:/.test(lit.raw)) {
			item.kind = "machine";
			item.notes.push("JSON written as a format string: machine output is never translated; add `// i18n-ignore: machine output` on the line above");
			out.push(item);
			continue;
		}
		if ((crate === "atlas-host" || crate === "atlas-desktop") && (macro === "eprintln" || macro === "eprint")) {
			item.kind = "log";
			item.notes.push("a daemon or shell log line for a maintainer: logs are never translated; add `// i18n-ignore: log` unless a person sees it in the app");
			out.push(item);
			continue;
		}
		if (/\u001b|\\x1b|\\u\{1b\}|\\e\b/.test(lit.raw) || lit.text.includes("\u001b")) reasons.push("ansi: color codes sit inside the text; keep the styling in code (style the translated string as a whole) and never put escapes in a catalog");
		if (/\{\{|\}\}/.test(lit.raw)) reasons.push("braces: `{{` `}}` print literal braces; a catalog has no escape for them");
		if (lit.raw.includes("\n") || /\\n(?!$)/.test(lit.raw.replace(/\\n$/, ""))) reasons.push("multi-line: several lines in one call; one message per line, or one message with the whole block when the lines belong together");
		for (const p of ph) {
			if (p.spec) reasons.push(`format-spec: {${p.id}:${p.spec}} formats width, precision or Debug; format the value first (let s = format!(...)) and pass the string`);
			if (/^\d+$/.test(p.id)) reasons.push("positional-index: {0}/{1} reuse or reorder arguments; name them");
		}
		const empties = ph.filter((p) => p.id === "").length;
		if (empties > positional.length) reasons.push("arguments: fewer arguments than {} placeholders (a name captured inline mixed with positional ones): name every placeholder");
		if (macro === "bail" || macro === "anyhow") item.notes.push("an error message: grep the repo for callers that `contains()` or compare it (a program never parses human text), and keep `Coded` ids for failures a client must tell apart");
		if (args.length) item.notes.push("t! values are &str, String, &String or numbers: wrap any other Display type (`path.display()`, an enum) in `.to_string()`");
		if (/[\u2713\u2717\u25b8\u25cf\u2022]|[✓✗▸]/u.test(lit.text)) item.notes.push("a status glyph (✓ ✗ ▸) leads the line: keep it in code before the translated text");

		// the English with every placeholder named, for the suggested catalog entry
		let n = 0;
		const vars: string[] = [];
		const english = lit.text
			.replace(/\u001b\[[0-9;]*[A-Za-z]/g, "")
			.replace(/\{\{/g, "\u0001")
			.replace(/\}\}/g, "\u0002")
			.replace(/\{([^{}:]*)(?::[^{}]*)?\}/g, (_, id: string) => {
				const key = id.trim();
				let name = key;
				if (key === "") {
					const a = positional[n++] ?? "";
					name = nameOfArg(a) ?? `value${n}`;
					if (!nameOfArg(a) && !item.notes.some((x) => x.startsWith("name:"))) item.notes.push("name: value1, value2… stand for arguments that are calls or expressions; give each a meaningful name in the message and the call");
					vars.push(`${name} = ${a}`);
				} else if (/^\d+$/.test(key)) name = `value${key}`;
				else {
					const given = named.find((x) => x.startsWith(`${key}`) && /^[A-Za-z_]\w*\s*=/.test(x));
					vars.push(given ? given.replace(/\s*=\s*/, " = ") : `${name} = ${key}`);
				}
				return `{${name}}`;
			})
			.replace(/\u0001/g, "{")
			.replace(/\u0002/g, "}")
			.replace(/\\n$|\n$/, "")
			.trim();
		const unique = [...new Set(vars)];
		const base = `${area}_${slug(english)}`;
		const call = `t!("${base}"${unique.length ? `, ${unique.join(", ")}` : ""})`;
		item.english = english;
		item.call = WRAPS[macro](call);
		item.kind = reasons.length ? "judgement" : ph.length ? "vars" : "text";
		out.push(item);
	}
	lines.forEach((text, i) => {
		if (/^\s*\/\//.test(text) || /i18n-ignore/.test(text) || /i18n-ignore/.test(lines[i - 1] ?? "")) return;
		if (UNCOUNTED.some((re) => re.test(text)) && LETTERS.test(text.replace(/\{[^{}]*\}/g, ""))) {
			out.push({ line: i + 1, macro: "", format: text.trim().slice(0, 100), args: [], kind: "uncounted", reasons: [], notes: ["not counted by the ratchet (an error context or a ratatui widget): if a person reads it, migrate it with the batch"] });
		}
	});
	return out.sort((a, b) => a.line - b.line);
}
