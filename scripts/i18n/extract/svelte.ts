// The Svelte half of `bun run i18n:extract`: find the user-facing text of one component on the real svelte/compiler AST
// and say, for each piece, whether it can be moved into the catalog safely (a candidate: an exact span to replace and the
// English message with its variables) or why not (a refusal with file:line and a reason). Pure: no catalog, no files.
//
// What it moves: a run of text nodes (with `{simple}` variables between them) and the static attributes the ratchet counts
// (`title`, `placeholder`, `aria-label`, `aria-description`, `alt`, `label`, `tooltip`, `description`, `message`), also on
// kit components (`<Select label="..." />`). What it refuses, with a reason, and leaves byte for byte: plurals, text joined
// to a call or any other expression, text split by inline markup (`Press <kbd>x</kbd>`), template literals with logic,
// string literals inside mustache expressions, shortcuts (those go through kbd()), URLs, identifier-like tokens, braces.
// `<code>`, `<pre>`, `<kbd>`, `<script>`, `<style>` and the node after `<!-- i18n-ignore -->` are skipped like the ratchet.
import { type AST, parse } from "svelte/compiler";
import { SHORTCUT } from "../hardcoded/lines.ts";
import { lineOf } from "../util.ts";

export type Reason =
	| "plural"
	| "concatenation"
	| "inline-markup"
	| "template-literal"
	| "expression-literal"
	| "ambiguous-variable"
	| "shortcut"
	| "url"
	| "identifier-like"
	| "braces"
	| "near-common"
	| "catalog-rule"
	| "shadowed-m"
	| "script-string"
	| "ts-module"
	| "parse-error";

export interface Refusal {
	line: number;
	reason: Reason;
	text: string;
	detail?: string;
	/** How many strings of the hardcoded ratchet this accounts for (0: something the ratchet does not count). */
	covers: number;
}

export interface Variable {
	name: string;
	/** The source expression the caller passes for it. */
	expr: string;
}

export interface Candidate {
	line: number;
	kind: "text" | "attr";
	attr?: string;
	/** Element or component the text sits in (the owner of the attribute), for the description. */
	tag: string;
	/** Strings of the hardcoded ratchet this replaces. */
	covers: number;
	english: string;
	vars: Variable[];
	/** The span of the source replaced by `{m.key(...)}` (text) or `attr={m.key(...)}` (attribute). */
	start: number;
	end: number;
}

export interface Analysis {
	candidates: Candidate[];
	refused: Refusal[];
	/** The file already has `import { m } ...`, so none is added. */
	hasImport: boolean;
	instance?: { importEnd?: number; indent: string; contentStart: number; lang?: string };
	/** The `lang` of any script of the file, so a created script matches it. */
	lang?: string;
}

/** The same attribute names the `hardcoded` ratchet counts (scripts/i18n/hardcoded/svelte.ts; a test keeps them equal). */
export const ATTRS = new Set(["title", "placeholder", "aria-label", "aria-description", "alt", "label", "tooltip", "description", "message"]);
const SKIP_ELEMENTS = new Set(["code", "pre", "kbd", "style", "script"]);
/** Elements that sit inside a sentence: text next to one of these is a sentence in pieces, which a translator cannot reorder. */
const INLINE = new Set(["a", "abbr", "b", "bdi", "bdo", "cite", "code", "data", "dfn", "em", "i", "kbd", "mark", "q", "s", "samp", "small", "span", "strong", "sub", "sup", "time", "u", "var"]);
const LETTERS = /\p{L}/u;
const WS = /[ \t\r\n]+/g;

interface SNode {
	type: string;
	start: number;
	end: number;
	[k: string]: unknown;
}

const asNode = (v: unknown): SNode | undefined =>
	typeof v === "object" && v !== null && "type" in v && typeof v.type === "string" && "start" in v && typeof v.start === "number" ? (v as SNode) : undefined;

function fragmentNodes(v: unknown): unknown[] | undefined {
	if (typeof v !== "object" || v === null || !("type" in v) || v.type !== "Fragment") return undefined;
	return "nodes" in v && Array.isArray(v.nodes) ? v.nodes : undefined;
}

const str = (v: unknown) => (typeof v === "string" ? v : "");
const compact = (s: string, n = 80) => s.replace(/\s+/g, " ").trim().slice(0, n);

/** A piece of a message: literal text, or an expression the caller fills. */
type Piece = { text: string } | { expr: SNode };

/** `item.name` -> "name", `count` -> "count", anything with a call, operator or index -> undefined. */
function simpleName(e: SNode | undefined): string | undefined {
	let n: SNode | undefined = e;
	if (n?.type === "ChainExpression") n = asNode(n.expression);
	if (n?.type === "Identifier") return str(n.name);
	if (n?.type !== "MemberExpression" || n.computed === true) return undefined;
	const prop = asNode(n.property);
	if (prop?.type !== "Identifier" || !simpleRoot(asNode(n.object))) return undefined;
	return str(prop.name);
}

function simpleRoot(e: SNode | undefined): boolean {
	if (!e) return false;
	if (e.type === "Identifier") return true;
	if (e.type !== "MemberExpression" || e.computed === true) return false;
	return asNode(e.property)?.type === "Identifier" && simpleRoot(asNode(e.object));
}

const PLURAL_TEST = /(?:[=!]==?|[<>]=?)\s*1\b|\b1\s*(?:[=!]==?|[<>]=?)|\.length\b|plural/;

/** Why an expression cannot be a plain variable: a plural switch, or any other computation. */
function whyNotSimple(source: string, e: SNode): { reason: Reason; detail: string } {
	const text = compact(source.slice(e.start, e.end), 60);
	const branchLiterals = literalsOf(e).filter((l) => /^(?:s|es|ies|)$/.test(l.text));
	if ((e.type === "ConditionalExpression" || e.type === "LogicalExpression") && (branchLiterals.length > 0 || PLURAL_TEST.test(source.slice(e.start, e.end)))) {
		return { reason: "plural", detail: `{${text}} picks a word by a count: write one message with plural variants (i18n/README.md, Message values) and pass the count` };
	}
	return { reason: "concatenation", detail: `{${text}} is computed (call, operator, condition): the sentence is built in pieces; compute the value first, then pass it as one variable` };
}

/** The string literals a user may read through an expression: the expression itself, the branches of `?:`, the sides of `||`/`??`. */
function literalsOf(e: SNode | undefined, out: { node: SNode; text: string }[] = []): { node: SNode; text: string }[] {
	if (!e) return out;
	if (e.type === "Literal" && typeof e.value === "string") {
		if (LETTERS.test(e.value)) out.push({ node: e, text: e.value });
	} else if (e.type === "TemplateLiteral") {
		const text = (Array.isArray(e.quasis) ? e.quasis : []).map((q) => str(asNode(q)?.cooked ?? asNode(asNode(q)?.value)?.cooked)).join("");
		if (LETTERS.test(text)) out.push({ node: e, text });
	} else if (e.type === "ConditionalExpression") {
		literalsOf(asNode(e.consequent), out);
		literalsOf(asNode(e.alternate), out);
	} else if (e.type === "LogicalExpression") {
		literalsOf(asNode(e.left), out);
		literalsOf(asNode(e.right), out);
	}
	return out;
}

interface Built {
	english: string;
	vars: Variable[];
}
type Verdict = Built | { reason: Reason; detail: string };

/** Pieces to one message, or the reason it is not safe to extract. */
function build(source: string, pieces: Piece[]): Verdict {
	let english = "";
	const vars: Variable[] = [];
	for (const p of pieces) {
		if ("text" in p) {
			if (/[{}]/.test(p.text)) return { reason: "braces", detail: "the text has { or }, which a catalog reads as a variable: write it by hand with the right escape" };
			english += p.text.replace(WS, " ");
			continue;
		}
		const name = simpleName(p.expr);
		if (!name) return whyNotSimple(source, p.expr);
		const expr = source.slice(p.expr.start, p.expr.end);
		if (name === "os") return { reason: "ambiguous-variable", detail: "a variable named os collides with the os input every message gets: rename it" };
		const seen = vars.find((v) => v.name === name);
		if (seen && seen.expr !== expr) return { reason: "ambiguous-variable", detail: `{${seen.expr}} and {${expr}} would both be {${name}}: name them apart (compute one into a variable with another name)` };
		if (!seen) vars.push({ name, expr });
		english += `{${name}}`;
	}
	return { english: english.replace(/^ +| +$/g, ""), vars };
}

/** Rules on the finished English that do not depend on the catalog. */
function textVerdict(built: Built): { reason: Reason; detail: string } | undefined {
	const t = built.english;
	if (SHORTCUT.test(t)) return { reason: "shortcut", detail: "a keyboard shortcut is a {key} placeholder filled by kbd() / label() from keys.ts (Rust: atlas_i18n::shortcut), never text in a catalog" };
	const bare = t.replace(/\{[^{}]*\}/g, "").trim();
	if (/^(?:https?:\/\/|www\.)\S+$/.test(bare) || /:\/\//.test(bare)) return { reason: "url", detail: "a URL is not translated: keep it in code" };
	if (!/\s/.test(bare) && /[_/\\@]|\w:\w|\.[A-Za-z]|[a-z][A-Z]|^--|^[#$]/.test(bare)) return { reason: "identifier-like", detail: "one token that looks like an identifier, path, flag or brand: probably not prose; add it by hand if a person reads it" };
	return undefined;
}

/** Elements and components with children or text anywhere inside: `<span class="dot"></span>` is a picture, `<b>here</b>` is words. */
function hasWords(n: SNode): boolean {
	const stack: unknown[] = [n];
	while (stack.length) {
		const v = stack.pop();
		const node = asNode(v);
		if (!node) continue;
		if (node.type === "Text" && LETTERS.test(str(node.data))) return true;
		if (node.type === "ExpressionTag") return true;
		for (const child of Object.values(node)) {
			const nodes = fragmentNodes(child);
			if (nodes) stack.push(...nodes);
		}
	}
	return false;
}

/** The `"s"` of a plural switch is reported once, as the plural, not again as a literal. */
const notSuffix = (l: { text: string }) => !/^(?:s|es|ies)$/.test(l.text);

const isRunNode = (v: unknown) => {
	const n = asNode(v);
	return n?.type === "Text" || n?.type === "ExpressionTag";
};

function neighborIsInline(v: unknown): boolean {
	const n = asNode(v);
	return n?.type === "RegularElement" && INLINE.has(str(n.name)) && hasWords(n);
}

/** Where `m` means something else: the whole file (a top-level binding of the script) or the spans of the template that bind it. */
interface Shadow {
	whole: boolean;
	ranges: [number, number][];
}

/** Does a binding pattern (`m`, `{ m }`, `[a, m]`, `m = 1`, `...m`) bind the name `m`? */
function bindsM(p: unknown): boolean {
	const n = asNode(p);
	if (!n) return false;
	switch (n.type) {
		case "Identifier":
			return n.name === "m";
		case "ObjectPattern":
			return (Array.isArray(n.properties) ? n.properties : []).some((x) => bindsM(asNode(x)?.type === "Property" ? asNode(x)?.value : x));
		case "ArrayPattern":
			return (Array.isArray(n.elements) ? n.elements : []).some(bindsM);
		case "AssignmentPattern":
			return bindsM(n.left);
		case "RestElement":
			return bindsM(n.argument);
		default:
			return false;
	}
}

/** The keys of a node that declare names (skipped when looking for references) and whether declaring `m` there scopes it to the node. */
const BINDERS: Record<string, string[]> = {
	EachBlock: ["context", "index"],
	SnippetBlock: ["parameters", "expression"],
	AwaitBlock: ["value", "error"],
	ArrowFunctionExpression: ["params"],
	FunctionExpression: ["params", "id"],
};

function shadowOf(instance: SNode | undefined, moduleScript: SNode | undefined, fragment: unknown): Shadow {
	const out: Shadow = { whole: false, ranges: [] };
	for (const script of [instance, moduleScript]) {
		for (const raw of Array.isArray(asNode(script?.content)?.body) ? (asNode(script?.content)?.body as unknown[]) : []) {
			let s = asNode(raw);
			if (s?.type === "ExportNamedDeclaration" || s?.type === "ExportDefaultDeclaration") s = asNode(s.declaration);
			if (s?.type === "VariableDeclaration" && (Array.isArray(s.declarations) ? s.declarations : []).some((d) => bindsM(asNode(d)?.id))) out.whole = true;
			if ((s?.type === "FunctionDeclaration" || s?.type === "ClassDeclaration") && asNode(s.id)?.name === "m") out.whole = true;
			if (s?.type === "ImportDeclaration" && !MESSAGES_IMPORT.test(str(asNode(s.source)?.value)) && (Array.isArray(s.specifiers) ? s.specifiers : []).some((x) => asNode(asNode(x)?.local)?.name === "m")) out.whole = true;
		}
	}
	const refs: number[] = [];
	const walk = (v: unknown, parent: SNode | undefined) => {
		if (Array.isArray(v)) return v.forEach((x) => walk(x, parent));
		const n = asNode(v);
		if (!n) {
			if (typeof v === "object" && v !== null) for (const [k, x] of Object.entries(v)) if (k !== "loc") walk(x, parent);
			return;
		}
		if (n.type === "Identifier") {
			const asProp = parent?.type === "MemberExpression" && parent.property === n && parent.computed !== true;
			const asKey = parent?.type === "Property" && parent.key === n && parent.computed !== true && parent.shorthand !== true;
			if (n.name === "m" && !asProp && !asKey) refs.push(n.start);
			return;
		}
		if (n.type === "ConstTag" || n.type === "LetDirective") {
			if (n.type === "LetDirective" ? n.name === "m" : JSON.stringify(n.declaration ?? "").includes('"name":"m"')) out.whole = true;
		}
		const binders = BINDERS[n.type] ?? [];
		if (binders.some((k) => bindsM(n[k]) || (k === "index" && n[k] === "m") || (n.type === "SnippetBlock" && k === "parameters" && (Array.isArray(n[k]) ? (n[k] as unknown[]) : []).some(bindsM)))) out.ranges.push([n.start, n.end]);
		for (const [k, x] of Object.entries(n)) if (k !== "loc" && k !== "name_loc" && !binders.includes(k)) walk(x, n);
	};
	walk(fragment, undefined);
	if (refs.some((r) => !out.ranges.some(([a, b]) => r >= a && r < b))) out.whole = true;
	return out;
}

const MESSAGES_IMPORT = /(?:i18n\/m(?:\.ts)?|paraglide\/messages)/;

function importsM(program: SNode | undefined): boolean {
	const body = Array.isArray(program?.body) ? program.body : [];
	return body.some((s) => {
		const n = asNode(s);
		if (n?.type !== "ImportDeclaration" || !MESSAGES_IMPORT.test(str(asNode(n.source)?.value))) return false;
		return (Array.isArray(n.specifiers) ? n.specifiers : []).some((sp) => asNode(asNode(sp)?.local)?.name === "m");
	});
}

export function analyzeSvelte(source: string): Analysis {
	const out: Analysis = { candidates: [], refused: [], hasImport: false };
	let ast: AST.Root;
	try {
		ast = parse(source, { modern: true });
	} catch (e) {
		out.refused.push({ line: 1, reason: "parse-error", text: "", detail: (e as Error).message, covers: 0 });
		return out;
	}
	const instance = asNode(ast.instance);
	const moduleScript = asNode(ast.module);
	const programOf = (s: SNode | undefined) => asNode(s?.content);
	out.hasImport = importsM(programOf(instance)) || importsM(programOf(moduleScript));
	const langOf = (s: SNode | undefined) => {
		const a = (Array.isArray(s?.attributes) ? s.attributes : []).map(asNode).find((x) => x?.name === "lang");
		const v = Array.isArray(a?.value) ? asNode(a.value[0]) : undefined;
		return typeof v?.data === "string" ? v.data : undefined;
	};
	out.lang = langOf(instance) ?? langOf(moduleScript);
	if (instance) {
		const program = programOf(instance);
		const imports = (Array.isArray(program?.body) ? program.body : []).map(asNode).filter((n) => n?.type === "ImportDeclaration");
		const last = imports[imports.length - 1];
		const lineStart = last ? source.lastIndexOf("\n", last.start - 1) + 1 : 0;
		out.instance = {
			importEnd: last?.end,
			indent: last ? /^[ \t]*/.exec(source.slice(lineStart, last.start))?.[0] ?? "\t" : "\t",
			contentStart: program?.start ?? instance.start,
			lang: out.lang,
		};
	}
	const shadow = out.hasImport ? { whole: false, ranges: [] } : shadowOf(instance, moduleScript, ast.fragment);

	const refuse = (start: number, reason: Reason, text: string, covers: number, detail?: string) => out.refused.push({ line: lineOf(source, start), reason, text: compact(text), detail, covers });

	const accept = (start: number, end: number, line: number, v: Built, extra: Pick<Candidate, "kind" | "attr" | "tag" | "covers">, shown: string) => {
		const bad = textVerdict(v);
		if (bad) return refuse(start, bad.reason, shown, extra.covers, bad.detail);
		if (shadow.whole || shadow.ranges.some(([a, b]) => start >= a && start < b)) return refuse(start, "shadowed-m", shown, extra.covers, "this file already uses an identifier named m, which the message calls would shadow: rename it, then run again");
		out.candidates.push({ ...extra, line, english: v.english, vars: v.vars, start, end });
	};

	const run = (nodes: unknown[], from: number, to: number, tag: string, inSentence: boolean) => {
		const members = nodes.slice(from, to).map((v) => asNode(v) as SNode);
		const first = members[0];
		const last = members[members.length - 1];
		const texts = members.filter((n) => n.type === "Text");
		const lettered = texts.filter((n) => LETTERS.test(str(n.data)));
		for (const n of members.filter((x) => x.type === "ExpressionTag")) {
			for (const l of literalsOf(asNode(n.expression)).filter(notSuffix)) {
				refuse(l.node.start, l.node.type === "TemplateLiteral" ? "template-literal" : "expression-literal", l.text, 0, `a string inside {…} that a person reads: use a message (m.area_name()) by hand, or one message per branch`);
			}
		}
		if (lettered.length === 0) return;
		const shown = source.slice(first.start, last.end);
		const before = nodes[from - 1];
		const after = nodes[to];
		if (inSentence || neighborIsInline(before) || neighborIsInline(after)) return refuse(first.start, "inline-markup", shown, lettered.length, "the text is a sentence split by inline markup (<b>, <a>, <code>…): a translator needs the whole sentence; use rich-text tags {#link}..{/link} by hand");
		const pieces: Piece[] = members.map((n) => (n.type === "Text" ? { text: str(n.data) } : { expr: asNode(n.expression) as SNode }));
		const built = build(source, pieces);
		if ("reason" in built) return refuse(first.start, built.reason, shown, lettered.length, built.detail);
		const lead = first.type === "Text" ? (/^[ \t\r\n]*/.exec(str(first.raw))?.[0].length ?? 0) : 0;
		const trail = last.type === "Text" ? (/[ \t\r\n]*$/.exec(str(last.raw))?.[0].length ?? 0) : 0;
		accept(first.start + lead, last.end - trail, lineOf(source, first.start + lead), built, { kind: "text", tag, covers: lettered.length }, shown);
	};

	const attribute = (a: SNode, tag: string) => {
		const name = str(a.name);
		const raw = a.value;
		if (raw === true) return;
		const parts = Array.isArray(raw) ? raw : [raw];
		const nodes = parts.map(asNode);
		if (nodes.some((n) => !n)) return;
		const pieces: Piece[] = [];
		let covers = 0;
		for (const n of nodes as SNode[]) {
			if (n.type === "Text") pieces.push({ text: str(n.data) });
			else if (n.type === "ExpressionTag") {
				const e = asNode(n.expression);
				if (!e) return;
				if (!Array.isArray(raw) && e.type === "Literal" && typeof e.value === "string") pieces.push({ text: e.value });
				else if (!Array.isArray(raw) && e.type === "TemplateLiteral") {
					const quasis = (Array.isArray(e.quasis) ? e.quasis : []).map(asNode);
					const exprs = (Array.isArray(e.expressions) ? e.expressions : []).map(asNode);
					quasis.forEach((q, i) => {
						pieces.push({ text: str(asNode(q?.value)?.cooked) });
						const x = exprs[i];
						if (x) pieces.push({ expr: x });
					});
				} else if (!Array.isArray(raw)) {
					for (const l of literalsOf(e).filter(notSuffix)) refuse(l.node.start, l.node.type === "TemplateLiteral" ? "template-literal" : "expression-literal", `${name}={${l.text}}`, 0, "a string inside {…} that a person reads: use a message by hand, or one message per branch");
					return;
				} else pieces.push({ expr: e });
			}
		}
		const allStatic = Array.isArray(raw) && nodes.every((n) => n?.type === "Text");
		if (allStatic) covers = 1;
		if (!pieces.some((p) => "text" in p && LETTERS.test(p.text))) return;
		const shown = compact(source.slice(a.start, a.end), 80);
		const built = build(source, pieces);
		if ("reason" in built) return refuse(a.start, built.reason, shown, covers, built.detail);
		accept(a.start, a.end, lineOf(source, a.start), built, { kind: "attr", attr: name, tag, covers }, shown);
	};

	const visit = (nodes: unknown[], tag: string, inSentence: boolean) => {
		let ignoreNext = false;
		let i = 0;
		while (i < nodes.length) {
			const n = asNode(nodes[i]);
			if (!n) {
				i++;
				continue;
			}
			if (n.type === "Comment") {
				if (/^\s*i18n-ignore\b/.test(str(n.data))) ignoreNext = true;
				i++;
				continue;
			}
			if (isRunNode(n)) {
				let j = i;
				while (j < nodes.length && isRunNode(nodes[j])) j++;
				const onlyWhitespace = nodes.slice(i, j).every((x) => asNode(x)?.type === "Text" && str(asNode(x)?.data).trim() === "");
				if (!onlyWhitespace) {
					const skip = ignoreNext;
					ignoreNext = false;
					if (!skip) run(nodes, i, j, tag, inSentence);
				}
				i = j;
				continue;
			}
			const skip = ignoreNext;
			ignoreNext = false;
			i++;
			if (skip || SKIP_ELEMENTS.has(str(n.name))) continue;
			const owner = str(n.name) || n.type;
			if (Array.isArray(n.attributes)) {
				for (const a of n.attributes) {
					const attr = asNode(a);
					if (attr?.type === "Attribute" && ATTRS.has(str(attr.name))) attribute(attr, owner);
				}
			}
			for (const child of Object.values(n)) {
				const inner = fragmentNodes(child);
				// an inline element among words (`Use <b>bold</b> sparingly`) makes its own text a piece of that sentence
				if (inner) visit(inner, owner, inSentence || (INLINE.has(owner) && nodes.some((x) => x !== n && isRunNode(x) && (asNode(x)?.type === "ExpressionTag" || LETTERS.test(str(asNode(x)?.data))))));
			}
		}
	};
	const top = fragmentNodes(ast.fragment);
	if (top) visit(top, "", false);
	out.candidates.sort((a, b) => a.start - b.start);
	return out;
}

/** The import line a component needs, by surface: desktop wraps Paraglide in `#lib/i18n/m.ts`; the site imports it directly. */
export const importLine = (surface: "desktop" | "site", file: string) => {
	if (surface === "desktop") return 'import { m } from "#lib/i18n/m.ts";';
	const up = file.split("/").slice(2, -1).map(() => "..").join("/");
	return `import { m } from "${up || "."}/lib/paraglide/messages.js";`;
};

/** `source` with every candidate replaced by its message call and the import added. `keyOf` gives each candidate its key. */
export function applySvelte(source: string, a: Analysis, used: { c: Candidate; key: string }[], line: string): string {
	let text = source;
	const edits = used.map(({ c, key }) => {
		const args = c.vars.length ? `{ ${c.vars.map((v) => (v.name === v.expr ? v.name : `${v.name}: ${v.expr}`)).join(", ")} }` : "";
		const call = `m.${key}(${args})`;
		return { start: c.start, end: c.end, text: c.kind === "attr" ? `${c.attr}={${call}}` : `{${call}}` };
	});
	if (!a.hasImport && used.length) {
		if (a.instance?.importEnd !== undefined) edits.push({ start: a.instance.importEnd, end: a.instance.importEnd, text: `\n${a.instance.indent}${line}` });
		else if (a.instance) edits.push({ start: a.instance.contentStart, end: a.instance.contentStart, text: `\n${a.instance.indent}${line}` });
		else edits.push({ start: 0, end: 0, text: `<script${a.lang ? ` lang="${a.lang}"` : ' lang="ts"'}>\n\t${line}\n</script>\n\n` });
	}
	for (const e of edits.sort((x, y) => y.start - x.start || y.end - x.end)) text = text.slice(0, e.start) + e.text + text.slice(e.end);
	return text;
}
