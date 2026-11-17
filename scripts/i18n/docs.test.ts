// Translated docs: the git blob sha that ties a translation to its English source, and the checker around it.
import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gitBlobSha } from "./hash.ts";
import { check, cleanup, errors, finding, blobSha, makeRepo, put, read, rulesOf } from "./testkit.ts";
import { runStatus } from "./check.ts";
import { loadCtx } from "./types.ts";

afterAll(cleanup);

const EN = "docs/getting-started.md";
const PT = "docs/pt-BR/getting-started.md";
const docs = async (root: string) => (await check(root, "docs")).filter((f) => f.severity !== "info");

describe("git blob sha", () => {
	test.each([
		["empty", ""],
		["ascii", "# Title\n\nHello.\n"],
		["no trailing newline", "no newline"],
		["accents and emoji", "# Instalação ✓ 🚀\n\nconfigurações\n"],
		["a large file", "line of docs text\n".repeat(50_000)],
	])("equals `git hash-object` for %s", async (_n, text) => {
		const dir = mkdtempSync(join(tmpdir(), "i18n-sha-"));
		try {
			const f = join(dir, "x.md");
			writeFileSync(f, text);
			expect(gitBlobSha(text)).toBe(await blobSha(dir, "x.md"));
			expect(gitBlobSha(new TextEncoder().encode(text))).toBe(await blobSha(dir, "x.md"));
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test("a different byte is a different sha, and CRLF is folded to the LF git stores", () => {
		expect(gitBlobSha("a\n")).not.toBe(gitBlobSha("b\n"));
		expect(gitBlobSha("a\r\nb\r\n")).toBe(gitBlobSha("a\nb\n"));
	});
});

describe("docs/<locale>/<page>.md", () => {
	test("a translation with the current source sha is clean and counted fresh", async () => {
		const root = await makeRepo();
		expect(await docs(root)).toEqual([]);
		const rows = await runStatus(loadCtx(root));
		expect(rows.find((r) => r.surface === "docs")).toMatchObject({ locale: "pt-BR", total: 1, fresh: 1, outdated: 0, missing: 0 });
	});

	test("English that changed is a WARNING with the new sha, never an error", async () => {
		const root = await makeRepo();
		put(root, EN, `${read(root, EN)}\nOne more paragraph.\n`);
		const f = finding(await docs(root), "docs-outdated");
		expect(f).toMatchObject({ severity: "warn", file: PT });
		expect(f?.message).toContain(await blobSha(root, EN));
		expect(errors(await check(root, "docs"))).toEqual([]);
		const row = (await runStatus(loadCtx(root))).find((r) => r.surface === "docs");
		expect(row).toMatchObject({ fresh: 0, outdated: 1 });
	});

	test("updating source_sha after retranslating clears the warning", async () => {
		const root = await makeRepo();
		put(root, EN, `${read(root, EN)}\nOne more paragraph.\n`);
		put(root, PT, read(root, PT).replace(/source_sha: "[0-9a-f]{40}"/, `source_sha: "${await blobSha(root, EN)}"`));
		expect(rulesOf(await docs(root))).not.toContain("docs-outdated");
	});

	test("a page without a translation is info only", async () => {
		const root = await makeRepo();
		put(root, "docs/crew.md", "# Crew\n");
		const fs = await check(root, "docs");
		expect(finding(fs, "docs-missing")).toMatchObject({ severity: "info", file: "docs/pt-BR/crew.md" });
		expect(fs.filter((f) => f.severity !== "info")).toEqual([]);
	});

	test.each([
		["no front matter", (t: string) => t.replace(/^---[\s\S]*?---\n/, ""), "front matter"],
		["no title", (t: string) => t.replace('title: "Primeiros passos"\n', ""), "title"],
		["a short sha", (t: string) => t.replace(/source_sha: "[0-9a-f]{40}"/, 'source_sha: "abc123"'), "source_sha"],
		["no source_sha", (t: string) => t.replace(/source_sha: .*\n/, ""), "source_sha"],
		["an unquoted all-digit sha (YAML parses a number)", (t: string) => t.replace(/source_sha: "[0-9a-f]{40}"/, `source_sha: ${"1".repeat(40)}`), "quoted string"],
		["a source that is gone", (t: string) => t.replace("source: getting-started.md", "source: deleted.md"), "deleted.md"],
		["a source outside docs/", (t: string) => t.replace("source: getting-started.md", "source: ../README.md"), "../README.md"],
		["no source", (t: string) => t.replace("source: getting-started.md\n", ""), "source"],
		["invalid YAML", (t: string) => t.replace("---\n", "---\n: : [\n"), "front matter"],
	])("%s is a front matter error", async (_n, edit, text) => {
		const root = await makeRepo();
		put(root, PT, edit(read(root, PT)));
		const f = finding(await docs(root), "docs-frontmatter");
		expect(f?.severity).toBe("error");
		expect(f?.message).toContain(text);
	});

	test("a changed code block or heading count is a structure warning", async () => {
		const root = await makeRepo();
		put(root, PT, read(root, PT).replace("bun install", "bun add"));
		expect(finding(await docs(root), "docs-structure")).toMatchObject({ severity: "warn" });
		put(root, PT, read(root, PT).replace("bun add", "bun install").replace("Instale o Atlas.", "Instale o Atlas.\n\n## Mais"));
		expect(finding(await docs(root), "docs-structure")?.message).toContain("heading");
		put(root, PT, read(root, PT).replace(/```sh[\s\S]*```\n/, ""));
		expect((await docs(root)).some((f) => f.message.includes("code block"))).toBe(true);
	});

	test("code blocks compare without comments: a translated comment is no warning, a changed command is", async () => {
		const root = await makeRepo();
		const withBlock = async (en: string, pt: string) => {
			put(root, EN, `# Start\n\n\`\`\`sh\n${en}\n\`\`\`\n`);
			const sha = await blobSha(root, EN);
			put(root, PT, `---\ntitle: "Começo"\nsource: getting-started.md\nsource_sha: "${sha}"\n---\n\n# Começo\n\n\`\`\`sh\n${pt}\n\`\`\`\n`);
			return (await docs(root)).filter((f) => f.rule === "docs-structure");
		};
		expect(await withBlock("# install it\nbun install # fast\ncurl https://x.example/a | sh // pipe", "# instale\nbun install # rápido\ncurl https://x.example/a | sh // pipe em shell")).toEqual([]);
		expect(await withBlock("bun install", "bun install\n\n# só um comentário a mais")).toEqual([]);
		const changed = await withBlock("# install\nbun install", "# instale\nbun add");
		expect(changed).toHaveLength(1);
		expect(changed[0].message).toContain("code block 1");
		expect(await withBlock("curl https://x.example/a", "curl https://x.example/b")).toHaveLength(1);
		expect(await withBlock("echo a#b", "echo a#c")).toHaveLength(1);
	});

	test("a `#` line inside a code block is not a heading", async () => {
		const root = await makeRepo();
		put(root, EN, "# Start\n\n```sh\n# a comment\nbun install\n```\n");
		put(root, PT, read(root, PT).replace(/\n# Primeiros passos[\s\S]*$/, "\n# Começo\n\n```sh\n# a comment\nbun install\n```\n").replace(/source_sha: "[0-9a-f]{40}"/, `source_sha: "${await blobSha(root, EN)}"`));
		expect(await docs(root)).toEqual([]);
	});

	test("pt-PT words in prose warn with their line; the same words in code do not", async () => {
		const root = await makeRepo();
		put(root, PT, `${read(root, PT)}\nGuarde o ficheiro.\n\n\`\`\`sh\nficheiro utilizador\n\`\`\`\n`);
		const hits = (await docs(root)).filter((f) => f.rule === "pt-pt");
		expect(hits.length).toBeGreaterThan(0);
		expect(hits.every((f) => f.severity === "warn" && (f.line ?? 0) > 5)).toBe(true);
		expect(hits.map((f) => f.message).join(" ")).not.toContain("utilizador");
	});
});
