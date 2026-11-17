// bun run i18n:translate [--locale pt-BR] [--area x] [--surface y] [--dry-run]
// Translates only what is new or changed: keys the locale lacks, and keys whose English hash differs from the lock.
// Bring your own model: ANTHROPIC_API_KEY (+ I18N_MODEL), or any OpenAI-compatible endpoint with I18N_BASE_URL
// (+ I18N_API_KEY, I18N_MODEL; Ollama works). The model output is validated with the same rules `i18n:check` runs
// (placeholders, plural forms, glossary, pt-PT words) and nothing is written for an area and locale if any key fails.
// --dry-run prints the prompts and touches neither the network nor a file.
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type Entry, entryIssues, hashOf } from "./engine.ts";
import { rebuildLock } from "./fix.ts";
import { OS_KEYS, osLine, termRegex } from "./glossary.ts";
import type { Area } from "./model.ts";
import { readLock } from "./lock.ts";
import { type MsgValue } from "./resolve.ts";
import { paraglideProblem } from "./shape.ts";
import { type Source, areasFor, loadSource, makeEntry } from "./source.ts";
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

/** Keys of `area` that `locale` lacks or whose English changed since they were translated (not merely unstamped). */
export function pending(ctx: Ctx, area: Area, locale: string): Pending[] {
	const base = area.entries.get(area.base) ?? new Map<string, Entry>();
	const mine = area.entries.get(locale) ?? new Map<string, Entry>();
	const lock = readLock(ctx.root, area.name, locale).byKey;
	const out: Pending[] = [];
	for (const [key, b] of base) {
		const t = mine.get(key);
		const stamp = lock.get(key);
		if (!t) out.push({ key, base: b });
		else if (stamp && stamp !== hashOf(b)) out.push({ key, base: b, previous: t });
	}
	return out;
}

const SYSTEM = (locale: string) =>
	[
		`You translate user interface text of the Atlas developer tool from English into ${locale}.`,
		"Follow the glossary and the style guide exactly. Keep every {placeholder}, {@reference}, markup tag, `code span`, URL and plural/select structure; translate nothing inside them.",
		"Use the description of each key to understand what UI element it is. Do not add explanations.",
		'Answer with ONE JSON object and nothing else: {"<key>": <value>, ...}. A value is a string, or, when the English value is a variants array, the same variants array with every match arm translated and the selectors/declarations unchanged. Plural selectors need the variants the target language requires (pt-BR: one and other).',
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

/** Up to 20 existing, fresh translations of the same area: the repository is the translation memory. */
function examples(ctx: Ctx, area: Area, locale: string, batch: Pending[]): string[] {
	const skip = new Set(batch.map((p) => p.key));
	const lock = readLock(ctx.root, area.name, locale).byKey;
	const out: string[] = [];
	for (const [key, t] of area.entries.get(locale) ?? []) {
		const b = area.entries.get(area.base)?.get(key);
		if (out.length >= EXAMPLES) break;
		if (b && !skip.has(key) && lock.get(key) === hashOf(b)) out.push(`${key}\n  en: ${b.raw}\n  ${locale}: ${t.raw}`);
	}
	return out;
}

export function buildPrompt(ctx: Ctx, area: Area, locale: string, batch: Pending[]): { system: string; user: string } {
	const style = join(ctx.root, "i18n", "style", `${locale}.md`);
	const parts: string[] = [];
	const gl = glossaryLines(ctx, locale, batch);
	if (gl.length) parts.push("Glossary:", ...gl, "");
	if (existsSync(style)) parts.push(`Style guide for ${locale}:`, readText(style).trim(), "");
	const ex = examples(ctx, area, locale, batch);
	if (ex.length) parts.push("Existing translations of the same area (match their wording):", ...ex, "");
	parts.push("Translate these:");
	for (const p of batch) {
		parts.push(`\n${p.key}`, `  description: ${p.base.description ?? "(none)"}`, `  en: ${p.base.raw}`);
		if (p.previous) parts.push(`  previous ${locale} (the English changed since): ${p.previous.raw}`);
	}
	return { system: SYSTEM(locale), user: parts.join("\n") };
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
	value: MsgValue;
	entry: Entry;
}

/** Validate the answer for `batch`; returns the candidates and every problem found (errors fail the batch). */
export function validate(ctx: Ctx, source: Source, area: Area, locale: string, batch: Pending[], answer: Record<string, unknown>): { ok: Candidate[]; errors: string[] } {
	const ok: Candidate[] = [];
	const errors: string[] = [];
	for (const p of batch) {
		const value = answer[p.key];
		const fail = (m: string) => errors.push(`${p.key}: ${m}`);
		if (value === undefined) {
			fail("missing in the model answer");
			continue;
		}
		const bad = paraglideProblem(value);
		if (bad) {
			fail(bad);
			continue;
		}
		const v = value as MsgValue;
		const entry = makeEntry({ key: p.key, area: area.name, file: p.base.file, value: v, lookup: source.lookup(locale) });
		const issues = entryIssues(ctx, locale, p.base, entry);
		for (const i of issues) if (i.severity === "error") fail(`${i.rule}: ${i.message}`);
		if (entry.refError) fail(`dangling-ref: ${entry.refError}`);
		if (!issues.some((i) => i.severity === "error") && !entry.refError) ok.push({ key: p.key, value: v, entry });
	}
	return { ok, errors };
}

/** Merge `items` into `<area>/<locale>.json` (canonical form, keys sorted). */
export function writeValues(root: string, area: Area, locale: string, items: { key: string; value: MsgValue }[]) {
	const path = join(root, area.dir, `${locale}.json`);
	const text = existsSync(path) ? readText(path) : "{}\n";
	const obj: Record<string, unknown> = JSON.parse(text);
	for (const i of items) obj[i.key] = i.value;
	writeFileSync(path, canonicalJson(obj, detectIndent(text)));
}

export type Complete = (prompt: { system: string; user: string }) => Promise<string>;

export interface Report {
	area: string;
	locale: string;
	translated: string[];
	errors: string[];
}

/** Translate everything pending for one area and locale. Writes only when every key validates. */
export async function translateArea(ctx: Ctx, source: Source, area: Area, locale: string, complete: Complete): Promise<Report> {
	const todo = pending(ctx, area, locale);
	const report: Report = { area: area.name, locale, translated: [], errors: [] };
	const accepted: Candidate[] = [];
	for (let i = 0; i < todo.length; i += BATCH) {
		const batch = todo.slice(i, i + BATCH);
		let answer: Record<string, unknown>;
		try {
			answer = parseAnswer(await complete(buildPrompt(ctx, area, locale, batch)));
		} catch (e) {
			report.errors.push(`batch ${i / BATCH + 1}: ${(e as Error).message}`);
			continue;
		}
		const v = validate(ctx, source, area, locale, batch, answer);
		accepted.push(...v.ok);
		report.errors.push(...v.errors);
	}
	if (report.errors.length || accepted.length === 0) return report;
	writeValues(ctx.root, area, locale, accepted);
	// Reload what was just written so the lock stamps the keys that now exist.
	const fresh = loadSource(ctx).areas.get(area.name);
	if (fresh) rebuildLock(ctx.root, fresh, locale, new Set(accepted.map((c) => c.key)));
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
	const area = opt("--area");
	const dry = args.includes("--dry-run");
	const targets = Object.keys(ctx.locales.locales).filter((l) => l !== ctx.locales.default && (!only || l === only));
	if (only && targets.length === 0) {
		console.error(`i18n:translate: "${only}" is not a target locale of i18n/locales.json`);
		process.exit(1);
	}
	const source = loadSource(ctx);
	let complete: Complete | undefined;
	let failed = false;
	let any = false;
	for (const a of areasFor(source, ctx.surface).filter((x) => !area || x.name === area)) {
		for (const locale of targets) {
			const todo = pending(ctx, a, locale);
			if (todo.length === 0) continue;
			any = true;
			if (dry) {
				for (let i = 0; i < todo.length; i += BATCH) {
					const p = buildPrompt(ctx, a, locale, todo.slice(i, i + BATCH));
					console.log(`=== ${a.name} ${locale} (batch ${i / BATCH + 1}) ===\n--- system ---\n${p.system}\n--- user ---\n${p.user}\n`);
				}
				continue;
			}
			try {
				complete ??= providerFromEnv();
			} catch (e) {
				console.error(`i18n:translate: ${(e as Error).message}`);
				process.exit(1);
			}
			const r = await translateArea(ctx, source, a, locale, complete);
			if (r.errors.length) {
				failed = true;
				console.error(`${r.area} ${r.locale}: nothing written, ${r.errors.length} problem(s):`);
				for (const e of r.errors) console.error(`  ${e}`);
			} else console.log(`${r.area} ${r.locale}: ${r.translated.length} key(s) written and stamped; run bun run i18n:check`);
		}
	}
	if (!any) console.log("i18n:translate: nothing to translate");
	if (failed) process.exit(1);
}

if (import.meta.main) await main();
