// The per-area rules: for one folder of i18n/messages/ compare each locale with the English source: missing, outdated,
// orphan, placeholders, plural forms, descriptions, glossary, pt-PT words, platform wording and the lock. The same
// `entryIssues` validates what `i18n:translate` and `i18n:add` write. Rules that look across areas (budget,
// dup-common, unused ...) are in checks/messages.ts.
import { hash12 } from "./hash.ts";
import { readLock, lockFile } from "./lock.ts";
import { type Area, type Entry, type Platform, PLATFORMS, areaFile } from "./model.ts";
import { compareShapes, sourceIssues } from "./shape.ts";
import { areaOfKey, isCommon, KEY_PATTERN } from "./source.ts";
import { type Ctx, type Finding, type Severity, type StatusRow, tierSeverity } from "./types.ts";
import { textIssues } from "./glossary.ts";

export type { Entry, Platform };
export { PLATFORMS };

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

/** Platform wording in one message: shortcuts spelled out, and OS nouns where the message does not select on `os`. */
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
					: `"${noun}" names one platform but the message does not select on os (mac|windows|linux); select on os (or reference a common_os_* term), or flag the key (meta.json: "platform")`,
			});
		}
	}
	return out;
}

/** English quotes are straight (style/en.md "Mechanics"): a curly one in a source value is typed by a keyboard layout or a model, never by design. */
const CURLY = /[\u2018\u2019\u201C\u201D]/;

/** `maxLength` of a key (meta.json): no text of the message, placeholders aside, may be longer. */
export function lengthIssues(e: Entry, maxLength: number | undefined = e.maxLength): Issue[] {
	if (!maxLength) return [];
	const longest = Math.max(0, ...e.texts.map((t) => t.replace(/\{[^{}]*\}/g, "").length));
	return longest > maxLength ? [{ rule: "length", severity: "error", message: `${longest} characters, the limit is ${maxLength} (meta.json maxLength)` }] : [];
}

/** Shape and text rules of one translated entry against its source (no lock, no file layout). */
export function entryIssues(ctx: Ctx, locale: string, base: Entry, target: Entry): Issue[] {
	const issues: Issue[] = compareShapes(base.shape, target.shape, locale).map((i) => ({ ...i, severity: "error" as const }));
	if (target.texts.every((t) => t.trim() === "")) issues.push({ rule: "missing", severity: "error", message: "the translation is empty" });
	issues.push(...platformIssues(target, base.platform), ...lengthIssues(target, base.maxLength));
	issues.push(...textIssues({ glossary: ctx.glossary, locale, source: base.texts.join("\n"), target: target.texts.join("\n"), description: base.description }));
	return issues;
}

/** Problems of the source entry itself, whatever the locales do. */
export function sourceEntryIssues(base: Entry, areaNames: string[]): Issue[] {
	const out: Issue[] = [];
	if (!KEY_PATTERN.test(base.key)) out.push({ rule: "key", severity: "error", message: "key must be snake_case: an area, an underscore, a name (settings_display_language)" });
	else {
		const owner = areaOfKey(base.key, areaNames);
		if (owner !== base.area) {
			out.push({ rule: "key-area", severity: "error", message: owner ? `key belongs to area "${owner}" (its longest area prefix), not "${base.area}"` : `every key of area "${base.area}" starts with "${base.area}_"` });
		}
	}
	if (base.description === undefined || base.description.trim() === "") {
		out.push({ rule: "description", severity: "error", message: "every source key needs a description in meta.json (what UI element it is, and any disambiguation); bun run i18n:add writes it with --desc" });
	} else {
		const d = base.description.trim().toLowerCase();
		if (base.texts.some((t) => t.trim().toLowerCase() === d)) out.push({ rule: "description", severity: "error", message: "the description only repeats the text" });
	}
	for (const i of sourceIssues(base.shape, "en")) out.push({ ...i, severity: "error" });
	if (isCommon(base.area) && !base.sentence && base.texts.length) {
		const sentence = base.texts.find(looksLikeSentence);
		if (sentence !== undefined) out.push({ rule: "common-sentence", severity: "error", message: `common* areas hold terms (Cancel, Trash), not sentences: "${sentence.slice(0, 50)}" is more than 4 words or ends like a sentence; move it to the area of the screen that owns it, or justify it with meta "sentence": "<reason>"` });
	}
	out.push(...platformIssues(base), ...lengthIssues(base));
	const curly = CURLY.exec(base.texts.join("\n"));
	if (curly) out.push({ rule: "quotes", severity: "error", message: `English uses straight quotes (" and '), not ${curly[0]}: retype it and, if the translation still fits, bun run i18n:fix --restamp ${base.key}` });
	if (base.refError) out.push({ rule: "dangling-ref", severity: "error", message: base.refError });
	return out;
}

/** A text that reads as a sentence: more than four words, a sentence end inside, or a closing full stop. */
export function looksLikeSentence(text: string): boolean {
	const t = text.replace(/\{[^{}]*\}/g, "x").trim();
	return t.split(/\s+/).length > 4 || /[.?!]\s/.test(t) || t.endsWith(".");
}

/** What to do about a generated area's English side, appended to messages that would send someone to edit it by hand. */
const generatedHint = (area: Area) => (area.generated ? ` (the English of ${area.name} is generated: change it with \`${area.generated}\`, never by hand)` : "");

export function checkArea(ctx: Ctx, area: Area, areaNames: string[]): Finding[] {
	const out: Finding[] = [...area.problems];
	const base = area.entries.get(area.base) ?? new Map<string, Entry>();
	for (const e of base.values()) for (const i of sourceEntryIssues(e, areaNames)) out.push(make(i.rule, i.severity, e.file, i.message, e.key, e.line));
	for (const locale of area.locales.filter((l) => l !== area.base)) {
		const sev = tierSeverity(ctx, locale);
		const mine = area.entries.get(locale) ?? new Map<string, Entry>();
		const lock = readLock(ctx.root, area.name, locale);
		for (const p of lock.problems) {
			// a problem of the file itself is the same for every locale: say it once
			if (p.key || !out.some((f) => f.rule === "lock" && f.message === p.message)) out.push(make("lock", "error", lockFile(area.name), p.message, p.key));
		}
		for (const [key, b] of base) {
			const t = mine.get(key);
			if (!t) {
				out.push(make("missing", sev, areaFile(area, locale), `no ${locale} translation of "${b.texts[0] ?? ""}": run bun run i18n:translate --locale ${locale} --area ${area.name}, or write it in ${areaFile(area, locale)} and run bun run i18n:fix`, key));
				continue;
			}
			for (const i of entryIssues(ctx, locale, b, t)) out.push(make(i.rule, i.severity, t.file, i.message, key, t.line));
			const stamp = lock.byKey.get(key);
			if (!stamp) out.push(make("unstamped", sev, t.file, "no lock entry: run bun run i18n:fix to stamp it", key, t.line));
			else if (stamp !== hashOf(b)) {
				out.push(make("outdated", sev, t.file, `the English text changed since this was translated; retranslate it (bun run i18n:translate) or, for a typo-only edit, bun run i18n:fix --restamp ${key}${generatedHint(area)}`, key, t.line));
			}
		}
		for (const [key, t] of mine) if (!base.has(key)) out.push(make("orphan", "error", t.file, `not in the ${area.base} source of this area: delete it from ${areaFile(area, locale)}, or bun run i18n:mv <old-key> <new-key> if it was renamed${generatedHint(area)}`, key, t.line));
		for (const key of lock.byKey.keys()) {
			if (!base.has(key) || !mine.has(key)) out.push(make("lock", "error", lockFile(area.name), `stale lock entry (the key is not in both ${area.base} and ${locale}); run bun run i18n:fix`, key));
		}
	}
	return out;
}

export function statusArea(ctx: Ctx, area: Area, surface: string): StatusRow[] {
	const base = area.entries.get(area.base) ?? new Map<string, Entry>();
	return area.locales
		.filter((l) => l !== area.base)
		.map((locale) => {
			const mine = area.entries.get(locale) ?? new Map<string, Entry>();
			const lock = readLock(ctx.root, area.name, locale);
			const row: StatusRow = { surface, locale, area: area.name, total: base.size, fresh: 0, outdated: 0, missing: 0 };
			for (const [key, b] of base) {
				if (!mine.has(key)) row.missing++;
				else if (lock.byKey.get(key) === hashOf(b)) row.fresh++;
				else row.outdated++;
			}
			return row;
		});
}
