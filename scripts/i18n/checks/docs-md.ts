// Translated docs: docs/<locale>/<slug>.md next to docs/<slug>.md. A translation's front matter names its English
// source and the git blob sha of that file when it was translated (`source_sha`, what `git hash-object` prints);
// a different sha today means the English moved on (a warning: a typo fix must not block a PR). Rules:
// docs-frontmatter (error), docs-outdated (warn), docs-structure (warn), docs-missing (info), pt-pt (warn).
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { ptPtHits } from "../glossary.ts";
import { gitBlobSha } from "../hash.ts";
import type { Checker, Ctx, Finding, StatusRow } from "../types.ts";
import { lineOf, readText, rel } from "../util.ts";

const DOCS = "docs";

export interface FrontMatter {
	title?: unknown;
	source?: unknown;
	source_sha?: unknown;
	/** Optional: `machine` marks a page translated by a model and not yet reviewed by a person (the site shows a banner). */
	translation?: unknown;
}

/** The front matter of a markdown file and the offset where the body starts, or undefined when it has none. */
export function frontMatter(text: string): { data: FrontMatter; bodyStart: number } | undefined {
	const m = /^---\r?\n([\s\S]*?)\r?\n---[ \t]*(?:\r?\n|$)/.exec(text);
	if (!m) return undefined;
	const data = Bun.YAML.parse(m[1]);
	if (typeof data !== "object" || data === null || Array.isArray(data)) throw new Error("front matter must be a mapping");
	return { data, bodyStart: m[0].length };
}

/** Fenced code blocks (their content) and the heading count of a markdown body; headings inside fences do not count. */
export function structure(body: string): { headings: number; blocks: string[] } {
	const blocks: string[] = [];
	let headings = 0;
	let fence: string | undefined;
	let cur: string[] = [];
	for (const line of body.split(/\r?\n/)) {
		const f = /^\s*(```+|~~~+)/.exec(line);
		if (f && fence === undefined) {
			fence = f[1][0];
			cur = [];
		} else if (f && f[1][0] === fence) {
			blocks.push(cur.join("\n"));
			fence = undefined;
		} else if (fence !== undefined) cur.push(line);
		else if (/^#{1,6}\s/.test(line)) headings++;
	}
	return { headings, blocks };
}

/** The executable part of a code block: comments (`#` and `//` to the end of the line, when they start a word)
 * dropped, whitespace trimmed, blank lines removed. A translated comment is not a changed command. */
export function codeSignature(block: string): string {
	return block
		.split("\n")
		.map((l) => l.replace(/(^|\s)(?:#|\/\/).*$/, "").trim())
		.filter(Boolean)
		.join("\n");
}

const englishDocs = (root: string) =>
	existsSync(join(root, DOCS)) ? readdirSync(join(root, DOCS)).filter((n) => n.endsWith(".md") && n !== "README.md").sort() : [];

const targets = (ctx: Ctx) => Object.keys(ctx.locales.locales).filter((l) => l !== ctx.locales.default);

interface Doc {
	locale: string;
	file: string;
	slug: string;
	findings: Finding[];
	state: "fresh" | "outdated";
}

function checkOne(ctx: Ctx, locale: string, name: string): Doc {
	const { root } = ctx;
	const path = join(root, DOCS, locale, name);
	const file = rel(root, path);
	const text = readText(path);
	const findings: Finding[] = [];
	const bad = (message: string, line?: number): void => void findings.push({ rule: "docs-frontmatter", severity: "error", file, line, message });
	let fm: ReturnType<typeof frontMatter>;
	try {
		fm = frontMatter(text);
	} catch (e) {
		bad(`invalid front matter: ${(e as Error).message}`, 1);
		return { locale, file, slug: name, findings, state: "outdated" };
	}
	if (!fm) {
		bad("missing front matter (---, title, source, source_sha, ---)", 1);
		return { locale, file, slug: name, findings, state: "outdated" };
	}
	const { title, source, source_sha: sha, translation } = fm.data;
	if (translation !== undefined && translation !== "machine") bad(`\`translation\` must be \`machine\` (AI-translated, not reviewed yet) or absent, not ${JSON.stringify(translation)}`, 1);
	if (typeof title !== "string" || title.trim() === "") bad("`title` is missing or empty", 1);
	if (typeof sha !== "string" || !/^[0-9a-f]{40}$/.test(sha)) bad("`source_sha` must be a quoted string of the 40-hex git blob sha of the English file (git hash-object docs/<source>); an unquoted all-digit sha parses as a number", 1);
	let state: Doc["state"] = "outdated";
	if (typeof source !== "string" || source.trim() === "") bad("`source` is missing (the English file in docs/, e.g. contributing.md)", 1);
	else if (source.includes("/") || source.includes("\\") || !existsSync(join(root, DOCS, source))) bad(`\`source\` ${JSON.stringify(source)} is not a file in docs/ (the English page was renamed or deleted)`, 1);
	else {
		const english = readText(join(root, DOCS, source));
		if (typeof sha === "string" && /^[0-9a-f]{40}$/.test(sha)) {
			const now = gitBlobSha(english);
			if (sha !== now) findings.push({ rule: "docs-outdated", severity: "warn", file, line: 1, message: `docs/${source} changed since this was translated (source_sha ${sha.slice(0, 7)}, now ${now.slice(0, 7)}); update the translation, then set source_sha to ${now}` });
			else state = "fresh";
		}
		const en = structure(english.slice(frontMatter(english)?.bodyStart ?? 0));
		const tr = structure(text.slice(fm.bodyStart));
		if (en.headings !== tr.headings) findings.push({ rule: "docs-structure", severity: "warn", file, message: `${tr.headings} heading(s), the English has ${en.headings}` });
		if (en.blocks.length !== tr.blocks.length) findings.push({ rule: "docs-structure", severity: "warn", file, message: `${tr.blocks.length} code block(s), the English has ${en.blocks.length}` });
		else en.blocks.forEach((b, i) => {
			if (codeSignature(b) !== codeSignature(tr.blocks[i])) findings.push({ rule: "docs-structure", severity: "warn", file, message: `code block ${i + 1} differs from the English (comments aside)` });
		});
	}
	if (locale === "pt-BR") {
		const prose = text.slice(fm.bodyStart).replace(/^\s*(```+|~~~+)[\s\S]*?^\s*\1/gm, (m) => m.replace(/[^\n]/g, " ")).replace(/`[^`\n]*`/g, (m) => " ".repeat(m.length));
		for (const h of ptPtHits(prose)) {
			const at = prose.toLowerCase().indexOf(h.word.toLowerCase());
			findings.push({ rule: "pt-pt", severity: "warn", file, line: lineOf(text, fm.bodyStart + at), message: `"${h.word}" is pt-PT; pt-BR says ${h.use}` });
		}
	}
	return { locale, file, slug: typeof source === "string" ? source : name, findings, state };
}

function collect(ctx: Ctx): { docs: Doc[]; missing: { locale: string; name: string }[] } {
	const docs: Doc[] = [];
	const missing: { locale: string; name: string }[] = [];
	const english = englishDocs(ctx.root);
	for (const locale of targets(ctx)) {
		const dir = join(ctx.root, DOCS, locale);
		const have = existsSync(dir) ? readdirSync(dir).filter((n) => n.endsWith(".md")).sort() : [];
		for (const name of have) docs.push(checkOne(ctx, locale, name));
		for (const name of english) if (!have.includes(name)) missing.push({ locale, name });
	}
	return { docs, missing };
}

const checker: Checker = {
	name: "docs-md",

	async run(ctx) {
		if (ctx.surface && ctx.surface !== "docs") return [];
		const { docs, missing } = collect(ctx);
		return [
			...docs.flatMap((d) => d.findings),
			...missing.map((m): Finding => ({ rule: "docs-missing", severity: "info", file: `${DOCS}/${m.locale}/${m.name}`, message: `no ${m.locale} translation of docs/${m.name} yet` })),
		];
	},

	async status(ctx) {
		if (ctx.surface && ctx.surface !== "docs") return [];
		const { docs, missing } = collect(ctx);
		const total = englishDocs(ctx.root).length;
		return targets(ctx).map((locale): StatusRow => {
			const mine = docs.filter((d) => d.locale === locale);
			const fresh = mine.filter((d) => d.state === "fresh").length;
			return { surface: "docs", locale, area: "docs", total, fresh, outdated: mine.length - fresh, missing: missing.filter((m) => m.locale === locale).length };
		});
	},
};

export default checker;
