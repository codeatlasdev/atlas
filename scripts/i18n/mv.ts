// bun run i18n:mv <old_key> <new_key> [--new-area --surface s] [--json]
// Renames a key, moving it to the area its new name belongs to, in every locale: values, meta, lock stamps (a stale
// translation stays stale: nothing is re-stamped), `{@old}` references in other messages, and the call sites in the
// sources of every surface:
//   web   `m.old(`, `m?.old(`, `m["old"]`, `const { old: x } = m`, `import { old as x }`, through whatever name the file
//         binds the messages module to (`messageModule.ts`: the desktop's `#lib/i18n/m.ts`, Paraglide's own output, `* as ns`)
//   Rust  `t!("old"`, `t_in!(tag, "old"`, `lookup(tag, "old"`, `lookup_on(…)`, `translate(…)`
// Everything else that could still reach the old name is LISTED, not edited, and the command exits 1 with file:line:
// `unproven` (a mention of the old name that is no call: a string, a comment) and `dynamic` (a key built at run time, a
// bare named import or shorthand destructuring that would need a local rename, an `i18n-keys` glob).
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { CommandError, areaPath, readObject, removeKey, setKey, writeObject } from "./edit.ts";
import { fix } from "./fix.ts";
import { hash12 } from "./hash.ts";
import { readLock, writeLock } from "./lock.ts";
import { IMPORT, MESSAGE_MODULE, bracketHead, escapeRe, memberHead, messageBindings } from "./messageModule.ts";
import { areaOfKey, KEY_PATTERN, loadSource, writeAreaFile, type Source } from "./source.ts";
import { loadCtx, type Ctx } from "./types.ts";
import { ANNOTATION } from "./unused.ts";
import { globToRegExp, lineOf, readText, rel, walk } from "./util.ts";

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
	/** Mentions of the old name that were NOT edited and are no known call form: check them by hand. */
	unproven: Spot[];
	/** Places that may reach the old name without writing it (a key built at run time) or that need a local rename: fix them by hand. */
	dynamic: (Spot & { reason: string })[];
	notes: string[];
}

export interface MoveOptions {
	newArea?: boolean;
	surface?: string;
}

const WEB = /\.(?:svelte|ts|tsx|js|jsx|mjs|astro)$/;

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

/** The Rust calls that take a message key as a literal: `t!("k"`, `t_in!(tag, "k"`, `lookup(tag, "k"`. Ends right after the opening quote. */
const RUST_CALL = String.raw`\b(?:t!\(|(?:t_in!|lookup_on|lookup|translate)\(\s*(?:[^,";()]|\([^()]*\))+,)\s*"`;
/** A const / let destructuring of the messages namespace `space`: groups 1 (head), 2 (entries), 3 (tail). */
const destructure = (space: string) => new RegExp(String.raw`((?:const|let|var)\s*\{)([^}]*)(\}\s*=\s*${escapeRe(space)}(?![\w$.\[(]))`, "g");

/** Rewrite the provable uses of `oldKey` in one source text; the new text and the lines edited. */
export function rewriteUses(text: string, file: string, oldKey: string, newKey: string): { text: string; lines: number[] } {
	const lines: number[] = [];
	const key = escapeRe(oldKey);
	let out = text;
	/** Replace every match of `re`; `to` returns the new text of a match, or undefined to keep it. */
	const sub = (re: RegExp, to: (m: string[]) => string | undefined) => {
		const src = out;
		// replace's arguments: the match, its groups, the offset, the whole text
		out = src.replace(re, (...a) => {
			const m = a.slice(0, -2) as string[];
			const next = to(m);
			if (next === undefined || next === m[0]) return m[0];
			lines.push(lineOf(src, a.at(-2) as number) + (m[0].slice(0, m[0].indexOf(oldKey)).match(/\n/g)?.length ?? 0));
			return next;
		});
	};
	if (file.endsWith(".rs")) sub(new RegExp(`(${RUST_CALL})${key}(")`, "g"), (m) => `${m[1]}${newKey}${m[2]}`);
	else {
		const { spaces } = messageBindings(text);
		for (const space of spaces) {
			sub(new RegExp(`(${memberHead(space)})${key}(?![\\w$])`, "g"), (m) => `${m[1]}${newKey}`);
			sub(new RegExp(`(${bracketHead(space)})${key}(["'\`]\\s*\\])`, "g"), (m) => `${m[1]}${newKey}${m[2]}`);
			// `const { old: x } = m` keeps its local name; the shorthand `{ old }` is left to `dynamicUses`
			sub(destructure(space), (m) => `${m[1]}${m[2].replace(new RegExp(`(?<![\\w$])${key}(?=\\s*:)`, "g"), newKey)}${m[3]}`);
		}
		// `import { old as x } from <messages>`: the local name stays
		sub(new RegExp(IMPORT), (m) => (MESSAGE_MODULE.test(m[2]) ? m[0].replace(new RegExp(`(?<![\\w$])${key}(?=\\s+as\\s)`), newKey) : undefined));
	}
	return { text: out, lines: [...new Set(lines)].sort((a, b) => a - b) };
}

/** Lines that still mention `key` as a whole word. */
export function mentions(text: string, key: string): number[] {
	const re = new RegExp(`(?<![A-Za-z0-9_])${escapeRe(key)}(?![A-Za-z0-9_])`);
	return text.split("\n").flatMap((l, i) => (re.test(l) ? [i + 1] : []));
}

/** `text` with everything but newlines of each match blanked, so offsets and line numbers still hold. */
const blank = (text: string, re: RegExp) => text.replace(re, (m) => m.replace(/[^\n]/g, " "));

/** The content of the `[ … ]` that opens at `open` (text[open] === "["), or undefined when it never closes. */
function bracketContent(text: string, open: number): string | undefined {
	let depth = 0;
	for (let i = open; i < text.length; i++) {
		if (text[i] === "[") depth++;
		else if (text[i] === "]" && --depth === 0) return text.slice(open + 1, i);
	}
	return undefined;
}

/**
 * What may still reach `oldKey` after `rewriteUses` without naming it, or needs a rename of a local identifier that no tool
 * can prove safe. Each is a place a person must read; the reason says what to do.
 */
export function dynamicUses(text: string, file: string, oldKey: string): { line: number; reason: string }[] {
	const out: { line: number; reason: string }[] = [];
	const add = (index: number, reason: string) => out.push({ line: lineOf(text, index), reason });
	for (const m of text.matchAll(ANNOTATION)) {
		for (const g of m[1].split(/[\s,]+/).filter((x) => x.includes("*") && globToRegExp(x).test(oldKey))) {
			add(m.index ?? 0, `the annotation i18n-keys: ${g} covers ${oldKey}: change the glob, or leave the family and rename every sibling`);
		}
	}
	if (file.endsWith(".rs")) {
		if (/\b(?:lookup_on|lookup|translate)\(/.test(text)) {
			for (const m of text.matchAll(/\bformat!\(\s*"([^"{}\\]+)\{/g)) {
				if (oldKey.startsWith(m[1])) add(m.index ?? 0, `format!("${m[1]}…") builds keys that start like ${oldKey} next to a lookup: annotate it with i18n-keys or rename the generator`);
			}
		}
		return sorted(out);
	}
	const { spaces, named } = messageBindings(text);
	if (spaces.length === 0 && named.length === 0) return sorted(out);
	let code = text;
	for (const m of text.matchAll(IMPORT)) {
		if (!MESSAGE_MODULE.test(m[2])) continue;
		const bare = (/\{([^}]*)\}/.exec(m[1])?.[1] ?? "").split(",").some((p) => p.trim() === oldKey);
		if (bare) add(m.index ?? 0, `${oldKey} is imported by name: rename the import and every use of it by hand (or import it as an alias)`);
		code = blank(code, new RegExp(escapeRe(m[0])));
	}
	for (const space of spaces) {
		for (const m of text.matchAll(destructure(space))) {
			const at = (m.index ?? 0) + m[1].length;
			for (const entry of m[2].split(",").map((e) => e.trim())) {
				if (entry.startsWith("...")) add(at, `the rest of a destructured ${space} takes every message: list the keys you need`);
				else if (entry.split(/[:=]/)[0].trim() === oldKey && !entry.includes(":")) add(at + m[2].indexOf(oldKey), `{ ${oldKey} } destructures it by its own name: write { <new key>: ${oldKey} } or rename the local by hand`);
			}
		}
		code = blank(code, destructure(space));
		// a local `m` (`const m = /x/.exec(…)`, `(m) => …`) is no message module: its `m[1]` says nothing about keys
		if (new RegExp(`\\b(?:const|let|var)\\s+${escapeRe(space)}\\b|\\(\\s*${escapeRe(space)}\\s*(?:,[^)]*)?\\)\\s*=>|\\b${escapeRe(space)}\\s*=>`).test(code)) continue;
		for (const m of code.matchAll(new RegExp(`(?<![\\w$.])${escapeRe(space)}\\??\\.?\\[`, "g"))) {
			const inner = bracketContent(code, (m.index ?? 0) + m[0].length - 1);
			if (inner === undefined || /^\s*(["'])[^"'`]*\1\s*$/.test(inner) || /^\s*`[^`$]*`\s*$/.test(inner)) continue;
			// `m[`app_${x}`]` and `m["app_" + x]`: the literal start is the only thing known about the key
			const prefix = /^\s*(?:`([^`$]*)\$\{|(["'])([^"']*)\2\s*\+)/.exec(inner);
			const start = prefix ? (prefix[1] ?? prefix[3]) : undefined;
			if (start !== undefined && !oldKey.startsWith(start)) continue;
			// a test that walks the whole catalog (`for (const k of keys) m[k]`) follows every rename by itself
			if (start === undefined && /\.test\.[jt]sx?$/.test(file)) continue;
			add(m.index ?? 0, start === undefined ? `${space}[…] with a computed key can reach any message: use static ${space}.key calls (a switch)` : `${space}[${start}…] builds keys that start like ${oldKey}: use static ${space}.key calls (a switch)`);
		}
		// (the namespace passed around as a value is not reported: a table of key names in the code is a literal mention)
	}
	return sorted(out);
}

const sorted = (xs: { line: number; reason: string }[]) => [...new Map(xs.sort((a, b) => a.line - b.line).map((x) => [`${x.line}:${x.reason}`, x])).values()];

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
	const report: MoveReport = { from: oldKey, to: newKey, fromArea: from.name, toArea: toName, updated: [], unproven: [], dynamic: [], notes: [] };

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
		const dynamic = dynamicUses(r.text, file, oldKey);
		report.dynamic.push(...dynamic.map((d) => ({ file, ...d })));
		const explained = new Set(dynamic.map((d) => d.line));
		for (const line of mentions(r.text, oldKey)) if (!explained.has(line)) report.unproven.push({ file, line });
	}
	fix(loadCtx(ctx.root));
	return report;
}

/** What is left to do by hand, one `file:line why` per place. */
export function leftovers(r: MoveReport): string[] {
	return [...r.dynamic.map((d) => `${d.file}:${d.line} ${d.reason}`), ...r.unproven.map((u) => `${u.file}:${u.line} still mentions ${r.from}`)].sort();
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
			for (const n of r.notes) console.log(`  note: ${n}`);
		}
		const left = leftovers(r);
		if (left.length) {
			const head = `${left.length} reference${left.length === 1 ? "" : "s"} to ${r.from} not rewritten (the catalogs and every call site it could prove are moved); edit these by hand, then bun run i18n:check`;
			if (json) console.error(JSON.stringify({ error: `${head}: ${left.join("; ")}` }));
			else console.error(`i18n:mv: ${head}:\n${left.map((l) => `  ${l}`).join("\n")}`);
			process.exit(1);
		}
	} catch (e) {
		if (!(e instanceof CommandError)) throw e;
		if (json) console.error(JSON.stringify({ error: e.message }));
		else console.error(`i18n:mv: ${e.message}`);
		process.exit(1);
	}
}
