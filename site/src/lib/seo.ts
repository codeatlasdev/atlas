// schema.org JSON-LD builders. Pure: pages pass the words in their language, nothing here reads Astro or the catalog.
import { SITE } from "../data/site";
import type { Locale } from "./i18n";

const SCHEMA = "https://schema.org";
const abs = (path: string) => new URL(path, SITE.url).href;

/** `@id` of the organization and of the web site: every page of the site names the same two nodes. */
export const ORG_ID = `${SITE.url}/#organization`;
export const SITE_ID = `${SITE.url}/#website`;

/** The graph every page carries: who publishes the site and the site itself, in the language of the page. */
export function siteGraph(locale: Locale) {
	return {
		"@context": SCHEMA,
		"@graph": [
			{
				"@type": "Organization",
				"@id": ORG_ID,
				name: SITE.name,
				url: SITE.url,
				logo: abs("/apple-touch-icon.png"),
				sameAs: [SITE.github, SITE.codeatlas],
			},
			{ "@type": "WebSite", "@id": SITE_ID, url: SITE.url, name: SITE.name, inLanguage: locale, publisher: { "@id": ORG_ID } },
		],
	};
}

/** A trail of `{ name, path }`, first to last (the last is the page itself). */
export function breadcrumbLd(items: { name: string; path: string }[]) {
	return {
		"@context": SCHEMA,
		"@type": "BreadcrumbList",
		itemListElement: items.map((item, i) => ({ "@type": "ListItem", position: i + 1, name: item.name, item: abs(item.path) })),
	};
}

export interface TechArticleInput {
	headline: string;
	description?: string;
	locale: Locale;
	/** The page's own path. */
	path: string;
	/** Path of its share image. */
	image: string;
	/** ISO date of the last change of the source file; left out when unknown. */
	dateModified?: string;
}

export function techArticleLd(a: TechArticleInput) {
	return {
		"@context": SCHEMA,
		"@type": "TechArticle",
		headline: a.headline,
		...(a.description ? { description: a.description } : {}),
		inLanguage: a.locale,
		url: abs(a.path),
		mainEntityOfPage: abs(a.path),
		image: abs(a.image),
		...(a.dateModified ? { dateModified: a.dateModified } : {}),
		isPartOf: { "@id": SITE_ID },
		publisher: { "@id": ORG_ID },
	};
}
