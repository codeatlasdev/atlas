// The catalog model: what the source tree (source.ts) loads and every rule, generator and command reads.
import type { MsgValue } from "./resolve.ts";
import type { Shape } from "./shape.ts";
import type { Finding } from "./types.ts";

export type Platform = "mac" | "windows" | "linux";
export const PLATFORMS: readonly Platform[] = ["mac", "windows", "linux"];

/** One message of one locale of one area. */
export interface Entry {
	key: string;
	area: string;
	/** Repo-relative path of the area file it was read from, and its 1-based line. */
	file: string;
	line?: number;
	/** The value as written (it may contain `{@ref}`s). */
	value: MsgValue;
	/** What the lock hashes: `JSON.stringify(value)`, never the description. */
	source: string;
	/** What a translator reads and a writer stores (the same JSON text). */
	raw: string;
	/** The value with every `{@ref}` inlined: what the generators emit and the rules look at. */
	resolved: MsgValue;
	/** Why the references could not be inlined (the message is then checked as written). */
	refError?: string;
	shape: Shape;
	/** Human-readable text of every variant of the resolved message, for the glossary and pt-PT rules. */
	texts: string[];
	/** Translator description (English entries only, from meta.json). */
	description?: string;
	platform?: Platform;
	/** Why this English text may equal a common one (meta `distinct` or `i18n-distinct:` in the description). */
	distinct?: string;
	maxLength?: number;
	/** Why this common message may be a sentence (meta `sentence`). */
	sentence?: string;
}

export interface MetaEntry {
	description: string;
	platform?: Platform;
	distinct?: string;
	maxLength?: number;
	sentence?: string;
}

/** One folder of i18n/messages/: the messages of one area in every locale, their meta and locks. */
export interface Area {
	name: string;
	/** `i18n/messages/<name>` */
	dir: string;
	base: string;
	/** Every locale of i18n/locales.json, base included. */
	locales: string[];
	entries: Map<string, Map<string, Entry>>;
	meta: Map<string, MetaEntry>;
	/** Read and parse problems (bad JSON, bad value, unsorted file, meta that does not match). */
	problems: Finding[];
	/** The command that regenerates the English side of this area, when it is generated (surfaces.json). */
	generated?: string;
}

export const areaFile = (area: Area | { dir: string }, locale: string) => `${area.dir}/${locale}.json`;
