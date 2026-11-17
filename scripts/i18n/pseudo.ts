// bun run i18n:pseudo [--print <area>] [--root <dir>]
// A pseudo-locale, en-XA: every English message accented, padded by 35% and wrapped in ⟦ ⟧, so a screen shows at a
// glance what is not extracted (plain text), what truncates (the closing bracket is missing) and what is glued
// together from pieces (brackets in the middle of a sentence). Variables, markup, code spans, URLs, selectors and
// keys are kept byte for byte; every plural and `os` variant is transformed, so each platform and each form shows it.
//
//   (no args)       writes <app>/messages/en-XA.json next to the English catalog of each Paraglide project. The
//                   file is gitignored and not a locale of the project (it is not in settings.json), so no check
//                   reads it; list en-XA in a dev build's settings to see it.
//   --print <area>  prints the pseudo-localized messages of one Fluent area (crates/*/locales/en/<area>.ftl) to
//                   stdout. The Rust catalogs are embedded in the binary, so nothing is written for them.
import { writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { FluentParser, FluentSerializer, Message, type Pattern, Resource, TextElement } from "@fluent/syntax";
import { ftlFiles } from "./formats/fluent.ts";
import { loadParaglide } from "./formats/paraglide.ts";
import type { ParaglideValue } from "./shape.ts";
import { loadCtx } from "./types.ts";
import { canonicalJson, detectIndent, readText } from "./util.ts";

export const PSEUDO_LOCALE = "en-XA";
const OPEN = "⟦";
const CLOSE = "⟧";
const PAD = "~";
const ACCENTS: Record<string, string> = { a: "á", e: "é", i: "í", o: "ó", u: "ú", c: "ç", n: "ñ", y: "ý", A: "Å", E: "É", I: "Í", O: "Ó", U: "Ú", C: "Ç", N: "Ñ", Y: "Ý" };

/** What is kept verbatim inside a text: Paraglide placeables and markup, escaped braces, code spans, URLs. */
const KEEP = /\\[{}]|\{[^{}]*\}|`[^`\n]*`|https?:\/\/[^\s)>\]|`]+/g;

const accent = (s: string) => s.replace(/[aeiouncyAEIOUNCY]/g, (c) => ACCENTS[c]);
const letters = (s: string) => (s.match(/\p{L}/gu) ?? []).length;

/** Accent the prose of `s`, leaving the kept spans alone; returns the text and how many letters it has. */
function accentProse(s: string): { text: string; letters: number } {
	let out = "";
	let last = 0;
	let n = 0;
	for (const m of s.matchAll(KEEP)) {
		const prose = s.slice(last, m.index);
		out += accent(prose) + m[0];
		n += letters(prose);
		last = (m.index ?? 0) + m[0].length;
	}
	const tail = s.slice(last);
	return { text: out + accent(tail), letters: n + letters(tail) };
}

const padding = (n: number) => (n > 0 ? ` ${PAD.repeat(Math.ceil(n * 0.35))}` : "");

/** One message text: accented, padded by 35% of its letters, wrapped. Already wrapped text is returned unchanged. */
export function pseudoText(s: string): string {
	if (s.startsWith(OPEN) && s.endsWith(CLOSE)) return s;
	const p = accentProse(s);
	return `${OPEN}${p.text}${padding(p.letters)}${CLOSE}`;
}

/** A Paraglide catalog value: a string, or variants whose every match arm is transformed (keys and selectors kept). */
export function pseudoValue(v: ParaglideValue): ParaglideValue {
	if (typeof v === "string") return pseudoText(v);
	return v.map((variant) => ({ ...variant, match: Object.fromEntries(Object.entries(variant.match).map(([k, t]) => [k, pseudoText(t)])) }));
}

export function pseudoCatalog(en: Record<string, ParaglideValue>): Record<string, ParaglideValue> {
	return Object.fromEntries(Object.entries(en).map(([k, v]) => [k, k === "$schema" ? v : pseudoValue(v)]));
}

/** Pseudo-localize the patterns of a Fluent message in place: text elements accented, variables and selectors kept,
 * every select variant transformed on its own. A pattern that is only a select is not wrapped itself. */
function pseudoPattern(p: Pattern | null) {
	if (!p) return;
	let n = 0;
	for (const el of p.elements) {
		if (el.type === "TextElement") {
			const t = accentProse(el.value);
			el.value = t.text;
			n += t.letters;
		} else if (el.expression.type === "SelectExpression") {
			for (const v of el.expression.variants) pseudoPattern(v.value);
		}
	}
	const onlySelect = p.elements.length === 1 && p.elements[0].type === "Placeable";
	if (onlySelect) return;
	const first = p.elements[0];
	if (first?.type === "TextElement" && first.value.startsWith(OPEN)) return;
	p.elements.unshift(new TextElement(OPEN));
	p.elements.push(new TextElement(`${padding(n)}${CLOSE}`));
}

/** The messages of an `.ftl` source, pseudo-localized and serialized (ids, variables and selectors untouched). */
export function pseudoFluent(source: string): string {
	const parsed = new FluentParser().parse(source);
	const messages = parsed.body.filter((e): e is Message => e.type === "Message");
	for (const m of messages) {
		pseudoPattern(m.value);
		for (const a of m.attributes) pseudoPattern(a.value);
		m.comment = null;
	}
	return new FluentSerializer().serialize(new Resource(messages));
}

/** Write en-XA.json next to the English catalog of every Paraglide project; the paths written. */
export function writePseudoCatalogs(root: string): string[] {
	const written: string[] = [];
	for (const p of loadParaglide(loadCtx(root))) {
		const en = p.catalogs.get(p.surface.base);
		if (!en) continue;
		const text = readText(en);
		const out = join(dirname(en), `${PSEUDO_LOCALE}.json`);
		writeFileSync(out, canonicalJson(pseudoCatalog(JSON.parse(text)), detectIndent(text)));
		written.push(out);
	}
	return written;
}

/** The pseudo-localized messages of one Fluent area, over every crate that has it. */
export function printFluentArea(root: string, area: string): string {
	const ctx = loadCtx(root);
	const files = ftlFiles(root).filter((f) => f.locale === ctx.locales.default);
	const mine = files.filter((f) => basename(f.path, ".ftl") === area);
	if (mine.length === 0) throw new Error(`no area "${area}" (areas: ${[...new Set(files.map((f) => basename(f.path, ".ftl")))].join(", ") || "none"})`);
	return mine.map((f) => pseudoFluent(readText(f.path))).join("\n");
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const opt = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const root = opt("--root") ?? join(import.meta.dir, "..", "..");
	try {
		const area = opt("--print");
		if (args.includes("--print") && !area) throw new Error("usage: bun run i18n:pseudo [--print <area>]");
		if (area) process.stdout.write(printFluentArea(root, area));
		else {
			for (const f of writePseudoCatalogs(root)) console.log(`wrote ${f.slice(root.length + 1)}`);
			console.log("Rust catalogs are embedded: bun run i18n:pseudo -- --print <area> prints one area");
		}
	} catch (e) {
		console.error(`i18n:pseudo: ${(e as Error).message}`);
		process.exit(1);
	}
}
