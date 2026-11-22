// The structure of one message: the variables it interpolates, its markup tags, its code spans and URLs, and its
// plural/select selectors. Two shapes are compared to catch a translation that dropped `{count}`, lost a plural form
// or rewrote a URL. Shapes are taken from the RESOLVED message (references inlined), so a translation may write its
// own `os` select where the English references `{@common_os_trash}`.
import { type MsgValue, type Variant, normalize, parseMatchKey, selectorInfo } from "./resolve.ts";

export type { Variant };
export type ParaglideValue = MsgValue;

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

/** Variables, markup, code spans and URLs of one message text (`Hello {name}`, `{#link}..{/link}`). References
 * (`{@key}`) are not variables. */
function scanText(text: string, into: Scan) {
	const plain = text.replace(/\\[{}]/g, "");
	for (const m of plain.matchAll(/\{([^{}]*)\}/g)) {
		const inner = m[1].trim();
		if (inner.startsWith("@")) continue;
		if (inner.startsWith("#")) into.markup.add(`#${inner.slice(1).split(/[\s/]/)[0]}`);
		else if (inner.startsWith("/")) into.markup.add(`/${inner.slice(1).split(/\s/)[0]}`);
		else if (inner) into.vars.add(inner.replace(/^\$/, "").split(/[\s:]/)[0]);
	}
	const prose = plain.replace(/\{[^{}]*\}/g, " ");
	for (const m of prose.matchAll(/`([^`]+)`/g)) into.code.push(m[1]);
	for (const m of prose.matchAll(/https?:\/\/[^\s)>\]|`]+/g)) into.urls.push(m[0]);
}

const sorted = (xs: Iterable<string>) => [...xs].sort();

/** What is wrong with the structure of a catalog value, or undefined when it is a string or one well-formed variants object. */
export function paraglideProblem(v: unknown): string | undefined {
	if (typeof v === "string") return undefined;
	if (!Array.isArray(v) || v.length === 0) return "value must be a string or a variants array";
	if (v.length > 1) return "a message has exactly one variants object";
	for (const variant of v) {
		if (typeof variant !== "object" || variant === null || typeof variant.match !== "object" || variant.match === null) {
			return "a variant needs a `match` object";
		}
		if (Object.values(variant.match).some((t) => typeof t !== "string")) return "every `match` value must be a string";
		if (!Array.isArray(variant.selectors) || variant.selectors.length === 0) return "a variants object needs its `selectors`";
	}
	return undefined;
}

/** Every human-readable text of a value (the string, or each match arm). */
export const paraglideTexts = (v: MsgValue): string[] => (typeof v === "string" ? [v] : v.flatMap((x) => Object.values(x.match)));

export function paraglideShape(v: MsgValue): Shape {
	const scan: Scan = { vars: new Set(), markup: new Set(), code: [], urls: [] };
	const selectors: SelectorShape[] = [];
	if (typeof v === "string") scanText(v, scan);
	else {
		const msg = normalize(v);
		for (const arm of msg.arms) scanText(arm.text, scan);
		for (const d of msg.declarations) {
			const input = /^input\s+\{?\$?(\w+)/.exec(d.trim());
			if (input) scan.vars.add(input[1]);
			const local = /^local\s+\$?\w+\s*=\s*\{?\s*\$?(\w+)/.exec(d.trim());
			if (local) scan.vars.add(local[1]);
		}
		for (const name of msg.selectors) {
			const info = selectorInfo(msg, name);
			const values = new Set<string>();
			for (const key of Object.keys(v[0].match)) {
				const literal = key.trim() === "*" ? "*" : parseMatchKey(key)[name];
				if (literal !== undefined) values.add(info.kind === "plural" && literal === "*" ? "other" : literal);
			}
			selectors.push({ kind: info.kind, name: info.var, values: sorted(values) });
		}
	}
	return { vars: sorted(scan.vars), markup: sorted(scan.markup), attrs: [], code: scan.code.sort(), urls: scan.urls.sort(), selectors };
}

export interface ShapeIssue {
	rule: "placeholder" | "verbatim" | "plural";
	message: string;
}

const diff = (a: string[], b: string[]) => a.filter((x) => !b.includes(x));

const FIX_ZERO = `use a second selector: "local countExact = count", match "countExact=0, countPlural=*"`;

/** A plural selector matches CLDR categories only (the Rust evaluator compares the category name), so an arm keyed by a number never fires there. */
const numberArms = (s: SelectorShape) => (s.kind === "plural" ? s.values.filter((v) => /^\d+$/.test(v)) : []);

const numberArmIssue = (s: SelectorShape, i: number): ShapeIssue[] =>
	numberArms(s).length ? [{ rule: "plural", message: `plural selector ${i + 1} has an arm keyed by the number ${numberArms(s).join(", ")}: only categories (one, other) match there, an exact number needs another selector (${FIX_ZERO})` }] : [];

/** Selectors a translation may add to the source's: an exact number (0) on a count the source already pluralizes. The English
 * says "0 files" fine; pt-BR must say "Nenhum arquivo", so only its message carries `countExact`. */
function withoutExactExtras(source: Shape, target: Shape): SelectorShape[] {
	const counts = new Set(source.selectors.filter((s) => s.kind === "plural").map((s) => s.name));
	const has = (name?: string) => source.selectors.some((s) => s.kind === "select" && s.name === name);
	return target.selectors.filter((s) => !(s.kind === "select" && counts.has(s.name) && !has(s.name) && s.values.every((v) => v === "*" || /^\d+$/.test(v))));
}

/** Problems of the source's own selectors: a plural without `one` and `other` cannot be translated correctly. */
export function sourceIssues(source: Shape, locale: string): ShapeIssue[] {
	return source.selectors.flatMap((s, i): ShapeIssue[] => [
		...(s.kind === "plural" && diff(requiredPlurals(locale), s.values).length
			? [{ rule: "plural" as const, message: `plural selector ${i + 1} needs ${requiredPlurals(locale).join(" + ")} (has ${s.values.join(", ") || "none"})` }]
			: []),
		...numberArmIssue(s, i),
	]);
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
	names("code span", source.code, target.code, "verbatim", (x) => `\`${x}\``);
	names("URL", source.urls, target.urls, "verbatim");
	target.selectors.forEach((t, i) => out.push(...numberArmIssue(t, i)));
	const targetSelectors = withoutExactExtras(source, target);
	if (source.selectors.length !== targetSelectors.length) {
		out.push({ rule: "plural", message: `source has ${source.selectors.length} plural/select selector(s), the translation ${targetSelectors.length}` });
		return out;
	}
	source.selectors.forEach((s, i) => {
		const t = targetSelectors[i];
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
