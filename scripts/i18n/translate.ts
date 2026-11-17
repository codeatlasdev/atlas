// bun run i18n:translate [--locale pt-BR] [--surface desktop] [--dry-run]
// Translates only what is new or changed: keys the locale lacks, and keys whose English hash differs from the lock.
// Bring your own model: ANTHROPIC_API_KEY (+ I18N_MODEL), or any OpenAI-compatible endpoint with I18N_BASE_URL
// (+ I18N_API_KEY, I18N_MODEL; Ollama works). The model output is validated with the same rules `i18n:check` runs
// (placeholders, plural forms, glossary, pt-PT words) and nothing is written for a surface+locale if any key fails.
// --dry-run prints the prompts and touches neither the network nor a file.
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { FluentParser } from "@fluent/syntax";
import { type Entry, entryIssues, hashOf, type Surface } from "./engine.ts";
import { rebuildLock } from "./fix.ts";
import { entryOf, parseMessage } from "./formats/fluent.ts";
import { formatProblem, paraglideEntry } from "./formats/paraglide.ts";
import { OS_KEYS, osLine, termRegex } from "./glossary.ts";
import { readLocks } from "./lock.ts";
import { type ParaglideValue, paraglideProblem } from "./shape.ts";
import { loadSurfaces, type Loaded } from "./surfaces.ts";
import { type Ctx, loadCtx } from "./types.ts";
import { canonicalJson, detectIndent, readText } from "./util.ts";

const BATCH = 40;
const EXAMPLES = 20;

export interface Pending {
	key: string;
	base: Entry;
	/** The outdated translation, shown to the model as the previous version; undefined for a missing key. */
	previous?: Entry;
}

/** Keys of `surface` that `locale` lacks or whose English changed since they were translated (not merely unstamped). */
export function pending(ctx: Ctx, surface: Surface, locale: string): Pending[] {
	const base = surface.entries.get(surface.base) ?? new Map<string, Entry>();
	const mine = surface.entries.get(locale) ?? new Map<string, Entry>();
	const lock = readLocks(ctx.root, surface.name, locale).byKey;
	const out: Pending[] = [];
	for (const [key, b] of base) {
		const t = mine.get(key);
		const stamp = lock.get(key)?.hash;
		if (!t) out.push({ key, base: b });
		else if (stamp && stamp !== hashOf(b)) out.push({ key, base: b, previous: t });
	}
	return out;
}

const SYSTEM = (locale: string, kind: Surface["kind"]) =>
	[
		`You translate user interface text of the Atlas developer tool from English into ${locale}.`,
		"Follow the glossary and the style guide exactly. Keep every {placeholder}, markup tag, `code span`, URL and plural/select structure; translate nothing inside them.",
		"Use the description of each key to understand what UI element it is. Do not add explanations.",
		kind === "paraglide"
			? 'Answer with ONE JSON object and nothing else: {"<key>": <value>, ...}. A value is a string, or, when the English value is a variants array, the same variants array with every match arm translated and the selectors/declarations unchanged. Plural selectors need the variants the target language requires (pt-BR: one and other).'
			: 'Answer with ONE JSON object and nothing else: {"<key>": "<the whole Fluent message>", ...}. Each value is the complete message source, starting with the id and " = ", with attributes and select expressions as in the English; no comments.',
	].join("\n");

/** Glossary entries that matter for this batch, as prompt lines: a term that occurs in the English text, or an OS
 * noun the English text uses (the translator must then keep the `os` select and give each platform its noun). */
function glossaryLines(ctx: Ctx, locale: string, batch: Pending[]): string[] {
	const text = batch.flatMap((p) => p.base.texts).join("\n");
	const out: string[] = [];
	for (const e of ctx.glossary.entries) {
		const forms = e.os?.[locale];
		if (forms) {
			const en = e.os?.[ctx.locales.default];
			if (en && OS_KEYS.some((k) => termRegex(en[k]).test(text))) out.push(`- ${e.term} (select on os): ${osLine(forms)}`);
			continue;
		}
		if (!termRegex(e.term).test(text)) continue;
		const gender = e.gender?.[locale];
		const advice = e.advice?.[locale];
		const rendering = e[locale];
		if (e.dnt) out.push(`- ${e.term}: never translate, keep exactly${gender ? ` (gender: ${gender})` : ""}`);
		else if (typeof rendering === "string") out.push(`- ${e.term} -> ${rendering}`);
		if (advice) out.push(`- ${e.term}: ${advice}`);
	}
	return out;
}

/** Up to 20 existing, fresh translations of the same areas: the repository is the translation memory. */
function examples(ctx: Ctx, surface: Surface, locale: string, batch: Pending[]): string[] {
	const areas = new Set(batch.map((p) => p.base.area));
	const skip = new Set(batch.map((p) => p.key));
	const lock = readLocks(ctx.root, surface.name, locale).byKey;
	const out: string[] = [];
	for (const [key, t] of surface.entries.get(locale) ?? []) {
		const b = surface.entries.get(surface.base)?.get(key);
		if (out.length >= EXAMPLES) break;
		if (b && areas.has(b.area) && !skip.has(key) && lock.get(key)?.hash === hashOf(b)) out.push(`${key}\n  en: ${b.raw}\n  ${locale}: ${t.raw}`);
	}
	return out;
}

export function buildPrompt(ctx: Ctx, surface: Surface, locale: string, batch: Pending[]): { system: string; user: string } {
	const style = join(ctx.root, "i18n", "style", `${locale}.md`);
	const parts: string[] = [];
	const gl = glossaryLines(ctx, locale, batch);
	if (gl.length) parts.push("Glossary:", ...gl, "");
	if (existsSync(style)) parts.push(`Style guide for ${locale}:`, readText(style).trim(), "");
	const ex = examples(ctx, surface, locale, batch);
	if (ex.length) parts.push("Existing translations of the same areas (match their wording):", ...ex, "");
	parts.push("Translate these:");
	for (const p of batch) {
		parts.push(`\n${p.key}`, `  description: ${p.base.description ?? "(none)"}`, `  en: ${p.base.raw}`);
		if (p.previous) parts.push(`  previous ${locale} (the English changed since): ${p.previous.raw}`);
	}
	return { system: SYSTEM(locale, surface.kind), user: parts.join("\n") };
}

/** The JSON object in a model answer (tolerates a code fence or a sentence around it). */
export function parseAnswer(text: string): Record<string, unknown> {
	const a = text.indexOf("{");
	const b = text.lastIndexOf("}");
	if (a < 0 || b < a) throw new Error("the model answer has no JSON object");
	const obj = JSON.parse(text.slice(a, b + 1));
	if (typeof obj !== "object" || obj === null || Array.isArray(obj)) throw new Error("the model answer is not a JSON object");
	return obj;
}

export interface Candidate {
	key: string;
	/** The value to store: a catalog value (paraglide) or the message source (fluent). */
	value: unknown;
	entry: Entry;
}

/** Validate the answer for `batch`; returns the candidates and every problem found (errors fail the batch). */
export function validate(ctx: Ctx, surface: Surface, locale: string, batch: Pending[], answer: Record<string, unknown>): { ok: Candidate[]; errors: string[] } {
	const ok: Candidate[] = [];
	const errors: string[] = [];
	for (const p of batch) {
		const value = answer[p.key];
		const fail = (m: string) => errors.push(`${p.key}: ${m}`);
		if (value === undefined) {
			fail("missing in the model answer");
			continue;
		}
		let entry: Entry;
		if (surface.kind === "paraglide") {
			const bad = paraglideProblem(value);
			if (bad) {
				fail(bad);
				continue;
			}
			entry = paraglideEntry(p.key, value as ParaglideValue, p.base.file, undefined, undefined);
		} else {
			if (typeof value !== "string") {
				fail("a Fluent value must be the message source as a string");
				continue;
			}
			const r = parseMessage(value);
			if ("error" in r) {
				fail(r.error);
				continue;
			}
			if (r.message.id.name !== p.key) {
				fail(`the message id is "${r.message.id.name}", expected "${p.key}"`);
				continue;
			}
			entry = entryOf(r.message, p.base.file, p.base.area, value);
		}
		const issues = entryIssues(ctx, locale, p.base, entry);
		for (const i of issues) if (i.severity === "error") fail(`${i.rule}: ${i.message}`);
		if (!issues.some((i) => i.severity === "error")) ok.push({ key: p.key, value, entry });
	}
	return { ok, errors };
}

function writeParaglide(loaded: Loaded, locale: string, items: Candidate[]) {
	const path = loaded.catalogs?.get(locale);
	if (!path) throw new Error(`${loaded.surface.name} has no catalog for ${locale}`);
	const text = existsSync(path) ? readText(path) : "{}\n";
	const obj: Record<string, unknown> = JSON.parse(text);
	for (const i of items) obj[i.key] = i.value;
	mkdirSync(dirname(path), { recursive: true });
	writeFileSync(path, canonicalJson(obj, detectIndent(text)));
	const written = readText(path);
	if (formatProblem(written, JSON.parse(written))) throw new Error(`${path}: wrote a non-canonical catalog`);
}

function writeFluent(ctx: Ctx, surface: Surface, locale: string, items: Candidate[]) {
	const parser = new FluentParser({ withSpans: true });
	const byFile = new Map<string, Candidate[]>();
	for (const i of items) {
		const baseFile = surface.entries.get(surface.base)?.get(i.key)?.file ?? "";
		const file = baseFile.replace(`/locales/${surface.base}/`, `/locales/${locale}/`);
		byFile.set(file, [...(byFile.get(file) ?? []), i]);
	}
	for (const [file, list] of byFile) {
		const path = join(ctx.root, file);
		let text = existsSync(path) ? readText(path) : "";
		for (const i of list) {
			const existing = parser.parse(text).body.find((e) => e.type === "Message" && e.id.name === i.key);
			const src = String(i.value).trim();
			if (existing?.type === "Message" && existing.span && existing.id.span) {
				text = text.slice(0, existing.id.span.start) + src + text.slice(existing.span.end);
			} else text = `${text}${text === "" || text.endsWith("\n\n") ? "" : text.endsWith("\n") ? "\n" : "\n\n"}${src}\n`;
		}
		mkdirSync(dirname(path), { recursive: true });
		writeFileSync(path, text);
	}
}

export type Complete = (prompt: { system: string; user: string }) => Promise<string>;

export interface Report {
	surface: string;
	locale: string;
	translated: string[];
	errors: string[];
}

/** Translate everything pending for one surface+locale. Writes only when every key validates. */
export async function translateSurface(ctx: Ctx, loaded: Loaded, locale: string, complete: Complete): Promise<Report> {
	const { surface } = loaded;
	const todo = pending(ctx, surface, locale);
	const report: Report = { surface: surface.name, locale, translated: [], errors: [] };
	const accepted: Candidate[] = [];
	for (let i = 0; i < todo.length; i += BATCH) {
		const batch = todo.slice(i, i + BATCH);
		let answer: Record<string, unknown>;
		try {
			answer = parseAnswer(await complete(buildPrompt(ctx, surface, locale, batch)));
		} catch (e) {
			report.errors.push(`batch ${i / BATCH + 1}: ${(e as Error).message}`);
			continue;
		}
		const v = validate(ctx, surface, locale, batch, answer);
		accepted.push(...v.ok);
		report.errors.push(...v.errors);
	}
	if (report.errors.length || accepted.length === 0) return report;
	if (surface.kind === "paraglide") writeParaglide(loaded, locale, accepted);
	else writeFluent(ctx, surface, locale, accepted);
	// Reload what was just written so the lock stamps the keys that now exist.
	const fresh = loadSurfaces(ctx).find((l) => l.surface.name === surface.name);
	if (fresh) rebuildLock(ctx.root, fresh.surface, locale, new Set(accepted.map((c) => c.key)));
	report.translated = accepted.map((c) => c.key);
	return report;
}

/** A model call from the environment: OpenAI-compatible when I18N_BASE_URL is set, else Anthropic. */
export function providerFromEnv(env: Record<string, string | undefined> = process.env): Complete {
	const model = env.I18N_MODEL;
	if (!model) throw new Error("set I18N_MODEL (the model id), plus ANTHROPIC_API_KEY, or I18N_BASE_URL and I18N_API_KEY for an OpenAI-compatible endpoint");
	if (env.I18N_BASE_URL) {
		const url = `${env.I18N_BASE_URL.replace(/\/$/, "")}/chat/completions`;
		return async ({ system, user }) => {
			const r = await fetch(url, {
				method: "POST",
				headers: { "content-type": "application/json", ...(env.I18N_API_KEY ? { authorization: `Bearer ${env.I18N_API_KEY}` } : {}) },
				body: JSON.stringify({ model, temperature: 0, messages: [{ role: "system", content: system }, { role: "user", content: user }] }),
			});
			if (!r.ok) throw new Error(`${url}: HTTP ${r.status} ${await r.text()}`);
			const j = await r.json();
			const content = j?.choices?.[0]?.message?.content;
			if (typeof content !== "string") throw new Error(`${url}: no message content in the response`);
			return content;
		};
	}
	if (!env.ANTHROPIC_API_KEY) throw new Error("set ANTHROPIC_API_KEY, or I18N_BASE_URL (and I18N_API_KEY) for an OpenAI-compatible endpoint");
	return async ({ system, user }) => {
		const r = await fetch("https://api.anthropic.com/v1/messages", {
			method: "POST",
			headers: { "content-type": "application/json", "x-api-key": env.ANTHROPIC_API_KEY ?? "", "anthropic-version": "2023-06-01" },
			body: JSON.stringify({ model, max_tokens: 16000, temperature: 0, system, messages: [{ role: "user", content: user }] }),
		});
		if (!r.ok) throw new Error(`anthropic: HTTP ${r.status} ${await r.text()}`);
		const j = await r.json();
		const block = Array.isArray(j?.content) ? j.content.find((c: { type?: string }) => c?.type === "text") : undefined;
		if (typeof block?.text !== "string") throw new Error("anthropic: no text block in the response");
		return block.text;
	};
}

async function main() {
	const args = process.argv.slice(2);
	const opt = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const root = opt("--root") ?? join(import.meta.dir, "..", "..");
	const ctx = loadCtx(root, opt("--surface"));
	const only = opt("--locale");
	const dry = args.includes("--dry-run");
	const targets = Object.keys(ctx.locales.locales).filter((l) => l !== ctx.locales.default && (!only || l === only));
	if (only && targets.length === 0) {
		console.error(`i18n:translate: "${only}" is not a target locale of i18n/locales.json`);
		process.exit(1);
	}
	let complete: Complete | undefined;
	let failed = false;
	let any = false;
	for (const loaded of loadSurfaces(ctx)) {
		for (const locale of targets.filter((l) => loaded.surface.locales.includes(l))) {
			const todo = pending(ctx, loaded.surface, locale);
			if (todo.length === 0) continue;
			any = true;
			if (dry) {
				for (let i = 0; i < todo.length; i += BATCH) {
					const p = buildPrompt(ctx, loaded.surface, locale, todo.slice(i, i + BATCH));
					console.log(`=== ${loaded.surface.name} ${locale} (batch ${i / BATCH + 1}) ===\n--- system ---\n${p.system}\n--- user ---\n${p.user}\n`);
				}
				continue;
			}
			try {
				complete ??= providerFromEnv();
			} catch (e) {
				console.error(`i18n:translate: ${(e as Error).message}`);
				process.exit(1);
			}
			const r = await translateSurface(ctx, loaded, locale, complete);
			if (r.errors.length) {
				failed = true;
				console.error(`${r.surface} ${r.locale}: nothing written, ${r.errors.length} problem(s):`);
				for (const e of r.errors) console.error(`  ${e}`);
			} else console.log(`${r.surface} ${r.locale}: ${r.translated.length} key(s) written and stamped; run bun run i18n:check`);
		}
	}
	if (!any) console.log("i18n:translate: nothing to translate");
	if (failed) process.exit(1);
}

if (import.meta.main) await main();
