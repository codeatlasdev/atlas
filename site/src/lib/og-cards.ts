// Every share image of the site and the words on it. Build time only (reads the docs collection and the message catalog); the
// pure mapping from a page to its image is in og.ts.
import { m } from "./paraglide/messages.js";
import { loadNav, titleOf } from "./docs";
import { LOCALES, platformList, type Locale } from "./i18n";
import { ogKeyOf, ogSegment, type OgCard } from "./og";
import { SITE } from "../data/site";

export interface OgEntry {
	locale: Locale;
	key: string;
	card: OgCard;
}

const host = SITE.url.replace(/^https?:\/\//, "");

/** The card of each page that is not a docs page: [path, title, subtitle, kicker, ground]. */
function pageCards(locale: Locale): [string, string, string, string | undefined, OgCard["ground"]][] {
	const o = { locale };
	return [
		["/", m.home_hero_title({}, o), m.seo_og_home_subtitle({}, o), undefined, "cobalt"],
		["/download/", m.page_download_h1({}, o), m.seo_og_download_subtitle({ platforms: platformList(locale) }, o), undefined, "cobalt"],
		["/changelog/", m.nav_changelog({}, o), m.seo_og_changelog_subtitle({}, o), undefined, "paper"],
		["/privacy/", m.footer_privacy({}, o), m.seo_og_privacy_subtitle({}, o), undefined, "paper"],
		["/code-signing-policy/", m.footer_code_signing({}, o), m.seo_og_signing_subtitle({}, o), undefined, "paper"],
	];
}

/**
 * One image per page and language. pt-BR gets docs images only for real translations: an untranslated /pt-br/docs/ page is a copy of the
 * English one and its `og:image` is the English image (Base derives it from the canonical path).
 */
export async function ogEntries(): Promise<OgEntry[]> {
	const { entries, translations } = await loadNav();
	const out: OgEntry[] = [];
	for (const locale of LOCALES) {
		const o = { locale };
		const footer = m.seo_og_footer({}, o);
		const prefix = `${host}${locale === "en" ? "" : `/${ogSegment(locale)}`}`;
		for (const [path, title, subtitle, kicker, ground] of pageCards(locale)) {
			out.push({ locale, key: ogKeyOf(path), card: { title, subtitle, kicker, footer, ground, url: `${prefix}${path === "/" ? "" : path.slice(0, -1)}` } });
		}
		for (const entry of entries) {
			const translation = translations.get(entry.id)?.translation;
			if (locale !== "en" && !translation) continue;
			const title = (locale === "en" ? undefined : translation?.data.title) ?? entry.data.title ?? titleOf(entry);
			const subtitle = (locale === "en" ? entry.data.description : undefined) ?? m.seo_og_docs_subtitle({}, o);
			out.push({ locale, key: `docs/${entry.id}`, card: { title, subtitle, kicker: m.nav_docs({}, o), footer, ground: "paper", url: `${prefix}/docs/${entry.id}` } });
		}
	}
	return out;
}
