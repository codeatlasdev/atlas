import { expect, test } from "bun:test";
import { markdownPathOf, wantsMarkdown } from "./negotiate";

const BROWSER = "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";

test("a browser (HTML first) never gets markdown", () => {
	expect(wantsMarkdown(BROWSER)).toBe(false);
	expect(wantsMarkdown("text/html")).toBe(false);
});

test("no Accept, a bare wildcard and text/* mean HTML", () => {
	expect(wantsMarkdown(null)).toBe(false);
	expect(wantsMarkdown("")).toBe(false);
	expect(wantsMarkdown("*/*")).toBe(false);
	expect(wantsMarkdown("text/*")).toBe(false);
});

test("a client that names text/markdown gets it", () => {
	expect(wantsMarkdown("text/markdown")).toBe(true);
	expect(wantsMarkdown("text/markdown, */*")).toBe(true);
	expect(wantsMarkdown("TEXT/Markdown;charset=utf-8")).toBe(true);
	expect(wantsMarkdown("text/markdown, text/html;q=0.8")).toBe(true);
});

test("markdown and HTML at the same quality: the first one listed wins", () => {
	expect(wantsMarkdown("text/markdown, text/html")).toBe(true);
	expect(wantsMarkdown("text/html, text/markdown")).toBe(false);
});

test("quality decides before the order", () => {
	expect(wantsMarkdown("text/html, text/markdown;q=0.5")).toBe(false);
	expect(wantsMarkdown("text/html;q=0.4, text/markdown;q=0.9")).toBe(true);
	expect(wantsMarkdown("text/markdown;q=0")).toBe(false);
	expect(wantsMarkdown("text/markdown;q=0, text/html")).toBe(false);
	expect(wantsMarkdown("text/markdown;q=junk, text/html")).toBe(false);
});

test("a wildcard never outranks a named type", () => {
	expect(wantsMarkdown("text/markdown;q=0.1, */*")).toBe(false);
	expect(wantsMarkdown("text/markdown;q=0.5, text/html;q=0.1, */*")).toBe(true);
});

test("only docs pages have markdown", () => {
	expect(markdownPathOf("/docs/cli/")).toBe("/docs/cli.md");
	expect(markdownPathOf("/docs/getting-started")).toBe("/docs/getting-started.md");
	expect(markdownPathOf("/pt-br/docs/cli/")).toBe("/pt-br/docs/cli.md");
	for (const p of ["/docs/", "/docs/cli.md", "/download/", "/", "/docs/a/b/", "/pt-br/docs/"]) expect(markdownPathOf(p)).toBeNull();
});
