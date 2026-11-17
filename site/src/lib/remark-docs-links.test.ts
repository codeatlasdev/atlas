// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { describe, expect, test } from "bun:test";
import remarkDocsLinks from "./remark-docs-links.mjs";

// Links are authored to read well on GitHub, then rewritten for the site. The plugin only sees the file path to know whether
// the page is an English doc (docs/x.md) or a translation (docs/pt-BR/x.md, whose relative links resolve from docs/pt-BR/).
const rewrite = (path: string, url: string): string => {
	const link = { type: "link", url, children: [] };
	remarkDocsLinks()({ type: "root", children: [{ type: "paragraph", children: [link] }] }, { path });
	return link.url;
};
const EN = "/repo/docs/getting-started.md";
const PT = "/repo/docs/pt-BR/getting-started.md";

describe("English docs", () => {
	test("a sibling doc becomes its page, keeping the anchor", () => {
		expect(rewrite(EN, "installation.md#windows")).toBe("/docs/installation/#windows");
		expect(rewrite(EN, "./cli.md")).toBe("/docs/cli/");
	});
	test("another repo file goes to GitHub, relative to docs/", () => {
		expect(rewrite(EN, "../skills/atlas/SKILL.md")).toBe("https://github.com/codeatlasdev/atlas/blob/main/skills/atlas/SKILL.md");
	});
	test("absolute and anchor links are untouched", () => {
		expect(rewrite(EN, "https://example.com/a.md")).toBe("https://example.com/a.md");
		expect(rewrite(EN, "#top")).toBe("#top");
	});
});

describe("pt-BR translations", () => {
	test("both x.md and ../x.md become the pt-BR page of that doc (a page exists for every doc)", () => {
		expect(rewrite(PT, "installation.md#windows")).toBe("/pt-br/docs/installation/#windows");
		expect(rewrite(PT, "../installation.md")).toBe("/pt-br/docs/installation/");
	});
	test("../README.md is the docs index", () => {
		expect(rewrite(PT, "../README.md")).toBe("/pt-br/docs/");
	});
	test("other repo files resolve from docs/pt-BR/, so ../../ reaches the repo root", () => {
		expect(rewrite(PT, "../../skills/atlas/SKILL.md")).toBe("https://github.com/codeatlasdev/atlas/blob/main/skills/atlas/SKILL.md");
	});
	test("a file next to the English docs is reached with ../ (docs/perf/x.json), not lost one level up", () => {
		expect(rewrite(PT, "../perf/x.json")).toBe("https://github.com/codeatlasdev/atlas/blob/main/docs/perf/x.json");
	});
	test("a Windows path is recognised too", () => {
		expect(rewrite("C:\\repo\\docs\\pt-BR\\x.md", "cli.md")).toBe("/pt-br/docs/cli/");
	});
	test("an English doc that merely mentions pt-BR in its name is not a translation", () => {
		expect(rewrite("/repo/docs/pt-BR-notes.md", "cli.md")).toBe("/docs/cli/");
	});
});
