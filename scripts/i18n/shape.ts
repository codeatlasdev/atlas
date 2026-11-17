// The structure of one message, independent of its file format: the variables it interpolates, its markup tags,
// its code spans and URLs, and its plural/select selectors. Two shapes are compared to catch a translation that
// dropped `{count}`, lost a plural form or rewrote a URL. Paraglide (inlang message-format) and Fluent both
// reduce to the same shape, so one set of rules covers both.
import type { Expression, Message, Pattern, PatternElement, SelectExpression } from "@fluent/syntax";

export const PLURAL_CATEGORIES = ["zero", "one", "two", "few", "many", "other"];

export interface SelectorShape {
	/** `plural` selectors need the target locale's categories; `select` ones must match the source's values. */
	kind: "plural" | "select";
	values: string[];
	/** The variable the selector matches on (`count`, `os`); the `os` selector is what allows platform nouns. */
	name?: string;
}

export interface Shape {
	vars: string[];
	markup: string[];
	attrs: string[];
	code: string[];
	urls: string[];
	selectors: SelectorShape[];
}

/** Plural categories a locale's translation must provide (CLDR; a 0 or 1 is `one` in pt-BR, `many` falls to `other`). */
export function requiredPlurals(locale: string): string[] {
	return locale === "en" || locale.startsWith("pt") ? ["one", "other"] : ["other"];
}

interface Scan {
	vars: Set<string>;
	markup: Set<string>;
	code: string[];
	urls: string[];
}

const newScan = (): Scan => ({ vars: new Set(), markup: new Set(), code: [], urls: [] });

/** Variables, markup, code spans and URLs of one paraglide message text (`Hello {name}`, `{#link}..{/link}`). */
function scanParaglide(text: string, into: Scan) {
	const plain = text.replace(/\\[{}]/g, "");
	for (const m of plain.matchAll(/\{([^{}]*)\}/g)) {
		const inner = m[1].trim();
		if (inner.startsWith("#")) into.markup.add(`#${inner.slice(1).split(/[\s/]/)[0]}`);
		else if (inner.startsWith("/")) into.markup.add(`/${inner.slice(1).split(/\s/)[0]}`);
		else if (inner) into.vars.add(inner.replace(/^\$/, "").split(/[\s:]/)[0]);
	}
	scanLiteral(plain.replace(/\{[^{}]*\}/g, " "), into);
}

function scanLiteral(text: string, into: Scan) {
	for (const m of text.matchAll(/`([^`]+)`/g)) into.code.push(m[1]);
	for (const m of text.matchAll(/https?:\/\/[^\s)>\]|`]+/g)) into.urls.push(m[0]);
}

const sorted = (xs: Iterable<string>) => [...xs].sort();

export interface Variant {
	declarations?: string[];
	selectors?: string[];
	match: Record<string, string>;
}

export type ParaglideValue = string | Variant[];

/** What is wrong with the structure of a catalog value, or undefined when it is a string or well-formed variants. */
export function paraglideProblem(v: unknown): string | undefined {
	if (typeof v === "string") return undefined;
	if (!Array.isArray(v) || v.length === 0) return "value must be a string or a non-empty variants array";
	for (const variant of v) {
		if (typeof variant !== "object" || variant === null || typeof variant.match !== "object" || variant.match === null) {
			return "a variant needs a `match` object";
		}
		if (Object.values(variant.match).some((t) => typeof t !== "string")) return "every `match` value must be a string";
	}
	return undefined;
}

/** Every human-readable text of a paraglide value (the string, or each match arm). */
export function paraglideTexts(v: ParaglideValue): string[] {
	return typeof v === "string" ? [v] : v.flatMap((x) => Object.values(x.match));
}

export function paraglideShape(v: ParaglideValue): Shape {
	const scan = newScan();
	const selectors: SelectorShape[] = [];
	if (typeof v === "string") {
		scanParaglide(v, scan);
	} else {
		for (const variant of v) {
			for (const t of Object.values(variant.match)) scanParaglide(t, scan);
			for (const d of variant.declarations ?? []) {
				const input = /^input\s+\{?\$?(\w+)/.exec(d.trim());
				if (input) scan.vars.add(input[1]);
				const local = /^local\s+\$?\w+\s*=\s*\{?\s*\$?(\w+)/.exec(d.trim());
				if (local) scan.vars.add(local[1]);
			}
			const names = variant.selectors ?? [];
			const plural = new Set<string>();
			for (const d of variant.declarations ?? []) {
				const m = /^local\s+\$?(\w+)\s*=\s*\{?[^:]*:\s*plural\b(.*?)\}?$/.exec(d.trim());
				if (m && !/type\s*=\s*\|?ordinal/.test(m[2])) plural.add(m[1]);
			}
			names.forEach((name, i) => {
				const values = new Set<string>();
				for (const k of Object.keys(variant.match)) {
					const part = k.trim() === "*" ? "*" : new RegExp(`(?:^|\\s)\\$?${name}=([^\\s]+)`).exec(k)?.[1];
					if (part !== undefined) values.add(part);
				}
				const kind = plural.has(name) ? "plural" : "select";
				const norm = kind === "plural" ? [...values].map((x) => (x === "*" ? "other" : x)) : [...values];
				const aliasOf = (variant.declarations ?? []).map((d) => new RegExp(`^local\\s+\\$?${name}\\s*=\\s*\\{?\\s*\\$?(\\w+)`).exec(d.trim())?.[1]).find(Boolean);
				selectors[i] = { kind, name: aliasOf ?? name, values: sorted(new Set([...(selectors[i]?.values ?? []), ...norm])) };
			});
		}
	}
	return finish(scan, [], selectors);
}

function finish(scan: Scan, attrs: string[], selectors: SelectorShape[]): Shape {
	return { vars: sorted(scan.vars), markup: sorted(scan.markup), attrs: sorted(attrs), code: scan.code.sort(), urls: scan.urls.sort(), selectors };
}

/** The plain text of a Fluent pattern (placeables dropped), one string per select arm. */
function fluentPatternTexts(p: Pattern | null, out: string[] = []): string[] {
	if (!p) return out;
	let cur = "";
	const flush = () => {
		if (cur.trim()) out.push(cur);
		cur = "";
	};
	for (const el of p.elements) {
		if (el.type === "TextElement") cur += el.value;
		else if (el.expression.type === "SelectExpression") {
			flush();
			for (const v of el.expression.variants) fluentPatternTexts(v.value, out);
		} else cur += " ";
	}
	flush();
	return out;
}

export function fluentTexts(m: Message): string[] {
	return [m.value, ...m.attributes.map((a) => a.value)].flatMap((p) => fluentPatternTexts(p));
}

function visitExpression(e: Expression, scan: Scan, selectors: SelectorShape[]) {
	switch (e.type) {
		case "VariableReference":
			scan.vars.add(e.id.name);
			break;
		case "FunctionReference":
			for (const a of e.arguments.positional) visitExpression(a, scan, selectors);
			for (const a of e.arguments.named) visitExpression(a.value, scan, selectors);
			break;
		case "Placeable":
			visitExpression(e.expression, scan, selectors);
			break;
		case "SelectExpression":
			visitSelect(e, scan, selectors);
			break;
		default:
			break;
	}
}

function visitSelect(e: SelectExpression, scan: Scan, selectors: SelectorShape[]) {
	visitExpression(e.selector, scan, selectors);
	const names = e.variants.map((v) => (v.key.type === "Identifier" ? v.key.name : v.key.value));
	// A select whose only named arm is the default `*[other]` is a plain select (`[mac] ... *[other] ...`), not a plural.
	const plural = e.variants.some((v) => v.key.type === "Identifier" && v.key.name !== "other" && PLURAL_CATEGORIES.includes(v.key.name));
	const name = e.selector.type === "VariableReference" ? e.selector.id.name : undefined;
	selectors.push({ kind: plural ? "plural" : "select", name, values: plural ? sorted(names.filter((n) => PLURAL_CATEGORIES.includes(n))) : sorted(names) });
	for (const v of e.variants) visitPattern(v.value, scan, selectors);
}

function visitPattern(p: Pattern | null, scan: Scan, selectors: SelectorShape[]) {
	if (!p) return;
	for (const el of p.elements as PatternElement[]) {
		if (el.type === "TextElement") scanLiteral(el.value, scan);
		else visitExpression(el.expression, scan, selectors);
	}
}

export function fluentShape(m: Message): Shape {
	const scan = newScan();
	const selectors: SelectorShape[] = [];
	visitPattern(m.value, scan, selectors);
	for (const a of m.attributes) visitPattern(a.value, scan, selectors);
	return finish(scan, m.attributes.map((a) => a.id.name), selectors);
}

export interface ShapeIssue {
	rule: "placeholder" | "verbatim" | "plural";
	message: string;
}

const diff = (a: string[], b: string[]) => a.filter((x) => !b.includes(x));

/** Problems of the source's own selectors: a plural without `one` and `other` cannot be translated correctly. */
export function sourceIssues(source: Shape, locale: string): ShapeIssue[] {
	return source.selectors.flatMap((s, i): ShapeIssue[] =>
		s.kind === "plural" && diff(requiredPlurals(locale), s.values).length
			? [{ rule: "plural", message: `plural selector ${i + 1} needs ${requiredPlurals(locale).join(" + ")} (has ${s.values.join(", ") || "none"})` }]
			: [],
	);
}

/** Everything a translation must keep from its source, and the plural forms its locale needs. */
export function compareShapes(source: Shape, target: Shape, locale: string): ShapeIssue[] {
	const out: ShapeIssue[] = [];
	const names = (label: string, a: string[], b: string[], rule: ShapeIssue["rule"], fmt = (x: string) => x) => {
		for (const x of diff(a, b)) out.push({ rule, message: `${label} ${fmt(x)} is missing in the translation` });
		for (const x of diff(b, a)) out.push({ rule, message: `${label} ${fmt(x)} is not in the source` });
	};
	names("variable", source.vars, target.vars, "placeholder", (x) => `{${x}}`);
	names("markup", source.markup, target.markup, "placeholder", (x) => `{${x}}`);
	names("attribute", source.attrs, target.attrs, "placeholder", (x) => `.${x}`);
	names("code span", source.code, target.code, "verbatim", (x) => `\`${x}\``);
	names("URL", source.urls, target.urls, "verbatim");
	if (source.selectors.length !== target.selectors.length) {
		out.push({ rule: "plural", message: `source has ${source.selectors.length} plural/select selector(s), the translation ${target.selectors.length}` });
		return out;
	}
	source.selectors.forEach((s, i) => {
		const t = target.selectors[i];
		if (t.kind !== s.kind) {
			out.push({ rule: "plural", message: `selector ${i + 1} is a ${s.kind} in the source and a ${t.kind} in the translation` });
		} else if (s.kind === "plural") {
			const missing = diff(requiredPlurals(locale), t.values);
			if (missing.length) out.push({ rule: "plural", message: `plural selector ${i + 1} needs ${requiredPlurals(locale).join(" + ")} for ${locale}; missing ${missing.join(", ")}` });
		} else {
			const missing = diff(s.values, t.values);
			const extra = diff(t.values, s.values);
			if (missing.length || extra.length) out.push({ rule: "plural", message: `select selector ${i + 1} values differ: missing [${missing}] extra [${extra}]` });
		}
	});
	return out;
}
