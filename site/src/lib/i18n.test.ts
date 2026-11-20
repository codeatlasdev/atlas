// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { describe, expect, test } from "bun:test";
import { delocalizePath, docPath, hreflangLinks, inSitemap, localeOfAstro, localizePath, switchTarget, withXDefault } from "./i18n";

const ORIGIN = "https://atlas.codeatlas.com.br";

describe("locale of a request", () => {
	test("Astro's pt-br path, the pt-BR code and bare pt are all pt-BR; everything else is English", () => {
		expect(["pt-br", "pt-BR", "pt", "PT-BR"].map(localeOfAstro)).toEqual(["pt-BR", "pt-BR", "pt-BR", "pt-BR"]);
		expect([undefined, "en", "fr", ""].map(localeOfAstro)).toEqual(["en", "en", "en", "en"]);
	});
});

describe("paths", () => {
	test("English is unprefixed, pt-BR gets /pt-br/ (and the root keeps its slash)", () => {
		expect(localizePath("/docs/x/", "en")).toBe("/docs/x/");
		expect(localizePath("/docs/x/", "pt-BR")).toBe("/pt-br/docs/x/");
		expect(localizePath("/", "pt-BR")).toBe("/pt-br/");
		expect(docPath("cli", "pt-BR")).toBe("/pt-br/docs/cli/");
	});
	test("delocalize round-trips localize, and a path that merely starts with the letters is not a locale", () => {
		for (const p of ["/", "/docs/x/", "/download/"]) expect(delocalizePath(localizePath(p, "pt-BR"))).toEqual({ locale: "pt-BR", path: p });
		expect(delocalizePath("/pt-br")).toEqual({ locale: "pt-BR", path: "/" });
		expect(delocalizePath("/pt-brazil/x/")).toEqual({ locale: "en", path: "/pt-brazil/x/" });
		expect(delocalizePath("/docs/pt-br/")).toEqual({ locale: "en", path: "/docs/pt-br/" });
	});
});

describe("which pages get hreflang", () => {
	test("a page that exists in both languages lists both and x-default = English", () => {
		expect(hreflangLinks({ en: "/docs/getting-started/", "pt-BR": "/pt-br/docs/getting-started/" }, ORIGIN)).toEqual([
			{ hreflang: "en", href: `${ORIGIN}/docs/getting-started/` },
			{ hreflang: "pt-BR", href: `${ORIGIN}/pt-br/docs/getting-started/` },
			{ hreflang: "x-default", href: `${ORIGIN}/docs/getting-started/` },
		]);
	});
	test("an English-only page gets none (a lone self-reference is noise, and pt-br would be a fallback copy)", () => {
		expect(hreflangLinks({ en: "/docs/cli/" }, ORIGIN)).toEqual([]);
		expect(hreflangLinks({}, ORIGIN)).toEqual([]);
	});
	test("a pt-BR page without an English twin has no x-default to invent", () => {
		expect(hreflangLinks({ "pt-BR": "/pt-br/docs/x/", en: undefined }, ORIGIN)).toEqual([]);
	});
});

describe("language switch in the nav", () => {
	test("English page: links to pt-BR only when the translation exists", () => {
		expect(switchTarget("en", { en: "/docs/a/", "pt-BR": "/pt-br/docs/a/" }, undefined)).toEqual({ locale: "pt-BR", path: "/pt-br/docs/a/" });
		expect(switchTarget("en", undefined, undefined)).toBeUndefined();
		expect(switchTarget("en", { en: "/docs/a/" }, undefined)).toBeUndefined();
	});
	test("translated pt-BR page: back to its English twin", () => {
		expect(switchTarget("pt-BR", { en: "/docs/a/", "pt-BR": "/pt-br/docs/a/" }, "/pt-br/docs/a/")).toEqual({ locale: "en", path: "/docs/a/" });
	});
	test("fallback pt-BR page (no alternates): the canonical path is the English page", () => {
		expect(switchTarget("pt-BR", undefined, "/docs/b/")).toEqual({ locale: "en", path: "/docs/b/" });
		expect(switchTarget("pt-BR", undefined, undefined)).toBeUndefined();
	});
});

describe("sitemap membership", () => {
	const translated = new Set(["getting-started"]);
	test("every English page is in", () => {
		for (const p of ["/", "/docs/cli/", "/download/", "/pt-brazil/"]) expect(inSitemap(p, translated)).toBe(true);
	});
	test("a translated pt-BR docs page is in", () => {
		expect(inSitemap("/pt-br/docs/getting-started/", translated)).toBe(true);
	});
	test("a fallback pt-BR docs page is out (English body, canonical to English)", () => {
		expect(inSitemap("/pt-br/docs/cli/", translated)).toBe(false);
	});
	test("the pt-BR twins of the localized pages are in; the /pt-br/docs/ redirect and unknown pages are out", () => {
		for (const p of ["/pt-br/", "/pt-br/download/", "/pt-br/changelog/", "/pt-br/privacy/", "/pt-br/code-signing-policy/"]) expect(inSitemap(p, translated)).toBe(true);
		for (const p of ["/pt-br/docs/", "/pt-br/404/", "/pt-br/download/extra/"]) expect(inSitemap(p, translated)).toBe(false);
	});
	test("a nested path under a translated slug is not the translated page", () => {
		expect(inSitemap("/pt-br/docs/getting-started/extra/", translated)).toBe(false);
	});
});

describe("sitemap alternates", () => {
	const en = { url: "https://x/docs/a/", lang: "en" };
	const pt = { url: "https://x/pt-br/docs/a/", lang: "pt-BR" };
	test("x-default is the English URL, added after the real alternates", () => {
		expect(withXDefault([en, pt])).toEqual([en, pt, { url: en.url, lang: "x-default" }]);
	});
	test("a page with no alternates stays without (nothing to default between)", () => {
		expect(withXDefault(undefined)).toBeUndefined();
	});
	test("no English alternate, no x-default; an existing x-default is not doubled", () => {
		expect(withXDefault([pt])).toEqual([pt]);
		const done = [en, pt, { url: en.url, lang: "x-default" }];
		expect(withXDefault(done)).toEqual(done);
	});
});
