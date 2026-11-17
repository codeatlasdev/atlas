// bun run i18n:find <text|key> [--json]
// Before adding a string, find out whether it exists. A KEY gives its area file:line, its values and where the sources
// use it; a TEXT gives the messages whose English (or any locale's) text equals it, contains it, or is close to it.
// Exit 1 (with the command that adds it) when nothing matches. `--json` is the same data for an agent.
import { join } from "node:path";
import { dirname } from "node:path";
import { CommandError } from "./edit.ts";
import type { Entry } from "./model.ts";
import { type Source, consumers, loadSource } from "./source.ts";
import { type Ctx, loadCtx } from "./types.ts";
import { readText, walk } from "./util.ts";

export interface Match {
	key: string;
	area: string;
	file: string;
	line?: number;
	how: "key" | "prefix" | "equal" | "contains" | "similar";
	values: Record<string, string>;
	description?: string;
}

export interface Usage {
	surface: string;
	file: string;
	line: number;
}

export interface FindResult {
	query: string;
	mode: "key" | "text";
	matches: Match[];
	/** Only in key mode. */
	usages: Usage[];
}

const norm = (s: string) => s.toLowerCase().replace(/\{[^{}]*\}/g, " ").replace(/[^\p{L}\p{N}]+/gu, " ").trim();
const words = (s: string) => new Set(norm(s).split(" ").filter(Boolean));

function similarity(a: string, b: string): number {
	const x = words(a);
	const y = words(b);
	if (x.size === 0 || y.size === 0) return 0;
	const common = [...x].filter((w) => y.has(w)).length;
	return common / (x.size + y.size - common);
}

/** The values of an entry per locale, as the text a person reads. */
function valuesOf(source: Source, e: Entry): Record<string, string> {
	const out: Record<string, string> = {};
	for (const a of [source.areas.get(e.area)]) {
		for (const [locale, entries] of a?.entries ?? []) {
			const x = entries.get(e.key);
			if (x) out[locale] = x.texts.length === 1 ? x.texts[0] : JSON.stringify(x.value);
		}
	}
	return out;
}

const matchOf = (source: Source, e: Entry, how: Match["how"]): Match => ({ key: e.key, area: e.area, file: e.file, line: e.line, how, values: valuesOf(source, e), description: e.description });

/** Lines of the consuming surfaces' sources that mention the key. */
export function usagesOf(source: Source, key: string): Usage[] {
	const entry = [...source.areas.values()].flatMap((a) => a.entries.get(a.base)?.get(key) ?? []).at(0);
	if (!entry) return [];
	const re = new RegExp(`(?<![A-Za-z0-9_])${key}(?![A-Za-z0-9_])`);
	const out: Usage[] = [];
	for (const s of consumers(source.config, entry.area)) {
		const def = source.config.surfaces[s];
		const dir = def.paraglide ? `${dirname(def.paraglide)}/src` : def.rust;
		if (!dir) continue;
		for (const path of walk(join(source.root, dir), (p) => /\.(?:svelte|ts|tsx|js|mjs|astro|rs)$/.test(p) && !/[\\/]paraglide[\\/]/.test(p))) {
			readText(path).split("\n").forEach((l, i) => {
				if (re.test(l)) out.push({ surface: s, file: path.slice(source.root.length + 1).replaceAll("\\", "/"), line: i + 1 });
			});
		}
	}
	return out;
}

export function find(ctx: Ctx, query: string): FindResult {
	const source = loadSource(ctx);
	const entries = [...source.areas.values()].flatMap((a) => [...(a.entries.get(a.base)?.values() ?? [])]);
	const q = query.trim();
	if (q === "") throw new CommandError('usage: bun run i18n:find <text|key>; add a message with bun run i18n:add <key> "English text" --desc "what it is"');
	const exact = entries.find((e) => e.key === q);
	if (exact) return { query: q, mode: "key", matches: [matchOf(source, exact, "key")], usages: usagesOf(source, q) };
	if (/^[a-z][a-z0-9_]*$/.test(q) && q.includes("_")) {
		const prefix = entries.filter((e) => e.key.startsWith(q));
		if (prefix.length) return { query: q, mode: "key", matches: prefix.map((e) => matchOf(source, e, "prefix")), usages: [] };
	}
	const nq = norm(q);
	const scored: { e: Entry; how: Match["how"]; score: number }[] = [];
	for (const e of entries) {
		const texts = e.texts.map(norm);
		const others = [...source.areas.get(e.area)?.entries.values() ?? []].flatMap((m) => [...(m.get(e.key)?.texts ?? [])].map(norm));
		if (texts.includes(nq)) scored.push({ e, how: "equal", score: 3 });
		else if (others.includes(nq)) scored.push({ e, how: "equal", score: 2.5 });
		else if (nq.length >= 3 && (texts.some((t) => t.includes(nq) || (t.length >= 3 && nq.includes(t))) || others.some((t) => t.includes(nq)))) scored.push({ e, how: "contains", score: 2 });
		else {
			const sim = Math.max(...e.texts.map((t) => similarity(q, t)), 0);
			if (sim >= 0.5) scored.push({ e, how: "similar", score: sim });
		}
	}
	scored.sort((a, b) => b.score - a.score || (a.e.key < b.e.key ? -1 : 1));
	return { query: q, mode: "text", matches: scored.slice(0, 20).map((s) => matchOf(source, s.e, s.how)), usages: [] };
}

export function render(r: FindResult): string {
	const lines: string[] = [];
	for (const m of r.matches) {
		lines.push(`${m.key}  [${m.how}]  ${m.file}${m.line ? `:${m.line}` : ""}`);
		for (const [l, v] of Object.entries(m.values)) lines.push(`  ${l}: ${v}`);
		if (m.description) lines.push(`  about: ${m.description}`);
	}
	if (r.usages.length) {
		lines.push("used in:");
		for (const u of r.usages) lines.push(`  ${u.file}:${u.line} (${u.surface})`);
	} else if (r.mode === "key" && r.matches.length === 1) lines.push("used in: nowhere yet");
	return lines.join("\n");
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const json = args.includes("--json");
	const rootAt = args.indexOf("--root");
	const root = rootAt >= 0 ? args[rootAt + 1] : join(import.meta.dir, "..", "..");
	const query = args.filter((a, i) => !a.startsWith("--") && args[i - 1] !== "--root").join(" ");
	try {
		const r = find(loadCtx(root), query);
		if (json) console.log(JSON.stringify(r, null, 2));
		else console.log(render(r));
		if (r.matches.length === 0) {
			const fix = `no message matches "${query}": add one with bun run i18n:add <key> "${query}" --desc "what it is"`;
			if (json) console.error(JSON.stringify({ error: fix }));
			else console.error(fix);
			process.exit(1);
		}
	} catch (e) {
		if (json) console.error(JSON.stringify({ error: (e as Error).message }));
		else console.error((e as Error).message);
		process.exit(1);
	}
}
