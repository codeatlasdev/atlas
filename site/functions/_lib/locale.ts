// Which language a visitor gets when they open an English URL that has a Portuguese twin. Pure, so bun test pins every rule.
//
// Order: an explicit choice wins (`?lang=` on this request, then the `atlas_lang` cookie the language switch writes), then where the
// visitor is (Cloudflare's country for the request IP: in a Portuguese-speaking country the site opens in pt-BR even when the browser is
// set to English, because that is what most people there expect), then the browser's languages (Accept-Language, by q), then English.
// Crawlers are never redirected: each language has its own URL with hreflang alternates, and search engines must see the English page
// at the English URL. A redirect only ever lands on the pt-BR twin, or on the same English URL without `?lang=`; a /pt-br/ URL is always
// served as asked.
import { LANG_COOKIE, LANG_COOKIE_MAX_AGE, type Locale as SiteLocale } from "../../src/lib/i18n";

export type { SiteLocale };
/** Countries whose official language is Portuguese (ISO 3166-1 alpha-2, as in `request.cf.country`). */
const LUSOPHONE: Record<string, true> = { BR: true, PT: true, AO: true, MZ: true, CV: true, GW: true, ST: true, TL: true, MO: true };

const BOT = /bot|crawl|spider|slurp|bingpreview|facebookexternalhit|embedly|quora link preview|whatsapp|telegram|discord|slack|lighthouse|headless|preview/i;

/** `en`, `pt-BR`, or null for anything else (case-insensitive, `pt`/`pt-br`/`pt_BR` are Portuguese). */
export function parseLocale(v: string | null | undefined): SiteLocale | null {
	const s = v?.trim().toLowerCase().replace("_", "-");
	if (!s) return null;
	if (s === "en" || s.startsWith("en-")) return "en";
	if (s === "pt" || s.startsWith("pt-")) return "pt-BR";
	return null;
}

/** The value of one cookie in a `Cookie` header. */
export function cookieValue(header: string | null, name: string): string | null {
	for (const part of header?.split(";") ?? []) {
		const i = part.indexOf("=");
		if (i > 0 && part.slice(0, i).trim() === name) return decodeURIComponent(part.slice(i + 1).trim());
	}
	return null;
}

/** The supported language the browser ranks highest (ties keep header order), or null when it names neither. */
export function fromAcceptLanguage(header: string | null): SiteLocale | null {
	let best: { locale: SiteLocale; q: number } | null = null;
	for (const item of header?.split(",") ?? []) {
		const [tag, ...params] = item.trim().split(";");
		const locale = parseLocale(tag);
		if (!locale) continue;
		const qp = params.map((p) => p.trim()).find((p) => p.startsWith("q="));
		const q = qp ? Number(qp.slice(2)) : 1;
		if (!Number.isFinite(q) || q <= 0) continue;
		if (!best || q > best.q) best = { locale, q };
	}
	return best?.locale ?? null;
}

export interface Visitor {
	/** `?lang=` of this request, if any. */
	query: string | null;
	cookie: string | null;
	country: string | null;
	acceptLanguage: string | null;
	userAgent: string | null;
}

export function preferredLocale(v: Visitor): SiteLocale {
	const chosen = parseLocale(v.query) ?? parseLocale(cookieValue(v.cookie, LANG_COOKIE));
	if (chosen) return chosen;
	if (v.country && LUSOPHONE[v.country.toUpperCase()] === true) return "pt-BR";
	return fromAcceptLanguage(v.acceptLanguage) ?? "en";
}

export const isCrawler = (userAgent: string | null) => !userAgent || BOT.test(userAgent);

/** `/docs/x/` -> `/pt-br/docs/x/`. Only called for English paths. */
export const portuguesePath = (path: string) => (path === "/" ? "/pt-br/" : `/pt-br${path}`);

export const langCookie = (locale: SiteLocale) => `${LANG_COOKIE}=${locale}; Path=/; Max-Age=${LANG_COOKIE_MAX_AGE}; SameSite=Lax; Secure`;

/**
 * What to do with a request for an English page that has a Portuguese twin: `null` serves it as is, a Response redirects (and, when
 * `?lang=` was given, stores the choice). `?lang=` is removed from the URL it redirects to, so a shared link never carries it.
 */
export function localeRedirect(request: Request): Response | null {
	const url = new URL(request.url);
	// A Portuguese URL is served as asked (a handler may be shared by both languages, like the changelog's).
	if (url.pathname === "/pt-br" || url.pathname.startsWith("/pt-br/")) return null;
	const query = url.searchParams.get("lang");
	const visitor: Visitor = {
		query,
		cookie: request.headers.get("cookie"),
		country: ((request as Request & { cf?: { country?: string } }).cf?.country as string | undefined) ?? null,
		acceptLanguage: request.headers.get("accept-language"),
		userAgent: request.headers.get("user-agent"),
	};
	const explicit = parseLocale(query);
	if (!explicit && isCrawler(visitor.userAgent)) return null;
	const locale = preferredLocale(visitor);
	if (locale === "en" && !explicit) return null;
	url.searchParams.delete("lang");
	if (locale === "pt-BR") url.pathname = portuguesePath(url.pathname);
	const headers = new Headers({ location: `${url.pathname}${url.search}`, "cache-control": "private, no-store", vary: "Cookie, Accept-Language" });
	if (explicit) headers.append("set-cookie", langCookie(explicit));
	return new Response(null, { status: 302, headers });
}

/** Pages Function for an English page with a Portuguese twin: redirect when the visitor wants Portuguese, else the static page. */
export const localizedPage: PagesFunction = async ({ request, next }) => {
	const redirect = localeRedirect(request);
	if (redirect) return redirect;
	const res = await next();
	const out = new Response(res.body, res);
	out.headers.append("vary", "Cookie, Accept-Language");
	return out;
};
