// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { expect, test } from "bun:test";
import { breadcrumbLd, ORG_ID, SITE_ID, siteGraph, techArticleLd } from "./seo";

test("the site graph names the organization and the web site in the language of the page", () => {
	const g = siteGraph("pt-BR");
	expect(g["@graph"].map((n) => n["@type"])).toEqual(["Organization", "WebSite"]);
	const site = g["@graph"][1] as Record<string, unknown>;
	expect(site.inLanguage).toBe("pt-BR");
	expect(site["@id"]).toBe(SITE_ID);
	expect(site.publisher).toEqual({ "@id": ORG_ID });
	expect(JSON.parse(JSON.stringify(g))).toEqual(g);
});

test("a breadcrumb is ordered, 1-based and absolute", () => {
	const b = breadcrumbLd([{ name: "Atlas", path: "/" }, { name: "Docs", path: "/docs/introduction/" }, { name: "CLI", path: "/docs/cli/" }]);
	expect(b["@type"]).toBe("BreadcrumbList");
	expect(b.itemListElement.map((i) => [i.position, i.item])).toEqual([
		[1, "https://atlas.codeatlas.com.br/"],
		[2, "https://atlas.codeatlas.com.br/docs/introduction/"],
		[3, "https://atlas.codeatlas.com.br/docs/cli/"],
	]);
});

test("a TechArticle has its language and image, and dateModified only when known", () => {
	const base = { headline: "CLI", locale: "en" as const, path: "/docs/cli/", image: "/og/en/docs/cli.png" };
	const a = techArticleLd({ ...base, description: "Commands", dateModified: "2026-11-01T10:00:00+00:00" });
	expect(a).toMatchObject({ "@type": "TechArticle", headline: "CLI", inLanguage: "en", url: "https://atlas.codeatlas.com.br/docs/cli/", image: "https://atlas.codeatlas.com.br/og/en/docs/cli.png", dateModified: "2026-11-01T10:00:00+00:00" });
	const b = techArticleLd(base);
	expect("dateModified" in b).toBe(false);
	expect("description" in b).toBe(false);
});
