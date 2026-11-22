// bun run i18n:generate [--surface <name>]
// Turns the one source tree (i18n/messages/**) into what each build-time consumer reads. Deterministic and sorted.
// Nothing it writes is committed.
//   paraglide  <dir>/<locale>.json per locale (apps/desktop/.i18n, site/.i18n): the input of Paraglide's compiler
//   arb        <dir>/app_<language>.arb per locale: Flutter ICU messages (the mobile surface)
// Rust has no generator: crates/atlas-i18n/build.rs embeds the same JSON and evaluates it. References (`{@common_os_trash}`)
// are inlined for Paraglide and ARB, evaluated at run time in Rust.
//
// This file and what it imports must run with bun alone: the apps call it before their own `bun install` is
// complete (no svelte imports on this path).
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { arbFile, arbLocale } from "./emit/arb.ts";
import { MessageError } from "./resolve.ts";
import { type Source, consumedAreas, loadSource } from "./source.ts";
import { type Ctx, loadCtx } from "./types.ts";
import { canonicalJson, readText } from "./util.ts";

export interface Output {
	/** Repo-relative, forward slashes. */
	path: string;
	content: string;
}

/** The files a surface's generator writes. Throws a MessageError naming the key for a shape the target cannot say. */
export function outputsFor(source: Source, surface: string): Output[] {
	const def = source.config.surfaces[surface];
	if (!def) throw new Error(`no surface "${surface}" in i18n/surfaces.json`);
	const areas = consumedAreas(source.config, surface).flatMap((n) => source.areas.get(n) ?? []);
	const locales = areas[0]?.locales ?? [];
	const out: Output[] = [];
	const messagesOf = (locale: string, area = areas) =>
		area.flatMap((a) => [...(a.entries.get(locale)?.values() ?? [])].map((e) => ({ key: e.key, value: e.resolved, description: locale === a.base ? e.description : undefined, area: a.name })));
	try {
		if (def.paraglide) {
			for (const l of locales) {
				const obj = Object.fromEntries(messagesOf(l).sort((a, b) => (a.key < b.key ? -1 : 1)).map((m) => [m.key, m.value]));
				out.push({ path: `${def.paraglide}/${l}.json`, content: canonicalJson(obj, "\t") });
			}
		}
		if (def.arb) {
			const names = locales.map(arbLocale);
			if (new Set(names).size !== names.length) throw new Error(`surface ${surface}: two locales of one language (${locales.join(", ")}) would write the same ARB file`);
			for (const l of locales) out.push({ path: `${def.arb}/app_${arbLocale(l)}.arb`, content: arbFile(l, messagesOf(l)) });
		}
	} catch (e) {
		if (e instanceof MessageError) throw new Error(`surface ${surface}: ${e.message}`);
		throw e;
	}
	return out;
}

/** Write a surface's outputs (only the files that changed, so mtimes stay put when nothing did). Returns the paths written. */
export function generateSurface(source: Source, surface: string): string[] {
	const written: string[] = [];
	for (const o of outputsFor(source, surface)) {
		const p = join(source.root, o.path);
		if (existsSync(p) && readText(p) === o.content) continue;
		mkdirSync(dirname(p), { recursive: true });
		writeFileSync(p, o.content);
		written.push(o.path);
	}
	return written;
}

export const generateAll = (ctx: Ctx, only?: string): string[] => {
	const source = loadSource(ctx);
	const names = only ? [only] : Object.keys(source.config.surfaces);
	return names.flatMap((s) => generateSurface(source, s));
};

if (import.meta.main) {
	const args = process.argv.slice(2);
	const opt = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const root = opt("--root") ?? join(import.meta.dir, "..", "..");
	const only = opt("--surface");
	try {
		const source = loadSource(loadCtx(root));
		for (const p of source.problems) throw new Error(`${p.file}: ${p.message}`);
		const names = only ? [only] : Object.keys(source.config.surfaces);
		for (const s of names) for (const p of generateSurface(source, s)) console.log(`${s}: ${p}`);
	} catch (e) {
		console.error(`i18n:generate: ${(e as Error).message}`);
		process.exit(1);
	}
}
