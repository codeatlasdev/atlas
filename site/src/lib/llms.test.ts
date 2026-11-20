// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { expect, test } from "bun:test";
import { llmsFull, llmsTxt, markdownPath, type LlmsGroup } from "./llms";

const GROUPS: LlmsGroup[] = [
	{ title: "Start here", items: [{ slug: "introduction", title: "Introduction", description: "What Atlas is." }, { slug: "getting-started", title: "Getting started" }] },
	{ title: "Empty", items: [] },
	{ title: "Reference", items: [{ slug: "cli", title: "CLI", description: "Commands." }] },
];

test("llms.txt: the positioning, one section per non-empty group, every page as a .md link", () => {
	const t = llmsTxt(GROUPS, "en");
	expect(t.startsWith("# Atlas\n\n> Atlas is a free, open-source workspace for working with AI agents.")).toBe(true);
	expect(t).toContain("Grand Line");
	expect(t).toContain("## Start here\n\n- [Introduction](https://atlas.codeatlas.com.br/docs/introduction.md): What Atlas is.\n- [Getting started](https://atlas.codeatlas.com.br/docs/getting-started.md)\n");
	expect(t).toContain("## Reference");
	expect(t).not.toContain("## Empty");
	expect(t).toContain("https://atlas.codeatlas.com.br/llms-full.txt");
	expect(t).toContain("https://atlas.codeatlas.com.br/pt-br/llms.txt");
	expect(t.indexOf("## Optional")).toBeGreaterThan(t.indexOf("## Reference"));
});

test("the Portuguese index links the pt-BR .md files and points back to the English one", () => {
	const t = llmsTxt([{ title: "Start here", items: [{ slug: "getting-started", title: "Primeiros passos" }] }], "pt-BR");
	expect(t).toContain("- [Primeiros passos](https://atlas.codeatlas.com.br/pt-br/docs/getting-started.md)");
	expect(t).toContain("https://atlas.codeatlas.com.br/llms.txt");
	expect(t).not.toContain("llms-full");
	expect(markdownPath("cli", "pt-BR")).toBe("/pt-br/docs/cli.md");
});

test("llms-full.txt: every page in the given order after a comment with its address", () => {
	const t = llmsFull([{ slug: "introduction", markdown: "# Introduction\n\nA.\n" }, { slug: "cli", markdown: "# CLI\n\nB.\n" }]);
	expect(t.indexOf("<!-- https://atlas.codeatlas.com.br/docs/introduction/ -->")).toBeLessThan(t.indexOf("<!-- https://atlas.codeatlas.com.br/docs/cli/ -->"));
	expect(t).toContain("# Introduction\n\nA.");
	expect(t.split("\n---\n").length).toBe(2);
});
