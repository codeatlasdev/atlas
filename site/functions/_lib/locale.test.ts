import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { LOCALIZED_PAGES } from "../../src/lib/i18n";
import { cookieValue, fromAcceptLanguage, localeRedirect, preferredLocale, type Visitor } from "./locale";

const CHROME = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Safari/537.36";
const visitor = (v: Partial<Visitor>): Visitor => ({ query: null, cookie: null, country: null, acceptLanguage: null, userAgent: CHROME, ...v });

/** A request as Cloudflare hands it to a Pages Function: `cf.country` is the country of the client IP. */
function req(path: string, headers: Record<string, string> = {}, country?: string): Request {
	const r = new Request(`https://atlas.codeatlas.com.br${path}`, { headers: { "user-agent": CHROME, ...headers } });
	if (country) Object.defineProperty(r, "cf", { value: { country } });
	return r;
}

describe("preferredLocale", () => {
	test("a visitor in Brazil gets Portuguese even with an English browser", () => {
		expect(preferredLocale(visitor({ country: "BR", acceptLanguage: "en-US,en;q=0.9" }))).toBe("pt-BR");
	});
	test("an explicit choice beats the country: a Brazilian who picked English stays in English", () => {
		expect(preferredLocale(visitor({ country: "BR", cookie: "x=1; atlas_lang=en" }))).toBe("en");
		expect(preferredLocale(visitor({ country: "US", cookie: "atlas_lang=pt-BR" }))).toBe("pt-BR");
	});
	test("?lang= on the request beats an older cookie", () => {
		expect(preferredLocale(visitor({ query: "en", cookie: "atlas_lang=pt-BR", country: "BR" }))).toBe("en");
	});
	test("outside a Portuguese-speaking country the browser's ranking decides", () => {
		expect(preferredLocale(visitor({ country: "US", acceptLanguage: "pt-BR,pt;q=0.9,en;q=0.8" }))).toBe("pt-BR");
		expect(preferredLocale(visitor({ country: "DE", acceptLanguage: "de-DE,en;q=0.7,pt;q=0.5" }))).toBe("en");
		expect(preferredLocale(visitor({ country: "US", acceptLanguage: "fr-FR" }))).toBe("en");
	});
	test("an unknown cookie value is ignored, not trusted", () => {
		expect(preferredLocale(visitor({ cookie: "atlas_lang=klingon", acceptLanguage: "pt-PT" }))).toBe("pt-BR");
	});
});

describe("header parsing", () => {
	test("Accept-Language is ranked by q, ties keep header order, q=0 means not acceptable", () => {
		expect(fromAcceptLanguage("en;q=0.4, pt-BR;q=0.8")).toBe("pt-BR");
		expect(fromAcceptLanguage("pt;q=0, en")).toBe("en");
		expect(fromAcceptLanguage("es, it")).toBeNull();
		expect(fromAcceptLanguage(null)).toBeNull();
	});
	test("cookie lookup matches the whole name, not a suffix", () => {
		expect(cookieValue("not_atlas_lang=en; atlas_lang=pt-BR", "atlas_lang")).toBe("pt-BR");
		expect(cookieValue("atlas_language=en", "atlas_lang")).toBeNull();
	});
});

describe("localeRedirect", () => {
	test("a Brazilian visitor opening an English page is sent to its /pt-br/ twin, query kept", () => {
		const r = localeRedirect(req("/docs/crew/?x=1", {}, "BR"))!;
		expect(r.status).toBe(302);
		expect(r.headers.get("location")).toBe("/pt-br/docs/crew/?x=1");
		expect(r.headers.get("cache-control")).toContain("no-store");
		expect(localeRedirect(req("/", {}, "BR"))!.headers.get("location")).toBe("/pt-br/");
	});
	test("an English visitor is served the page as is (no redirect)", () => {
		expect(localeRedirect(req("/download/", { "accept-language": "en-US" }, "US"))).toBeNull();
	});
	test("crawlers and link previews are never redirected, so each URL keeps its own language for search", () => {
		const bot = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)";
		expect(localeRedirect(req("/", { "user-agent": bot, "accept-language": "pt-BR" }, "BR"))).toBeNull();
		expect(localeRedirect(req("/", { "user-agent": "WhatsApp/2.23" }, "BR"))).toBeNull();
	});
	test("?lang=en stores the choice and drops the parameter from the URL", () => {
		const r = localeRedirect(req("/download/?lang=en&a=b", {}, "BR"))!;
		expect(r.headers.get("location")).toBe("/download/?a=b");
		expect(r.headers.get("set-cookie")).toMatch(/^atlas_lang=en; Path=\/; Max-Age=31536000; SameSite=Lax; Secure$/);
	});
	test("?lang=pt-BR stores the choice and opens the Portuguese page", () => {
		const r = localeRedirect(req("/?lang=pt-BR", {}, "US"))!;
		expect(r.headers.get("location")).toBe("/pt-br/");
		expect(r.headers.get("set-cookie")).toStartWith("atlas_lang=pt-BR;");
	});
	test("a saved English choice wins over the country", () => {
		expect(localeRedirect(req("/", { cookie: "atlas_lang=en" }, "BR"))).toBeNull();
	});
});

test("every localized page and the docs have a Pages Function that redirects to the Portuguese twin", () => {
	const routes = [...LOCALIZED_PAGES.map((p) => (p === "/" ? "index.ts" : `${p.slice(1)}index.ts`)), "docs/[[path]].ts"];
	for (const r of routes) {
		const file = new URL(`../${r}`, import.meta.url);
		expect(existsSync(file)).toBe(true);
		expect(readFileSync(file, "utf8")).toMatch(/localizedPage|localeRedirect\(/);
	}
});

// A route that redirects to /pt-br/x/ while no src/pages/pt-br/x page exists is an endless loop: Astro's i18n fallback sends /pt-br/x/
// back to /x/. Every localized page must have its real Portuguese twin.
test("every localized page has a real /pt-br/ page, so the redirect can never loop", () => {
	for (const p of LOCALIZED_PAGES) {
		const base = p === "/" ? "index" : p.slice(1, -1);
		const twin = [`${base}.astro`, `${base}/index.astro`].map((f) => new URL(`../../src/pages/pt-br/${f}`, import.meta.url));
		expect({ page: p, twin: twin.some((u) => existsSync(u)) }).toEqual({ page: p, twin: true });
	}
});
