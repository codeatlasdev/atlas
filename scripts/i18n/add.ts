// bun run i18n:add <key> "English" --desc "what it is" [--pt-BR "..."] [--surface s] [--new-area] [--distinct "why"] [--json]
// Adds one message to the source tree: the area is the longest existing area name the key starts with (`settings_` ->
// settings); an unknown area needs --new-area, which creates the folder and lists it under --surface in surfaces.json.
// Refuses a key that exists, a text that equals a common message (use that one) and anything the rules of i18n:check
// would reject, before writing a byte. Writes en.json, meta.json and (with --pt-BR) pt-BR.json, then stamps the lock.
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { CommandError, setKey } from "./edit.ts";
import { entryIssues, sourceEntryIssues } from "./engine.ts";
import { fix } from "./fix.ts";
import { type MetaEntry } from "./model.ts";
import { areaOfKey, isCommon, KEY_PATTERN, loadSource, loadSurfacesConfig, makeEntry, renderSurfaces } from "./source.ts";
import { type Ctx, SURFACES_FILE, loadCtx } from "./types.ts";

export interface AddOptions {
	key: string;
	english: string;
	description: string;
	pt?: string;
	surface?: string;
	newArea?: boolean;
	distinct?: string;
}

export interface Added {
	key: string;
	area: string;
	createdArea: boolean;
	files: string[];
}

const plain = (t: string) => t.trim().toLowerCase().replace(/[.…]$/, "").trim();

/** Add `key`; throws a CommandError naming the fix when the key or text is not acceptable. Writes nothing then. */
export function addMessage(ctx: Ctx, o: AddOptions): Added {
	const source = loadSource(ctx);
	if (!KEY_PATTERN.test(o.key)) throw new CommandError(`"${o.key}" is not a key: snake_case, an area and a name (settings_display_language); the area is the part before the name`);
	const taken = [...source.areas.values()].find((a) => a.entries.get(a.base)?.has(o.key));
	if (taken) throw new CommandError(`${o.key} already exists in ${taken.dir}/en.json: see it with bun run i18n:find ${o.key}`);
	if (!o.english.trim()) throw new CommandError('the English text is empty: bun run i18n:add <key> "English text" --desc "what it is"');
	if (!o.description.trim()) throw new CommandError("every message needs --desc (what UI element it is, and any disambiguation)");

	const names = [...source.areas.keys()];
	let area = areaOfKey(o.key, names);
	let createdArea = false;
	if (!area) {
		if (!o.newArea) {
			const guess = o.key.slice(0, o.key.indexOf("_"));
			throw new CommandError(`no area for "${o.key}" (areas: ${names.join(", ")}): use a key that starts with one of them, or create ${guess} with --new-area --surface <${Object.keys(source.config.surfaces).join("|")}>`);
		}
		area = o.key.slice(0, o.key.indexOf("_"));
		if (!o.surface || !source.config.surfaces[o.surface]) throw new CommandError(`--new-area needs --surface <${Object.keys(source.config.surfaces).join("|")}>: the surface that consumes the new area ${area}`);
		createdArea = true;
	}
	if (!createdArea && !isCommon(area)) {
		const consumedBy = Object.entries(source.config.surfaces).filter(([, d]) => d.areas.includes(area as string)).map(([s]) => s);
		if (o.surface && !consumedBy.includes(o.surface)) throw new CommandError(`area ${area} is not consumed by ${o.surface} (it is by ${consumedBy.join(", ") || "no surface"}): pick a key of an area ${o.surface} lists in ${SURFACES_FILE}`);
	}
	if (!isCommon(area) && !o.distinct) {
		for (const a of [...source.areas.values()].filter((x) => isCommon(x.name))) {
			for (const c of a.entries.get(a.base)?.values() ?? []) {
				if (typeof c.value === "string" && plain(c.value) === plain(o.english)) throw new CommandError(`"${o.english}" is already ${c.key}: use it (m.${c.key}() / t!("${c.key}")), or add it with --distinct "why this one differs"`);
			}
		}
	}

	const meta: MetaEntry = { description: o.description.trim(), ...(o.distinct ? { distinct: o.distinct } : {}) };
	const lookup = source.lookup(ctx.locales.default);
	const en = makeEntry({ key: o.key, area, file: `i18n/messages/${area}/en.json`, value: o.english, lookup, meta });
	const bad = sourceEntryIssues(en, [...names, area]).filter((i) => i.severity === "error");
	let pt;
	if (o.pt !== undefined) {
		pt = makeEntry({ key: o.key, area, file: `i18n/messages/${area}/pt-BR.json`, value: o.pt, lookup: source.lookup("pt-BR") });
		bad.push(...entryIssues(ctx, "pt-BR", en, pt).filter((i) => i.severity === "error"));
	}
	if (bad.length) throw new CommandError(`${o.key} is not acceptable: ${bad.map((i) => `${i.rule}: ${i.message}`).join("; ")}`);

	const files = [`i18n/messages/${area}/en.json`, `i18n/messages/${area}/meta.json`];
	setKey(ctx.root, area, "en", o.key, o.english);
	setKey(ctx.root, area, "meta", o.key, o.distinct ? meta : meta.description);
	if (o.pt !== undefined) {
		setKey(ctx.root, area, "pt-BR", o.key, o.pt);
		files.push(`i18n/messages/${area}/pt-BR.json`);
	}
	if (createdArea && o.surface) registerArea(ctx.root, o.surface, area);
	fix(loadCtx(ctx.root));
	return { key: o.key, area, createdArea, files };
}

/** List a new area under a surface of surfaces.json (the file keeps its layout: one line per surface, areas sorted). */
function registerArea(root: string, surface: string, area: string) {
	const { config } = loadSurfacesConfig(root);
	config.surfaces[surface].areas = [...new Set([...config.surfaces[surface].areas, area])];
	writeFileSync(join(root, SURFACES_FILE), renderSurfaces(config));
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const flag = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const json = args.includes("--json");
	const root = flag("--root") ?? join(import.meta.dir, "..", "..");
	const valueFlags = new Set(["--desc", "--pt-BR", "--surface", "--distinct", "--root"]);
	const positional = args.filter((a, i) => !a.startsWith("--") && !valueFlags.has(args[i - 1] ?? ""));
	try {
		const [key, english] = positional;
		if (!key || english === undefined) throw new CommandError('usage: bun run i18n:add <key> "English text" --desc "what it is" [--pt-BR "..."] [--surface s] [--new-area]');
		const r = addMessage(loadCtx(root), { key, english, description: flag("--desc") ?? "", pt: flag("--pt-BR"), surface: flag("--surface"), newArea: args.includes("--new-area"), distinct: flag("--distinct") });
		if (json) console.log(JSON.stringify(r, null, 2));
		else {
			console.log(`added ${r.key} to area ${r.area}${r.createdArea ? " (new area)" : ""}: ${r.files.join(", ")}`);
			if (!r.files.some((f) => f.endsWith("pt-BR.json"))) console.log(`pt-BR is required: bun run i18n:translate --locale pt-BR --area ${r.area}, or write it in i18n/messages/${r.area}/pt-BR.json then bun run i18n:fix`);
		}
	} catch (e) {
		if (!(e instanceof CommandError)) throw e;
		if (json) console.error(JSON.stringify({ error: e.message }));
		else console.error(`i18n:add: ${e.message}`);
		process.exit(1);
	}
}
