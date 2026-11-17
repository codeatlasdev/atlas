// Locale and URL rules of the site, pure (no `astro:` or Paraglide imports) so astro.config.mjs, the pages and bun test
// share them. Canonical tags are BCP 47 (`en`, `pt-BR`, same as the inlang project); the URL segment of a locale is the
// `path` of i18n.locales in astro.config.mjs and must stay in sync with URL_SEGMENT.

export const LOCALES = ["en", "pt-BR"] as const;
export type Locale = (typeof LOCALES)[number];
export const BASE_LOCALE: Locale = "en";
export const URL_SEGMENT: Record<Locale, string> = { en: "", "pt-BR": "pt-br" };

/** `Astro.currentLocale` ("pt-br", "pt-BR", "pt", "en", undefined) to a canonical locale. Anything unknown is English. */
export function localeOfAstro(current: string | undefined): Locale {
	return current?.toLowerCase().startsWith("pt") ? "pt-BR" : BASE_LOCALE;
}

/** "/docs/x/" in a locale: English is unprefixed, every other locale gets its URL segment. */
export function localizePath(path: string, locale: Locale): string {
	const seg = URL_SEGMENT[locale];
	if (!seg) return path;
	return path === "/" ? `/${seg}/` : `/${seg}${path}`;
}

/** The inverse: which locale a pathname belongs to, and its path without the locale segment. */
export function delocalizePath(pathname: string): { locale: Locale; path: string } {
	for (const locale of LOCALES) {
		const seg = URL_SEGMENT[locale];
		if (seg && (pathname === `/${seg}` || pathname.startsWith(`/${seg}/`))) return { locale, path: pathname.slice(seg.length + 1) || "/" };
	}
	return { locale: BASE_LOCALE, path: pathname };
}

export const docPath = (slug: string, locale: Locale): string => localizePath(`/docs/${slug}/`, locale);

export interface HreflangLink {
	hreflang: string;
	href: string;
}

/**
 * `<link rel="alternate" hreflang>` set for a page. `paths` holds ONLY the locales in which the page really exists (a fallback
 * page that shows English under /pt-br/ is not one of them). A page that exists in one locale gets none: an alternate list with
 * only the page itself is noise. `x-default` is the English page.
 */
export function hreflangLinks(paths: Partial<Record<Locale, string>>, origin: string): HreflangLink[] {
	const present = LOCALES.filter((l) => paths[l] !== undefined);
	if (present.length < 2) return [];
	const url = (l: Locale) => new URL(paths[l]!, origin).href;
	const links: HreflangLink[] = present.map((l) => ({ hreflang: l, href: url(l) }));
	if (paths[BASE_LOCALE] !== undefined) links.push({ hreflang: "x-default", href: url(BASE_LOCALE) });
	return links;
}

/**
 * Where the language switch in the nav points, or undefined for no switch. From English: the pt-BR page, only when it is a real
 * translation (`alternates` has it). From a non-English page: the English equivalent (a fallback page has no `alternates` but its
 * canonical path IS the English page).
 */
export function switchTarget(locale: Locale, alternates: Partial<Record<Locale, string>> | undefined, canonicalPath: string | undefined): { locale: Locale; path: string } | undefined {
	if (locale === BASE_LOCALE) {
		const path = alternates?.["pt-BR"];
		return path === undefined ? undefined : { locale: "pt-BR", path };
	}
	const path = alternates?.[BASE_LOCALE] ?? canonicalPath;
	return path === undefined ? undefined : { locale: BASE_LOCALE, path };
}

/**
 * Whether a built page belongs in sitemap.xml. English pages always do. Under /pt-br/ only translated docs pages do: a fallback page
 * repeats the English body (duplicate content, canonical to English) and Astro's own fallbacks are redirects.
 */
export function inSitemap(pathname: string, translatedSlugs: ReadonlySet<string>): boolean {
	const { locale, path } = delocalizePath(pathname);
	if (locale === BASE_LOCALE) return true;
	const slug = path.match(/^\/docs\/([^/]+)\/$/)?.[1];
	return slug !== undefined && translatedSlugs.has(slug);
}

/** The sitemap integration emits one alternate per locale; add `x-default` (the English page) whenever English is one of them. */
export function withXDefault<L extends { url: string; lang: string }>(links: L[] | undefined): L[] | undefined {
	const en = links?.find((l) => l.lang === BASE_LOCALE);
	if (!links || !en || links.some((l) => l.lang === "x-default")) return links;
	return [...links, { ...en, lang: "x-default" }];
}
