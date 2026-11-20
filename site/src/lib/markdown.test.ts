// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { expect, test } from "bun:test";
import { absoluteLink, docMarkdown } from "./markdown";

const SITE = "https://atlas.codeatlas.com.br";
const REPO = "https://github.com/codeatlasdev/atlas";

test("a link to a sibling doc becomes the .md address of that page", () => {
	expect(absoluteLink("cli.md", "en")).toBe(`${SITE}/docs/cli.md`);
	expect(absoluteLink("./cli.md#flags", "en")).toBe(`${SITE}/docs/cli.md#flags`);
	// a translation links to `../x.md` (English) and `x.md`; both are the pt-BR page
	expect(absoluteLink("../cli.md", "pt-BR")).toBe(`${SITE}/pt-br/docs/cli.md`);
	expect(absoluteLink("cli.md", "pt-BR")).toBe(`${SITE}/pt-br/docs/cli.md`);
});

test("other relative links point into the repository, absolute ones stay", () => {
	expect(absoluteLink("../skills/atlas/SKILL.md", "en")).toBe(`${REPO}/blob/main/skills/atlas/SKILL.md`);
	expect(absoluteLink("../crates/atlas-cli", "en")).toBe(`${REPO}/tree/main/crates/atlas-cli`);
	expect(absoluteLink("../../skills/x/SKILL.md", "pt-BR")).toBe(`${REPO}/blob/main/skills/x/SKILL.md`);
	for (const u of ["https://example.com/a.md", "#top", "/download/", "mailto:a@b.c"]) expect(absoluteLink(u, "en")).toBe(u);
});

test("links are rewritten in prose, not in code fences or inline code", () => {
	const body = ["# Title", "", "See [the CLI](cli.md) and `[x](cli.md)`.", "", "```md", "[y](cli.md)", "```", "", "![img](../a.png)"].join("\n");
	const out = docMarkdown(body, "Title", "en");
	expect(out).toContain(`[the CLI](${SITE}/docs/cli.md)`);
	expect(out).toContain("`[x](cli.md)`");
	expect(out).toContain("[y](cli.md)");
	expect(out).toContain(`![img](${REPO}/blob/main/a.png)`);
});

test("a body without a title gets one; a `# comment` in a fence is not a title", () => {
	expect(docMarkdown("Text.", "Hello", "en")).toBe("# Hello\n\nText.\n");
	expect(docMarkdown("```sh\n# comment\n```", "Hello", "en")).toBe("# Hello\n\n```sh\n# comment\n```\n");
	expect(docMarkdown("# Own\n\nText.", "Hello", "en")).toBe("# Own\n\nText.\n");
});
