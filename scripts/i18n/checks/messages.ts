// The source tree i18n/messages/** (each area names its surface in its area.json) and i18n/surfaces.json: every rule of one area (engine.ts) plus the rules that look
// across areas and surfaces. Every finding names the command that fixes it.
//   area-surface     (from the loader) an area with no area.json, one naming an unknown surface, a malformed file
//   dup-key          the same key in two areas (Paraglide silently keeps the last one)
//   budget           an area file over the keys or bytes of surfaces.json: split it
//   dup-common       an English value equal to a common one (use common_action_cancel) unless meta says `distinct`
//   dup-value        the same English value under two non-common keys (warning)
//   dangling-ref     `{@key}` that does not exist, a cycle, or a target some consuming surface does not have
//   unused           a key none of the surfaces consuming it references (common*, generated and cli_* areas are exempt: their ids are computed)
//   undefined-key    `t!("x")` / `m.x(` in a source whose key is not in the tree; unconsumed-key: it is, in an area the surface lacks
//   generate         a message shape the generator cannot express
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { checkArea, statusArea } from "../engine.ts";
import { outputsFor } from "../generate.ts";
import type { Area, Entry } from "../model.ts";
import { refsOf } from "../resolve.ts";
import { type Source, areasFor, consumedAreas, consumers, isCommon, loadSource } from "../source.ts";
import type { Checker, Ctx, Finding } from "../types.ts";
import { isUsed, scanRefs } from "../unused.ts";

const err = (rule: string, file: string, message: string, key?: string, line?: number): Finding => ({ rule, severity: "error", file, line, key, message });

/** The English value of an entry as text, for duplicate detection: strings only, trimmed, case-folded, one trailing `.` or `…` dropped. */
const plainValue = (e: Entry): string | undefined => (typeof e.value === "string" ? e.value.trim().toLowerCase().replace(/[.…]$/, "").trim() : undefined);

/** Suggested sub-areas for an area over budget: the keys grouped by their second word. */
function splitAdvice(area: Area, keys: string[]): string {
	const groups = new Map<string, number>();
	for (const k of keys) {
		const rest = k.slice(area.name.length + 1);
		const g = rest.includes("_") ? rest.slice(0, rest.indexOf("_")) : rest;
		groups.set(g, (groups.get(g) ?? 0) + 1);
	}
	const top = [...groups].sort((a, b) => b[1] - a[1]).slice(0, 4).map(([g, n]) => `${area.name}_${g} (${n} keys)`);
	return `split it into areas by key prefix, e.g. ${top.join(", ")}: move the keys with i18n:mv --new-area --surface <s>`;
}

function budgetFindings(source: Source, areas: Area[]): Finding[] {
	const out: Finding[] = [];
	const { keys: maxKeys, bytes: maxBytes } = source.config.budget;
	for (const area of areas) {
		for (const locale of area.locales) {
			const file = `${area.dir}/${locale}.json`;
			const n = area.entries.get(locale)?.size ?? 0;
			let bytes = 0;
			try {
				bytes = readFileSync(join(source.root, file)).length;
			} catch {
				continue;
			}
			if (n > maxKeys || bytes > maxBytes) {
				const keys = [...(area.entries.get(locale)?.keys() ?? [])];
				out.push(err("budget", file, `${n} keys, ${bytes} bytes (budget ${maxKeys} keys, ${maxBytes} bytes per area file): ${splitAdvice(area, keys)}`));
			}
		}
	}
	return out;
}

function duplicateFindings(areas: Area[]): Finding[] {
	const out: Finding[] = [];
	const common = new Map<string, Entry>();
	for (const a of areas.filter((x) => isCommon(x.name))) for (const e of a.entries.get(a.base)?.values() ?? []) common.set(plainValue(e) ?? "\u0000", e);
	const seen = new Map<string, Entry>();
	for (const a of areas.filter((x) => !isCommon(x.name))) {
		for (const e of a.entries.get(a.base)?.values() ?? []) {
			const v = plainValue(e);
			if (v === undefined || v === "") continue;
			const c = common.get(v);
			if (c && !e.distinct) {
				out.push(err("dup-common", e.file, `the English text equals ${c.key}: use ${c.key} (the shared message), or say why this one differs: meta "distinct": "<reason>" or \`i18n-distinct: <reason>\` in its description`, e.key, e.line));
				continue;
			}
			const first = seen.get(v);
			if (first && !e.distinct) out.push({ rule: "dup-value", severity: "warn", file: e.file, line: e.line, key: e.key, message: `the same English text as ${first.key}: same role: fold it with bun run i18n:mv ${e.key} ${first.key} --into (it refuses a locale translated differently: --take <locale>=dup keeps the duplicate's); different role (a state vs a verb, another gender): add \`i18n-distinct: <why>\` to its description` });
			else seen.set(v, e);
		}
	}
	return out;
}

/** A reference must point at a message every surface consuming the referrer also consumes. */
function refScopeFindings(source: Source, areas: Area[]): Finding[] {
	const out: Finding[] = [];
	const owner = new Map<string, string>();
	for (const a of source.areas.values()) for (const k of a.entries.get(a.base)?.keys() ?? []) owner.set(k, a.name);
	for (const a of areas) {
		for (const e of a.entries.get(a.base)?.values() ?? []) {
			for (const ref of refsOf(e.value)) {
				const target = owner.get(ref);
				if (!target || isCommon(target)) continue;
				const missing = consumers(source.config, a.name).filter((s) => !consumedAreas(source.config, s).includes(target));
				if (missing.length) out.push(err("dangling-ref", e.file, `{@${ref}} is in area ${target}, which ${missing.join(", ")} does not consume (a reference may only target a common area or one the same surfaces consume)`, e.key, e.line));
			}
		}
	}
	return out;
}

function unusedFindings(source: Source, areas: Area[]): Finding[] {
	const out: Finding[] = [];
	const refsOfSurface = new Map(Object.entries(source.config.surfaces).map(([name, def]) => [name, scanRefs(source.root, def)]));
	for (const a of areas) {
		if (isCommon(a.name) || a.generated || a.name.startsWith("cli_")) continue; // cli_* ids are computed from the clap tree
		const scanners = consumers(source.config, a.name).map((s) => refsOfSurface.get(s)).filter((r) => r !== undefined);
		if (scanners.length === 0) continue;
		for (const e of a.entries.get(a.base)?.values() ?? []) {
			if (!scanners.some((r) => isUsed(r, e.key))) {
				out.push(err("unused", e.file, `no reference to this key in the source of ${consumers(source.config, a.name).join(" or ")} (m.key, t!("key")); delete it, or annotate dynamic use with \`// i18n-keys: prefix_*\``, e.key, e.line));
			}
		}
	}
	return out;
}

/** Every surface's generator must be able to express every message (a shape error names the key). */
function generateFindings(source: Source, surfaces: string[]): Finding[] {
	const out: Finding[] = [];
	for (const s of surfaces) {
		try {
			outputsFor(source, s);
		} catch (e) {
			out.push(err("generate", "i18n/surfaces.json", `${(e as Error).message}; simplify the message or change the surface (bun run i18n:generate shows it)`));
		}
	}
	return out;
}

/** A key may live in one area only. */
function dupKeyFindings(source: Source): Finding[] {
	const where = new Map<string, Entry[]>();
	for (const a of source.areas.values()) for (const e of a.entries.get(a.base)?.values() ?? []) where.set(e.key, [...(where.get(e.key) ?? []), e]);
	return [...where.values()]
		.filter((es) => es.length > 1)
		.flatMap((es) => es.map((e) => err("dup-key", e.file, `defined in ${es.map((x) => x.area).join(" and ")}; Paraglide silently keeps the last one: keep one (bun run i18n:mv ${e.key} <new_key> renames this one)`, e.key, e.line)));
}

/** A reference in a source to a key that is not in the tree (or not in an area the surface consumes). */
function referenceFindings(source: Source, surfaces: string[]): Finding[] {
	const out: Finding[] = [];
	const owner = new Map<string, string>();
	for (const a of source.areas.values()) for (const k of a.entries.get(a.base)?.keys() ?? []) if (!owner.has(k)) owner.set(k, a.name);
	for (const s of surfaces) {
		const refs = scanRefs(source.root, source.config.surfaces[s]);
		for (const r of refs?.calls ?? []) {
			const area = owner.get(r.key);
			if (!area) out.push(err("undefined-key", r.file, `no message "${r.key}" in i18n/messages: find the right one with bun run i18n:find ${r.key}, or add it with bun run i18n:add ${r.key} "English text" --desc "what it is"`, r.key, r.line));
			else if (!consumedAreas(source.config, s).includes(area)) out.push(err("unconsumed-key", r.file, `"${r.key}" is in area ${area}, which surface ${s} does not consume: set the surface of ${area} in i18n/messages/${area}/area.json, or use a message of an area it has (bun run i18n:find ${r.key})`, r.key, r.line));
		}
	}
	return out;
}

const checker: Checker = {
	name: "messages",

	async run(ctx) {
		if (ctx.surface && !loadSource(ctx).config.surfaces[ctx.surface]) return [];
		const source = loadSource(ctx);
		const areas = areasFor(source, ctx.surface);
		const names = [...source.areas.keys()];
		const out: Finding[] = [...source.problems];
		for (const a of areas) out.push(...checkArea(ctx, a, names));
		const surfaces = ctx.surface ? [ctx.surface] : Object.keys(source.config.surfaces);
		out.push(...dupKeyFindings(source), ...budgetFindings(source, areas), ...duplicateFindings(areas), ...refScopeFindings(source, areas), ...unusedFindings(source, areas));
		out.push(...generateFindings(source, surfaces), ...referenceFindings(source, surfaces));
		return out;
	},

	async status(ctx: Ctx) {
		const source = loadSource(ctx);
		const all = Object.keys(source.config.surfaces);
		return areasFor(source, ctx.surface).flatMap((a) => {
			const who = consumers(source.config, a.name);
			return statusArea(ctx, a, who.length === all.length ? "all" : who.join("+") || "-");
		});
	},
};

export default checker;
