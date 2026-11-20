// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { expect, test } from "bun:test";
import { LOCALIZED_PAGES, localizePath } from "./i18n";
import { ogKeyOf, ogPathOf, ogSegment, ogTree, titleSize, type OgCard } from "./og";

test("every localized page has its own image key, in both languages", () => {
	const keys = LOCALIZED_PAGES.map((p) => ogKeyOf(p));
	expect(keys).toEqual(["home", "download", "changelog", "privacy", "code-signing-policy"]);
	expect(new Set(keys).size).toBe(keys.length);
	expect(ogPathOf("/download/")).toBe("/og/en/download.png");
	expect(ogPathOf(localizePath("/download/", "pt-BR"))).toBe("/og/pt-br/download.png");
	expect(ogPathOf("/pt-br/")).toBe("/og/pt-br/home.png");
});

test("docs pages map to docs/<slug> and keep their language", () => {
	expect(ogPathOf("/docs/getting-started/")).toBe("/og/en/docs/getting-started.png");
	expect(ogPathOf("/pt-br/docs/cli/")).toBe("/og/pt-br/docs/cli.png");
	expect(ogKeyOf("/docs/cli/")).toBe("docs/cli");
});

test("a page with no image of its own (the 404) shares the home image", () => {
	expect(ogPathOf("/404/")).toBe("/og/en/home.png");
	expect(ogPathOf("/docs/")).toBe("/og/en/home.png");
});

test("the folder of a language is its URL segment, English included", () => {
	expect(ogSegment("en")).toBe("en");
	expect(ogSegment("pt-BR")).toBe("pt-br");
});

test("long titles get a smaller size, never below what fits three lines", () => {
	expect(titleSize("Get Atlas.")).toBeGreaterThan(titleSize("Put AI agents to work. Keep the final say."));
	expect(titleSize("x".repeat(200))).toBeGreaterThanOrEqual(80);
});

test("the layout carries the title, the subtitle, the address and the words of the language", () => {
	const card: OgCard = { title: "Primeiros passos", subtitle: "Instale o Atlas", kicker: "Documentação", footer: "Gratuito e de código aberto", url: "atlas.codeatlas.com.br/pt-br/docs/x", ground: "paper" };
	const text = JSON.stringify(ogTree(card, "data:image/svg+xml;base64,AAAA"));
	for (const s of [card.title, card.subtitle, card.kicker!, card.footer, card.url, "Atlas"]) expect(text).toContain(s);
});
