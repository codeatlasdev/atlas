// Fluent catalogs of the Rust surface: crates/<crate>/locales/<lang>/<area>.ftl. The translator description of a
// message is the `#` comment block directly above it; the area is the file name.
import { existsSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";
import { FluentParser, FluentSerializer, Junk, Message, type Resource } from "@fluent/syntax";
import { type Entry, type Platform, PLATFORMS, type Surface } from "../engine.ts";
import { fluentShape, fluentTexts } from "../shape.ts";
import type { Ctx, Finding } from "../types.ts";
import { LOCALES_FILE } from "../types.ts";
import { lineOf, readText, rel } from "../util.ts";

export const KEY_PATTERN = /^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/;

const parser = new FluentParser({ withSpans: true });
const serializer = new FluentSerializer();

/** `<crate>/locales/<lang>/<area>.ftl` files of the workspace: [locale, absolute path] pairs. */
export function ftlFiles(root: string): { locale: string; path: string }[] {
	const out: { locale: string; path: string }[] = [];
	const crates = join(root, "crates");
	if (!existsSync(crates)) return out;
	for (const c of readdirSync(crates).sort()) {
		const locales = join(crates, c, "locales");
		if (!existsSync(locales)) continue;
		for (const lang of readdirSync(locales).sort()) {
			const dir = join(locales, lang);
			if (!existsSync(dir) || !readdirSync(dir).length) continue;
			for (const f of readdirSync(dir).filter((n) => n.endsWith(".ftl")).sort()) out.push({ locale: lang, path: join(dir, f) });
		}
	}
	return out;
}

/** The canonical source of a message (value + attributes, no comment, layout normalized): what the lock hashes
 * and what a translation is written as. */
export const messageSource = (m: Message): string => serializer.serializeEntry(new Message(m.id, m.value, m.attributes, null)).trim();

const platformOf = (comment: string | undefined): Platform | undefined => {
	const p = comment === undefined ? undefined : /^\s*platform:\s*(mac|windows|linux)\s*$/m.exec(comment)?.[1];
	return PLATFORMS.find((x) => x === p);
};

export function entryOf(m: Message, file: string, area: string, text: string): Entry {
	return {
		key: m.id.name,
		area,
		file,
		line: lineOf(text, m.id.span?.start ?? 0),
		source: messageSource(m),
		raw: messageSource(m),
		shape: fluentShape(m),
		texts: fluentTexts(m),
		description: m.comment?.content.trim(),
		platform: platformOf(m.comment?.content),
	};
}

/** Parse one message written by a translator (or a model): exactly one Message and no junk, or the reason it is not. */
export function parseMessage(source: string): { message: Message } | { error: string } {
	const r: Resource = parser.parse(source);
	const junk = r.body.find((e): e is Junk => e instanceof Junk);
	if (junk) return { error: `Fluent syntax error: ${junk.annotations[0]?.message ?? "junk"}` };
	const msgs = r.body.filter((e): e is Message => e.type === "Message");
	if (msgs.length !== 1) return { error: `expected exactly one message, found ${msgs.length}` };
	return { message: msgs[0] };
}

export function loadFluent(ctx: Ctx): Surface | undefined {
	const { root } = ctx;
	const files = ftlFiles(root);
	if (files.length === 0) return undefined;
	const base = ctx.locales.default;
	const known = existsSync(join(root, LOCALES_FILE));
	const locales = [base, ...new Set([...Object.keys(ctx.locales.locales), ...files.map((f) => f.locale)].filter((l) => l !== base))];
	const problems: Finding[] = [];
	const entries = new Map<string, Map<string, Entry>>(locales.map((l) => [l, new Map()]));
	for (const { locale, path } of files) {
		const file = rel(root, path);
		const area = basename(path, ".ftl");
		if (known && !ctx.locales.locales[locale]) problems.push({ rule: "config", severity: "error", file, message: `locale directory "${locale}" is not in ${LOCALES_FILE}` });
		const text = readText(path);
		const map = entries.get(locale);
		if (!map) continue;
		for (const e of parser.parse(text).body) {
			if (e.type === "Junk") {
				const a = e.annotations[0];
				problems.push({ rule: "syntax", severity: "error", file, line: lineOf(text, e.span?.start ?? 0), message: `Fluent ${a?.code ?? "E????"}: ${a?.message ?? "invalid entry"}` });
			} else if (e.type === "Message") {
				const entry = entryOf(e, file, area, text);
				const dup = map.get(entry.key);
				if (dup) problems.push({ rule: "unique", severity: "error", file, line: entry.line, key: entry.key, message: `id is also defined at ${dup.file}:${dup.line}` });
				else map.set(entry.key, entry);
			}
		}
	}
	return {
		name: "rust",
		kind: "fluent",
		base,
		locales,
		entries,
		problems,
		keyPattern: KEY_PATTERN,
		keyHint: "kebab-case (lowercase words joined by hyphens, e.g. cli-help-about)",
		catalogFile: (l) => `crates/*/locales/${l}`,
	};
}
