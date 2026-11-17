// The writing lint: rules are data (i18n/style/flagged-terms.yaml, its header is the spec), the code only applies
// them. It reads catalog values (scope `ui`) and Markdown prose (scope `docs`); code, URLs and comments are not prose.
import { existsSync } from "node:fs";
import { join } from "node:path";
import type { Finding } from "./types.ts";
import { lineOf, readText, rel, walk } from "./util.ts";

export const FLAGGED_TERMS_FILE = "i18n/style/flagged-terms.yaml";

export interface Rule {
	id: string;
	lang: "en" | "pt-BR" | "any";
	scope: "ui" | "docs" | "any";
	severity: "error" | "warn";
	re: RegExp;
	except: RegExp[];
	suggest: string;
}

export interface Hit {
	rule: Rule;
	/** Offset of the match in the text that was linted. */
	index: number;
	text: string;
}

const WORD = "[\\p{L}\\p{N}_]";

/** Rules from the YAML; a malformed file or rule is a finding on the YAML, never a silently dropped rule. */
export function loadRules(root: string): { rules: Rule[]; problems: Finding[] } {
	const path = join(root, FLAGGED_TERMS_FILE);
	if (!existsSync(path)) return { rules: [], problems: [] };
	const problems: Finding[] = [];
	const bad = (message: string, key?: string) => problems.push({ rule: "writing-rules", severity: "error", file: FLAGGED_TERMS_FILE, key, message });
	let doc: unknown;
	try {
		doc = Bun.YAML.parse(readText(path));
	} catch (e) {
		bad(`invalid YAML: ${(e as Error).message}`);
		return { rules: [], problems };
	}
	const list = typeof doc === "object" && doc !== null && "rules" in doc && Array.isArray(doc.rules) ? doc.rules : undefined;
	if (!list) {
		bad("expected `rules:` to be a list");
		return { rules: [], problems };
	}
	const rules: Rule[] = [];
	const seen = new Set<string>();
	for (const r of list) {
		const id = typeof r?.id === "string" ? r.id : undefined;
		if (!id || typeof r.pattern !== "string") {
			bad("a rule needs a string `id` and `pattern`", id);
			continue;
		}
		if (seen.has(id)) bad("duplicate rule id", id);
		seen.add(id);
		const lang = r.lang ?? "any";
		const scope = r.scope ?? "any";
		const severity = r.severity;
		if (!["en", "pt-BR", "any"].includes(lang) || !["ui", "docs", "any"].includes(scope) || !["error", "warn"].includes(severity)) {
			bad("`lang` (en | pt-BR | any), `scope` (ui | docs | any) or `severity` (error | warn) is invalid", id);
			continue;
		}
		try {
			const flags = [...new Set(`${typeof r.flags === "string" ? r.flags : "iu"}g`)].join("");
			const source = r.word === false ? r.pattern : `(?<!${WORD})(?:${r.pattern})(?!${WORD})`;
			rules.push({
				id,
				lang,
				scope,
				severity,
				re: new RegExp(source, flags),
				except: (Array.isArray(r.except) ? r.except : []).map((x: string) => new RegExp(x, "iu")),
				suggest: typeof r.suggest === "string" ? r.suggest.trim() : "",
			});
		} catch (e) {
			bad(`pattern does not compile: ${(e as Error).message}`, id);
		}
	}
	return { rules, problems };
}

/** The sentence around [start, end) of `text`, for `except`. */
function sentence(text: string, start: number, end: number): string {
	const before = text.slice(0, start);
	const from = Math.max(before.lastIndexOf(". "), before.lastIndexOf("! "), before.lastIndexOf("? "), before.lastIndexOf("\n")) + 1;
	const after = /[.!?](?:\s|$)|\n/.exec(text.slice(end));
	return text.slice(from, after ? end + after.index + 1 : text.length);
}

/** Every hit of the rules that apply to `lang` (`en`, `pt-BR`, another locale: only `any` rules) and `scope`. */
export function lintText(rules: Rule[], text: string, lang: string, scope: "ui" | "docs"): Hit[] {
	const hits: Hit[] = [];
	for (const rule of rules) {
		if ((rule.lang !== "any" && rule.lang !== lang) || (rule.scope !== "any" && rule.scope !== scope)) continue;
		for (const m of text.matchAll(rule.re)) {
			const index = m.index ?? 0;
			if (m[0] === "") continue;
			const s = sentence(text, index, index + m[0].length);
			if (rule.except.some((x) => x.test(s))) continue;
			hits.push({ rule, index, text: m[0] });
		}
	}
	return hits.sort((a, b) => a.index - b.index);
}

const blank = (m: string) => m.replace(/[^\n]/g, " ");

/** Markdown with everything that is not prose replaced by spaces (so offsets and line numbers hold): front matter,
 * fenced code, HTML comments, inline code, link targets, URLs, HTML tags. */
export function markdownProse(text: string): string {
	return text
		.replace(/^---\r?\n[\s\S]*?\r?\n---[ \t]*(?:\r?\n|$)/, blank)
		.replace(/^[ \t]*(```+|~~~+)[^\n]*\n[\s\S]*?^[ \t]*\1[ \t]*$/gm, blank)
		.replace(/<!--[\s\S]*?-->/g, blank)
		.replace(/`+[^`\n]*`+/g, blank)
		.replace(/\]\([^)\n]*\)/g, (m) => `]${blank(m.slice(1))}`)
		.replace(/^\[[^\]\n]+\]:[ \t]*\S+/gm, blank)
		.replace(/https?:\/\/[^\s)>\]]+/g, blank)
		.replace(/<\/?[A-Za-z][^>\n]*>/g, blank);
}

/** `writing-lint: allow RULE_ID: reason`: the ids waived, and those waived without a reason. */
export function allowsIn(text: string): { ids: Set<string>; noReason: string[] } {
	const ids = new Set<string>();
	const noReason: string[] = [];
	for (const m of text.matchAll(/writing-lint:\s*allow\s+([\w-]+)\s*(?::\s*([^\n]*?))?\s*(?:-->|$)/gm)) {
		if (m[2]?.trim()) ids.add(m[1]);
		else noReason.push(m[1]);
	}
	return { ids, noReason };
}

export interface MarkdownHit {
	line: number;
	rule: Rule;
	text: string;
}

/** Hits in the prose of a Markdown file, minus those waived by an allow comment on the line before. */
export function lintMarkdown(rules: Rule[], text: string, lang: string): { hits: MarkdownHit[]; noReason: { line: number; id: string }[] } {
	const lines = text.split("\n");
	const waived = new Map<number, Set<string>>();
	const noReason: { line: number; id: string }[] = [];
	let pending = new Set<string>();
	lines.forEach((l, i) => {
		const a = /^\s*<!--\s*writing-lint:/.test(l) ? allowsIn(l) : undefined;
		if (a) {
			for (const id of a.ids) pending.add(id);
			for (const id of a.noReason) noReason.push({ line: i + 1, id });
		} else {
			if (pending.size) waived.set(i + 1, pending);
			pending = new Set();
		}
	});
	const hits = lintText(rules, markdownProse(text), lang, "docs")
		.map((h) => ({ line: lineOf(text, h.index), rule: h.rule, text: h.text }))
		.filter((h) => !waived.get(h.line)?.has(h.rule.id));
	return { hits, noReason };
}

/** Text of a catalog value that is prose: placeholders, markup tags, code spans and URLs removed. */
export const uiProse = (text: string) => text.replace(/\{[^{}]*\}/g, " ").replace(/`[^`\n]*`/g, " ").replace(/https?:\/\/\S+/g, " ");

export const hitMessage = (h: { rule: Rule; text: string }) => `"${h.text}"${h.rule.suggest ? `: ${h.rule.suggest}` : ""}`;

/** Markdown files that carry prose, repo-relative, split into the new-by-policy ones (`strict`: every translation
 * under docs/<locale>/) and the existing ones (`existing`: English docs, README, skills, site content, CHANGELOG),
 * which are ratcheted against a baseline instead. */
export function docFiles(root: string, nonDefaultLocales: string[]): { strict: { file: string; lang: string }[]; existing: string[] } {
	const strict: { file: string; lang: string }[] = [];
	const existing: string[] = [];
	for (const path of walk(join(root, "docs"), (p) => p.endsWith(".md"))) {
		const file = rel(root, path);
		const locale = nonDefaultLocales.find((l) => file.startsWith(`docs/${l}/`));
		if (locale) strict.push({ file, lang: locale });
		else existing.push(file);
	}
	for (const f of ["README.md", "CHANGELOG.md"]) if (existsSync(join(root, f))) existing.push(f);
	for (const dir of ["skills", "site/src/content"]) {
		for (const path of walk(join(root, dir), (p) => /\.mdx?$/.test(p))) existing.push(rel(root, path));
	}
	return { strict, existing };
}
