// i18n/glossary.yaml and the deterministic text rules built on it: do-not-translate terms, fixed renderings,
// and the pt-PT words that must not leak into pt-BR.
import { existsSync } from "node:fs";
import { join } from "node:path";
import { readText } from "./util.ts";

export const GLOSSARY_FILE = "i18n/glossary.yaml";

export const OS_KEYS = ["mac", "windows", "linux"] as const;
export type OsForms = Record<(typeof OS_KEYS)[number], string>;

export interface GlossaryEntry {
	term: string;
	/** Keep the term exactly as written in every locale. */
	dnt?: boolean;
	note?: string;
	/** Per locale: the noun an OS-specific concept takes on each platform (`en: { mac: your Mac, windows: this PC, linux: ... }`).
	 * Messages select on `os` (a selector `os` in the variants, `mac`|`windows`|`*`); the lint forbids the nouns outside such a select. */
	os?: Record<string, OsForms>;
	/** Per locale: grammatical gender of a kept term (`pt-BR: f` = "a branch"). Shown to the translator, not enforced. */
	gender?: Record<string, string>;
	/** Per locale: a decision about wording that cannot be matched mechanically. Shown to the translator, not enforced. */
	advice?: Record<string, string>;
	/** `<locale>: rendering` pairs (`pt-BR: Configurações`). */
	[locale: string]: unknown;
}

const STRUCTURED = new Set(["term", "dnt", "note", "os", "gender", "advice"]);

export interface Glossary {
	entries: GlossaryEntry[];
}

export function parseGlossary(text: string, file = GLOSSARY_FILE): Glossary {
	const doc = Bun.YAML.parse(text) as unknown;
	if (doc === null || doc === undefined) return { entries: [] };
	if (!Array.isArray(doc)) throw new Error(`${file}: expected a list of { term, dnt | <locale> } entries`);
	const seen = new Set<string>();
	for (const [i, e] of doc.entries()) {
		const at = `${file}: entry ${i + 1}`;
		if (typeof e !== "object" || e === null || typeof e.term !== "string" || e.term.trim() === "") {
			throw new Error(`${at} needs a non-empty \`term\``);
		}
		const key = e.term.toLowerCase();
		if (seen.has(key)) throw new Error(`${at}: duplicate term "${e.term}"`);
		seen.add(key);
		const renderings = Object.keys(e).filter((k) => !STRUCTURED.has(k));
		if (e.dnt !== true && renderings.length === 0 && !e.os && !e.advice) {
			throw new Error(`${at} ("${e.term}") needs \`dnt: true\`, a rendering per locale, \`os\` or \`advice\``);
		}
		for (const [loc, forms] of Object.entries(e.os ?? {})) {
			if (typeof forms !== "object" || forms === null || OS_KEYS.some((k) => typeof forms[k] !== "string")) {
				throw new Error(`${at} ("${e.term}"): os.${loc} needs a string for each of ${OS_KEYS.join(", ")}`);
			}
		}
		for (const k of renderings) if (typeof e[k] !== "string" || e[k].trim() === "") throw new Error(`${at} ("${e.term}"): ${k} must be a non-empty string`);
	}
	return { entries: doc as GlossaryEntry[] };
}

export function loadGlossary(root: string): Glossary {
	const p = join(root, GLOSSARY_FILE);
	return existsSync(p) ? parseGlossary(readText(p)) : { entries: [] };
}

/** The OS forms of `locale` for an entry, as `mac | windows | linux` for a prompt line. */
export const osLine = (forms: OsForms) => OS_KEYS.map((k) => `${k}: ${forms[k]}`).join(" | ");

export const renderingFor = (e: GlossaryEntry, locale: string): string | undefined =>
	typeof e[locale] === "string" ? (e[locale] as string) : undefined;

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const WORD = "[\\p{L}\\p{N}_]";

/** A term as a whole word, with an optional plural `s`/`es`; a term with a capital is case-sensitive (`Crew` is the
 * feature, `crew` is a noun), an all-lowercase one is not (`Commit` at the start of a button). */
export function termRegex(term: string): RegExp {
	const body = term.trim().split(/\s+/).map(escapeRe).join("\\s+");
	return new RegExp(`(?<!${WORD})${body}(?:e?s)?(?!${WORD})`, term === term.toLowerCase() ? "iu" : "u");
}

export interface TextIssue {
	rule: "dnt" | "glossary" | "pt-pt" | "pt-time";
	severity: "error" | "warn";
	message: string;
}

/** `i18n-allow <rule>:<token>` in a key's description waives one hit (a homograph such as the verb "log in"). */
export function allowed(description: string | undefined, rule: string, token: string): boolean {
	if (!description) return false;
	const t = token.toLowerCase();
	return [...description.matchAll(/i18n-allow\s+([\w-]+):([^\s,;]+)/g)].some((m) => m[1] === rule && m[2].toLowerCase() === t);
}

/** pt-PT forms and the pt-BR word to use instead. Each pattern is a whole word. */
const PT_PT: [RegExp, string][] = [
	[/ecrã(?:s)?/iu, "tela"],
	[/utilizador(?:es)?/iu, "usuário"],
	[/ficheiros?/iu, "arquivo"],
	[/definições/iu, "configurações"],
	[/registos?/iu, "registro"],
	[/partilh(?:ar|a|ado|ada|ados|adas|ando)/iu, "compartilhar"],
	[/palavras?-passe/iu, "senha"],
	[/aced(?:er|ido|endo)/iu, "acessar"],
	[/guard(?:ar|ado|ada|ados|adas)/iu, "salvar"],
	[/elimin(?:ar|ado|ada|ados|adas)/iu, "excluir"],
	[/ratos?/iu, "mouse"],
	[/transferir/iu, "baixar"],
	// "está a fazer": the pt-PT progressive; pt-BR says "está fazendo" (or the plain verb).
	[/(?:estou|está|estás|estamos|estão|estava|estavam) a \p{L}+(?:ar|er|ir)/iu, "gerúndio (está fazendo) ou o verbo simples"],
];

/** pt-PT words in pt-BR text. */
export function ptPtHits(text: string): { word: string; use: string }[] {
	const hits: { word: string; use: string }[] = [];
	for (const [re, use] of PT_PT) {
		const m = new RegExp(`(?<!${WORD})(?:${re.source})(?!${WORD})`, "iu").exec(text);
		if (m) hits.push({ word: m[0], use });
	}
	return hits;
}

/** A preposition glued to a time placeholder: "às {time}", "à {time}", "das {hora}". Which one is right depends on the
 * hour (à 01:00, às 13:00), and the code passes a formatted string, so no pt-BR message may put one before a time. */
const PT_TIME = /(?<![\p{L}])(?:às|à|das|da)\s+\{[^{}]*(?:time|hora|hour|clock)[^{}]*\}/iu;

/** Text rules for one translated key: `source` and `target` are the human-readable texts (all variants). */
export function textIssues(opts: {
	glossary: Glossary;
	locale: string;
	source: string;
	target: string;
	description?: string;
}): TextIssue[] {
	const { glossary, locale, source, target, description } = opts;
	const out: TextIssue[] = [];
	for (const e of glossary.entries) {
		const re = termRegex(e.term);
		if (!re.test(source)) continue;
		if (e.dnt) {
			if (!re.test(target) && !allowed(description, "dnt", e.term)) {
				out.push({ rule: "dnt", severity: "error", message: `"${e.term}" is never translated: keep it verbatim (waive a homograph with \`i18n-allow dnt:${e.term}\` in the description)` });
			}
			continue;
		}
		const r = renderingFor(e, locale);
		if (r && !target.toLowerCase().includes(r.toLowerCase()) && !allowed(description, "glossary", e.term)) {
			out.push({ rule: "glossary", severity: "warn", message: `"${e.term}" is "${r}" in the glossary (waive with \`i18n-allow glossary:${e.term}\`)` });
		}
	}
	if (locale === "pt-BR") {
		const time = PT_TIME.exec(target);
		if (time) {
			out.push({ rule: "pt-time", severity: "error", message: `"${time[0]}": à 01:00 but às 13:00, so a preposition before a time is wrong for one hour of the day; write "reinicia: {time}" or "hoje, {time}" (i18n/style/pt-BR.md, "Horas")` });
		}
		for (const h of ptPtHits(target)) {
			if (!allowed(description, "pt-pt", h.word)) {
				out.push({ rule: "pt-pt", severity: "error", message: `"${h.word}" is pt-PT; pt-BR says ${h.use} (waive with \`i18n-allow pt-pt:${h.word.toLowerCase()}\`)` });
			}
		}
	}
	return out;
}
