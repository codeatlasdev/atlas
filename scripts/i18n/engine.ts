// The format-independent rules. A format adapter (formats/*.ts) turns its files into a Surface of Entries; the
// rules here compare each locale with the source: missing, outdated, orphan, placeholders, plural forms,
// descriptions, glossary, pt-PT words and the lock. The same `entryIssues` validates what `i18n:translate` writes.
import { hash12 } from "./hash.ts";
import { type LockRead, readLocks } from "./lock.ts";
import { compareShapes, type Shape, sourceIssues } from "./shape.ts";
import { type Ctx, type Finding, type Severity, type StatusRow, tierSeverity } from "./types.ts";
import { textIssues } from "./glossary.ts";

export interface Entry {
	key: string;
	area: string;
	file: string;
	line?: number;
	/** What the lock hashes: the canonical form of the value, never the description. */
	source: string;
	/** What a translator reads and a writer stores (paraglide: the JSON value; fluent: the message source). */
	raw: string;
	shape: Shape;
	/** Human-readable text of every variant, for the glossary and pt-PT rules. */
	texts: string[];
	/** Translator description (English entries only). */
	description?: string;
	/** The one platform this English key is about (`# platform: mac`, i18n/platform-keys.json); it may name that platform's nouns without an `os` select. */
	platform?: Platform;
}

export type Platform = "mac" | "windows" | "linux";
export const PLATFORMS: readonly Platform[] = ["mac", "windows", "linux"];

export interface Surface {
	/** `desktop`, `site`, `rust`, ... also the directory under i18n/lock/. */
	name: string;
	kind: "paraglide" | "fluent";
	base: string;
	/** Every locale of the surface, base included. */
	locales: string[];
	entries: Map<string, Map<string, Entry>>;
	/** Read/parse problems the adapter found (bad JSON, junk, unsorted catalog ...). */
	problems: Finding[];
	keyPattern: RegExp;
	keyHint: string;
	/** Where a missing/absent key is reported when it has no file of its own. */
	catalogFile: (locale: string) => string;
}

export const hashOf = (e: Entry) => hash12(e.source);

const make = (rule: string, severity: Severity, file: string, message: string, key?: string, line?: number): Finding => ({
	rule,
	severity,
	file,
	line,
	key,
	message,
});

export interface Issue {
	rule: string;
	severity: Severity;
	message: string;
}

/** Modifier glyphs and spelled-out chords: a shortcut is a `{key}` placeholder filled by keys.ts kbd()/label(). */
const SHORTCUT = /[⌘⌃⌥⇧]|(?<![\p{L}])(?:Cmd|Command|Ctrl|Control|Shift|Alt|Option|Opt|Meta|Super)\s*\+/iu;
const NOUN = /(?<![\p{L}\p{N}_])(?:macOS|Mac|Finder|Windows|Explorer)(?![\p{L}\p{N}_])/gu;
const NOUNS_OF: Record<Platform, string[]> = { mac: ["Mac", "macOS", "Finder"], windows: ["Windows", "Explorer"], linux: [] };

/** Platform wording in one catalog value: shortcuts spelled out, and OS nouns where the message does not select on `os`. */
export function platformIssues(e: Entry, platform: Platform | undefined = e.platform): Issue[] {
	const out: Issue[] = [];
	const text = e.texts.join("\n").replace(/`[^`\n]*`/g, " ").replace(/https?:\/\/\S+/g, " ");
	const chord = SHORTCUT.exec(text);
	if (chord) out.push({ rule: "shortcut", severity: "error", message: `shortcut text "${chord[0]}" in a catalog value; use a {key} placeholder filled by keys.ts kbd()/label() (Rust: the shortcut helper)` });
	const selectsOs = e.shape.selectors.some((s) => s.name === "os");
	for (const noun of new Set(text.match(NOUN) ?? [])) {
		if (platform ? !NOUNS_OF[platform].includes(noun) : !selectsOs) {
			out.push({
				rule: "platform-noun",
				severity: "error",
				message: platform
					? `"${noun}" is not a ${platform} word, but the key is flagged ${platform}`
					: `"${noun}" names one platform but the message does not select on os (mac|windows|linux); select on os, or flag the key (Fluent: \`# platform: mac\`, Paraglide: i18n/platform-keys.json)`,
			});
		}
	}
	return out;
}

/** Shape and text rules of one translated entry against its source (no lock, no file layout). */
export function entryIssues(ctx: Ctx, locale: string, base: Entry, target: Entry): Issue[] {
	const issues: Issue[] = compareShapes(base.shape, target.shape, locale).map((i) => ({ ...i, severity: "error" as const }));
	if (target.texts.every((t) => t.trim() === "")) issues.push({ rule: "missing", severity: "error", message: "the translation is empty" });
	issues.push(...platformIssues(target, base.platform));
	issues.push(
		...textIssues({ glossary: ctx.glossary, locale, source: base.texts.join("\n"), target: target.texts.join("\n"), description: base.description }),
	);
	return issues;
}

/** Problems of the source entry itself, whatever the locales do. */
export function sourceEntryIssues(surface: Surface, base: Entry): Issue[] {
	const out: Issue[] = [];
	if (!surface.keyPattern.test(base.key)) out.push({ rule: "key", severity: "error", message: `key must look like ${surface.keyHint}` });
	if (base.description === undefined || base.description.trim() === "") {
		out.push({ rule: "description", severity: "error", message: "every source key needs a description (what UI element it is, and any disambiguation)" });
	} else {
		const d = base.description.trim().toLowerCase();
		if (base.texts.some((t) => t.trim().toLowerCase() === d)) out.push({ rule: "description", severity: "error", message: "the description only repeats the text" });
	}
	for (const i of sourceIssues(base.shape, surface.base)) out.push({ ...i, severity: "error" });
	out.push(...platformIssues(base));
	return out;
}

export function checkSurface(ctx: Ctx, surface: Surface): Finding[] {
	const out: Finding[] = [...surface.problems];
	const base = surface.entries.get(surface.base) ?? new Map<string, Entry>();
	for (const e of base.values()) {
		for (const i of sourceEntryIssues(surface, e)) out.push(make(i.rule, i.severity, e.file, i.message, e.key, e.line));
	}
	for (const locale of surface.locales.filter((l) => l !== surface.base)) {
		const sev = tierSeverity(ctx, locale);
		const mine = surface.entries.get(locale) ?? new Map<string, Entry>();
		const lock = readLocks(ctx.root, surface.name, locale);
		for (const p of lock.problems) out.push(make("lock", "error", p.file, p.message, p.key));
		for (const [key, b] of base) {
			const t = mine.get(key);
			if (!t) {
				out.push(make("missing", sev, surface.catalogFile(locale), `no ${locale} translation of "${b.texts[0] ?? ""}"`, key));
				continue;
			}
			for (const i of entryIssues(ctx, locale, b, t)) out.push(make(i.rule, i.severity, t.file, i.message, key, t.line));
			const l = lock.byKey.get(key);
			if (!l) out.push(make("unstamped", sev, t.file, "no lock entry: run bun run i18n:fix to stamp it", key, t.line));
			else if (l.hash !== hashOf(b)) {
				out.push(make("outdated", sev, t.file, "the English text changed since this was translated; retranslate it (bun run i18n:translate) or, for a typo-only edit, bun run i18n:fix --restamp " + key, key, t.line));
			} else if (l.area !== b.area) out.push(make("lock", "error", l.file, `locked under area "${l.area}", expected "${b.area}"; run bun run i18n:fix`, key));
		}
		for (const [key, t] of mine) if (!base.has(key)) out.push(make("orphan", "error", t.file, `not in the ${surface.base} source`, key, t.line));
		for (const [key, l] of lock.byKey) if (!base.has(key) || !mine.has(key)) out.push(make("lock", "error", l.file, `stale lock entry (the key is not in both ${surface.base} and ${locale}); run bun run i18n:fix`, key));
	}
	return out;
}

export function statusSurface(ctx: Ctx, surface: Surface): StatusRow[] {
	const rows: StatusRow[] = [];
	const base = surface.entries.get(surface.base) ?? new Map<string, Entry>();
	for (const locale of surface.locales.filter((l) => l !== surface.base)) {
		const mine = surface.entries.get(locale) ?? new Map<string, Entry>();
		const lock: LockRead = readLocks(ctx.root, surface.name, locale);
		const byArea = new Map<string, StatusRow>();
		for (const [key, b] of base) {
			const row = byArea.get(b.area) ?? { surface: surface.name, locale, area: b.area, total: 0, fresh: 0, outdated: 0, missing: 0 };
			byArea.set(b.area, row);
			row.total++;
			if (!mine.has(key)) row.missing++;
			else if (lock.byKey.get(key)?.hash === hashOf(b)) row.fresh++;
			else row.outdated++;
		}
		rows.push(...[...byArea.values()].sort((a, b) => (a.area < b.area ? -1 : 1)));
	}
	return rows;
}
