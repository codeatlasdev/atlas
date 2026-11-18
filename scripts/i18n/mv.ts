// bun run i18n:mv <old_key> <new_key> [--new-area --surface s] [--json]
// Renames a key, moving it to the area its new name belongs to, in every locale: values, meta, lock stamps (a stale
// translation stays stale: nothing is re-stamped), `{@old}` references in other messages, and the call sites in the
// sources of every surface: `m.old(`, `m["old"]`, `t!("old"`. Anything else that mentions the old name is REPORTED, not
// edited: it could not be proven to be the key (a Rust string, a comment, an `i18n-keys` annotation).
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { CommandError, areaPath, readObject, removeKey, setKey, writeObject } from "./edit.ts";
import { fix } from "./fix.ts";
import { hash12 } from "./hash.ts";
import { readLock, writeLock } from "./lock.ts";
import { areaOfKey, KEY_PATTERN, loadSource, writeAreaFile, type Source } from "./source.ts";
import { loadCtx, type Ctx } from "./types.ts";
import { lineOf, readText, rel, walk } from "./util.ts";

export interface Spot {
	file: string;
	line: number;
}

export interface MoveReport {
	from: string;
	to: string;
	fromArea: string;
	toArea: string;
	/** Edited for sure: call sites and references. */
	updated: Spot[];
	/** Mentions of the old name that were NOT edited: check them by hand. */
	unproven: Spot[];
	notes: string[];
}

export interface MoveOptions {
	newArea?: boolean;
	surface?: string;
}

const WEB = /\.(?:svelte|ts|tsx|js|jsx|mjs|astro)$/;
const USES_M = /import\s+(?:\*\s+as\s+m|\{[^}]*\bm\b[^}]*\})\s+from\s+["'][^"']*paraglide\/messages/;

/** Source files of every surface, repo-relative. */
function sourceFiles(source: Source): string[] {
	const out = new Set<string>();
	for (const def of Object.values(source.config.surfaces)) {
		const dir = def.paraglide ? `${def.paraglide.replace(/\/[^/]+$/, "")}/src` : def.rust;
		if (!dir) continue;
		const ok = def.rust ? (p: string) => p.endsWith(".rs") : (p: string) => WEB.test(p) && !/[\\/]paraglide[\\/]/.test(p);
		for (const p of walk(join(source.root, dir), ok)) out.add(rel(source.root, p));
	}
	return [...out].sort();
}

/** Rewrite the provable uses of `oldKey` in one source text; the new text and the lines edited. */
export function rewriteUses(text: string, file: string, oldKey: string, newKey: string): { text: string; lines: number[] } {
	const lines: number[] = [];
	const word = oldKey.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
	const patterns: [RegExp, string][] = [];
	if (file.endsWith(".rs")) patterns.push([new RegExp(`(\\bt!\\(\\s*")${word}(")`, "g"), `$1${newKey}$2`]);
	else if (USES_M.test(text)) {
		patterns.push([new RegExp(`(\\bm\\.)${word}(?![A-Za-z0-9_])`, "g"), `$1${newKey}`]);
		patterns.push([new RegExp(`(\\bm\\[\\s*["'\`])${word}(["'\`]\\s*\\])`, "g"), `$1${newKey}$2`]);
	}
	let out = text;
	for (const [re, to] of patterns) {
		for (const m of out.matchAll(re)) lines.push(lineOf(out, m.index ?? 0));
		out = out.replace(re, to);
	}
	return { text: out, lines: [...new Set(lines)].sort((a, b) => a - b) };
}

/** Lines that still mention `key` as a whole word. */
export function mentions(text: string, key: string): number[] {
	const re = new RegExp(`(?<![A-Za-z0-9_])${key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![A-Za-z0-9_])`);
	return text.split("\n").flatMap((l, i) => (re.test(l) ? [i + 1] : []));
}

export function moveKey(ctx: Ctx, oldKey: string, newKey: string, o: MoveOptions = {}): MoveReport {
	const source = loadSource(ctx);
	const names = [...source.areas.keys()];
	const from = [...source.areas.values()].find((a) => a.entries.get(a.base)?.has(oldKey));
	if (!from) throw new CommandError(`no message "${oldKey}": find it with bun run i18n:find ${oldKey}`);
	if (!KEY_PATTERN.test(newKey)) throw new CommandError(`"${newKey}" is not a key: snake_case, an area and a name (settings_display_language)`);
	const taken = [...source.areas.values()].find((a) => a.entries.get(a.base)?.has(newKey));
	if (taken) throw new CommandError(`${newKey} already exists in ${taken.dir}/en.json: see it with bun run i18n:find ${newKey}`);
	let toName = areaOfKey(newKey, names);
	let createdArea = false;
	if (!toName) {
		if (!o.newArea) throw new CommandError(`no area for "${newKey}" (areas: ${names.join(", ")}): start the key with one of them, or create ${newKey.slice(0, newKey.indexOf("_"))} with --new-area --surface <${Object.keys(source.config.surfaces).join("|")}>`);
		toName = newKey.slice(0, newKey.indexOf("_"));
		if (!o.surface || !source.config.surfaces[o.surface]) throw new CommandError(`--new-area needs --surface <${Object.keys(source.config.surfaces).join("|")}>`);
		createdArea = true;
	}
	const report: MoveReport = { from: oldKey, to: newKey, fromArea: from.name, toArea: toName, updated: [], unproven: [], notes: [] };

	// lock stamps travel with the key
	const stamps = new Map(from.locales.filter((l) => l !== from.base).map((l) => [l, readLock(ctx.root, from.name, l).byKey.get(oldKey)]));
	for (const file of [...from.locales, "meta"]) {
		const value = removeKey(ctx.root, from.name, file, oldKey);
		if (value !== undefined) setKey(ctx.root, toName, file, newKey, value);
	}
	for (const [l, stamp] of stamps) {
		if (!stamp) continue;
		writeLock(ctx.root, from.name, l, Object.fromEntries([...readLock(ctx.root, from.name, l).byKey].filter(([k]) => k !== oldKey)));
		writeLock(ctx.root, toName, l, { ...Object.fromEntries(readLock(ctx.root, toName, l).byKey), [newKey]: stamp });
	}
	if (createdArea && o.surface) writeAreaFile(ctx.root, toName, { surface: o.surface });
	if (!Object.keys(readObject(ctx.root, areaPath(from.name, "en"))).length) report.notes.push(`area ${from.name} is empty now: delete i18n/messages/${from.name}/`);

	// {@old} references inside other messages, every locale. Renaming a reference changes the English text, so the lock
	// hash of every translation that was fresh moves with it (a pure rename is no reason to retranslate); a translation
	// that was already outdated stays outdated.
	for (const a of source.areas.values()) {
		const renamed: { key: string; before: string; after: string }[] = [];
		for (const l of a.locales) {
			const rel0 = areaPath(a.name, l);
			const obj = readObject(ctx.root, rel0);
			const text = JSON.stringify(obj);
			if (!text.includes(`{@${oldKey}}`)) continue;
			writeObject(ctx.root, rel0, JSON.parse(text.replaceAll(`{@${oldKey}}`, `{@${newKey}}`)));
			for (const k of Object.keys(obj)) {
				if (!JSON.stringify(obj[k]).includes(`{@${oldKey}}`)) continue;
				report.updated.push({ file: `${rel0}#${k}`, line: 0 });
				if (l === a.base) renamed.push({ key: k, before: JSON.stringify(obj[k]), after: JSON.stringify(obj[k]).replaceAll(`{@${oldKey}}`, `{@${newKey}}`) });
			}
		}
		for (const l of a.locales.filter((x) => x !== a.base)) {
			const lock = readLock(ctx.root, a.name, l).byKey;
			const next = Object.fromEntries(lock);
			for (const r of renamed) if (lock.get(r.key) === hash12(r.before)) next[r.key] = hash12(r.after);
			writeLock(ctx.root, a.name, l, next);
		}
	}

	// call sites
	for (const file of sourceFiles(source)) {
		const path = join(ctx.root, file);
		const before = readText(path);
		const r = rewriteUses(before, file, oldKey, newKey);
		if (r.text !== before) {
			writeFileSync(path, r.text);
			for (const line of r.lines) report.updated.push({ file, line });
		}
		for (const line of mentions(r.text, oldKey)) report.unproven.push({ file, line });
	}
	fix(loadCtx(ctx.root));
	return report;
}

if (import.meta.main) {
	const args = process.argv.slice(2);
	const flag = (n: string) => (args.includes(n) ? args[args.indexOf(n) + 1] : undefined);
	const json = args.includes("--json");
	const root = flag("--root") ?? join(import.meta.dir, "..", "..");
	const positional = args.filter((a, i) => !a.startsWith("--") && !["--surface", "--root"].includes(args[i - 1] ?? ""));
	try {
		const [oldKey, newKey] = positional;
		if (!oldKey || !newKey) throw new CommandError("usage: bun run i18n:mv <old_key> <new_key> [--new-area --surface s]");
		const r = moveKey(loadCtx(root), oldKey, newKey, { newArea: args.includes("--new-area"), surface: flag("--surface") });
		if (json) console.log(JSON.stringify(r, null, 2));
		else {
			console.log(`moved ${r.from} (${r.fromArea}) -> ${r.to} (${r.toArea})`);
			for (const u of r.updated) console.log(`  updated ${u.file}${u.line ? `:${u.line}` : ""}`);
			for (const u of r.unproven) console.log(`  CHECK ${u.file}:${u.line} still mentions ${r.from}`);
			for (const n of r.notes) console.log(`  note: ${n}`);
		}
	} catch (e) {
		if (!(e instanceof CommandError)) throw e;
		if (json) console.error(JSON.stringify({ error: e.message }));
		else console.error(`i18n:mv: ${e.message}`);
		process.exit(1);
	}
}
