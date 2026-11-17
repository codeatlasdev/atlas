// bun run i18n:pseudo [--print <area>] [--root <dir>]
// A pseudo-locale, en-XA: every English message accented, padded by 35% and wrapped in ⟦ ⟧, so a screen shows at a
// glance what is not extracted (plain text), what truncates (the closing bracket is missing) and what is glued
// together from pieces (brackets in the middle of a sentence). Variables, markup, code spans, URLs, selectors and
// keys are kept byte for byte; every plural and `os` variant is transformed, so each platform and each form shows it.
//
//   (no args)       writes <paraglide dir>/en-XA.json (apps/desktop/.i18n, site/.i18n) from the generated English
//                   input. It is gitignored and not a locale of the project (it is not in settings.json), so nothing
//                   reads it until a dev build lists en-XA.
//   --print <area>  prints the pseudo-localized messages of one area as JSON to stdout (references kept, as Rust
//                   evaluates them). The Rust catalogs are embedded in the binary, so nothing is written for them.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { outputsFor } from "./generate.ts";
import type { ParaglideValue } from "./shape.ts";
import { loadSource } from "./source.ts";
import { loadCtx } from "./types.ts";
import { canonicalJson, detectIndent } from "./util.ts";

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

/** Write en-XA.json next to the generated English input of every Paraglide surface; the paths written. */
export function writePseudoCatalogs(root: string): string[] {
	const ctx = loadCtx(root);
	const source = loadSource(ctx);
	const written: string[] = [];
	for (const [name, def] of Object.entries(source.config.surfaces)) {
		if (!def.paraglide) continue;
		const en = outputsFor(source, name).find((o) => o.path === `${def.paraglide}/${ctx.locales.default}.json`);
		if (!en) continue;
		const out = join(root, def.paraglide, `${PSEUDO_LOCALE}.json`);
		mkdirSync(dirname(out), { recursive: true });
		writeFileSync(out, canonicalJson(pseudoCatalog(JSON.parse(en.content)), detectIndent(en.content)));
		written.push(out);
	}
	return written;
}

/** The pseudo-localized messages of one area as JSON (what the embedded Rust catalogs read), to stdout. */
export function printArea(root: string, areaName: string): string {
	const source = loadSource(loadCtx(root));
	const area = source.areas.get(areaName);
	if (!area) throw new Error(`no area "${areaName}" (areas: ${[...source.areas.keys()].join(", ") || "none"})`);
	const messages = Object.fromEntries([...(area.entries.get(area.base)?.values() ?? [])].map((e) => [e.key, pseudoValue(e.value)]));
	return canonicalJson(messages, "\t");
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const opt = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const root = opt("--root") ?? join(import.meta.dir, "..", "..");
	try {
		const area = opt("--print");
		if (args.includes("--print") && !area) throw new Error("usage: bun run i18n:pseudo [--print <area>]");
		if (area) process.stdout.write(printArea(root, area));
		else {
			for (const f of writePseudoCatalogs(root)) console.log(`wrote ${f.slice(root.length + 1)}`);
			console.log("Rust catalogs are embedded: bun run i18n:pseudo -- --print <area> prints one area as JSON");
		}
	} catch (e) {
		console.error(`i18n:pseudo: ${(e as Error).message}`);
		process.exit(1);
	}
}
