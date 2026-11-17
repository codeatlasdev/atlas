import { getCollection, type CollectionEntry } from "astro:content";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { docState, gitBlobSha, type DocState } from "./doc-translation";
import { docPath, type Locale } from "./i18n";

const REPO = "https://github.com/codeatlasdev/atlas";

export type DocEntry = CollectionEntry<"docs">;
export type TranslationEntry = CollectionEntry<"docsPtBr">;
export interface NavGroup {
	title: string;
	items: { slug: string; title: string }[];
}
/** One docs page as pt-BR readers see it: the English entry, its translation if any, and whether that is current. */
export interface PageTranslation {
	translation?: TranslationEntry;
	state: DocState;
}

/** The page title: `title:` frontmatter, else the first `# Heading` of the markdown. */
export function titleOf(entry: DocEntry): string {
	return entry.data.title ?? entry.body?.match(/^#\s+(.+)$/m)?.[1]?.trim() ?? entry.id;
}

export function editUrl(entry: DocEntry | TranslationEntry): string {
	const dir = "source" in entry.data ? "docs/pt-BR" : "docs";
	return `${REPO}/edit/main/${dir}/${entry.id}.md`;
}

const docsFile = (name: string) => resolve(process.cwd(), "../docs", name);

/**
 * Every English slug with its pt-BR translation state. Fails the build when a translation does not belong to an English page:
 * `source` must be `<slug>.md` of its own file name and that English file must exist.
 */
export async function loadTranslations(entries: DocEntry[]): Promise<Map<string, PageTranslation>> {
	const translations = new Map((await getCollection("docsPtBr")).map((t) => [t.id, t]));
	for (const t of translations.values()) {
		if (t.data.source !== `${t.id}.md`) throw new Error(`docs/pt-BR/${t.id}.md: source is "${t.data.source}", expected "${t.id}.md"`);
		if (!entries.some((e) => e.id === t.id)) throw new Error(`docs/pt-BR/${t.id}.md translates docs/${t.data.source}, which does not exist`);
	}
	return new Map(
		entries.map((e) => {
			const translation = translations.get(e.id);
			const current = gitBlobSha(readFileSync(docsFile(`${e.id}.md`)));
			return [e.id, { translation, state: docState(translation?.data, current) }];
		}),
	);
}

/**
 * Sidebar groups from docs/_nav.json. Fails the build when the nav and the files disagree. Item titles are the translation's
 * `title` for pt-BR when the page is translated. Group titles stay English for now (they are not in the message catalog yet).
 */
export async function loadNav(locale: Locale = "en"): Promise<{ groups: NavGroup[]; entries: DocEntry[]; translations: Map<string, PageTranslation> }> {
	const entries = await getCollection("docs");
	const translations = await loadTranslations(entries);
	const byId = new Map(entries.map((e) => [e.id, e]));
	const raw = JSON.parse(readFileSync(docsFile("_nav.json"), "utf8")) as {
		title: string;
		items: string[];
	}[];
	const listed = new Set<string>();
	const groups = raw.map((g) => ({
		title: g.title,
		items: g.items.map((slug) => {
			const e = byId.get(slug);
			if (!e) throw new Error(`docs/_nav.json lists "${slug}" but docs/${slug}.md does not exist`);
			listed.add(slug);
			const translated = locale === "en" ? undefined : translations.get(slug)?.translation;
			return { slug, title: translated?.data.title ?? titleOf(e) };
		}),
	}));
	const orphans = entries.filter((e) => !listed.has(e.id)).map((e) => e.id);
	if (orphans.length) throw new Error(`docs/_nav.json is missing: ${orphans.join(", ")}`);
	return { groups, entries, translations };
}

/** The /pt-br/ and English URLs of a docs page that exist as real translations (feeds hreflang and the language switch). */
export function docAlternates(slug: string, state: DocState): Partial<Record<Locale, string>> | undefined {
	return state === "missing" ? undefined : { en: docPath(slug, "en"), "pt-BR": docPath(slug, "pt-BR") };
}
