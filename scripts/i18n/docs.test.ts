// Translated docs: the git blob sha that ties a translation to its English source, and the checker around it.
import { afterAll, describe, expect, setDefaultTimeout, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gitBlobSha } from "./hash.ts";
import { check, cleanup, errors, finding, blobSha, makeRepo, must, put, read, rulesOf } from "./testkit.ts";
import { runStatus } from "./check.ts";
import { loadCtx } from "./types.ts";

// The diff-mode cases build real git repositories and the base resolver reads the environment: run them with a clean one.
const AMBIENT = ["ATLAS_BASE", "GITHUB_ACTIONS", "GITHUB_EVENT_NAME", "GITHUB_EVENT_PATH"];
const saved = Object.fromEntries(AMBIENT.map((k) => [k, process.env[k]]));
for (const k of AMBIENT) delete process.env[k];

afterAll(() => {
	for (const k of AMBIENT) {
		if (saved[k] === undefined) delete process.env[k];
		else process.env[k] = saved[k];
	}
	cleanup();
});

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

	test("`translation: machine` is accepted and leaves the page fresh; any other value is an error", async () => {
		const root = await makeRepo();
		put(root, PT, read(root, PT).replace("source: getting-started.md\n", "source: getting-started.md\ntranslation: machine\n"));
		expect(await docs(root)).toEqual([]);
		expect((await runStatus(loadCtx(root))).find((r) => r.surface === "docs")).toMatchObject({ fresh: 1, outdated: 0 });
		put(root, PT, read(root, PT).replace("translation: machine", "translation: reviewed"));
		const f = finding(await docs(root), "docs-frontmatter");
		expect(f?.severity).toBe("error");
		expect(f?.message).toContain("translation");
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

// Each case builds a real git repository; a loaded CI runner needs more than the default 5 s.
setDefaultTimeout(60_000);

const ISSUE = "https://github.com/codeatlasdev/atlas/issues/700";

/** `makeRepo` committed on `main` (the base) with a `feature` branch checked out: the change under review. */
async function feature(over: Record<string, string | null> = {}): Promise<string> {
	const root = await makeRepo(over);
	must(root, "init", "-q", "-b", "main");
	must(root, "add", "-A");
	must(root, "commit", "-q", "-m", "base");
	must(root, "checkout", "-q", "-b", "feature");
	return root;
}

/** The translation restamped to the English file as it is now. */
const restamp = async (root: string, extra = "") => put(root, PT, read(root, PT).replace(/source_sha: "[0-9a-f]{40}"/, `source_sha: "${await blobSha(root, EN)}"${extra}`));
const edit = (root: string) => put(root, EN, `${read(root, EN)}\nOne more paragraph.\n`);

describe("a change that edits an English page owes its translation", () => {
	test("red: English edited, translation untouched", async () => {
		const root = await feature();
		edit(root);
		const f = finding(await docs(root), "docs-outdated");
		expect(f).toMatchObject({ severity: "error", file: PT });
		expect(f?.message).toContain(await blobSha(root, EN));
		expect(f?.message).toContain("translation: pending");
		expect(errors(await check(root, "docs"))).toHaveLength(1);
	});

	test("red: English edited, translation touched but still stamped with the old sha", async () => {
		const root = await feature();
		edit(root);
		put(root, PT, read(root, PT).replace("Instale o Atlas.", "Instale o Atlas agora."));
		expect(finding(await docs(root), "docs-outdated")).toMatchObject({ severity: "error" });
	});

	test("green: English edited and the translation brought up to date (source_sha restamped)", async () => {
		const root = await feature();
		edit(root);
		await restamp(root);
		expect(await docs(root)).toEqual([]);
	});

	test("green: the escape `translation: pending` with an issue link; the page stays outdated (warning, site notice)", async () => {
		const root = await feature();
		edit(root);
		put(root, PT, read(root, PT).replace("source: getting-started.md\n", `source: getting-started.md\ntranslation: pending\nissue: ${ISSUE}\n`));
		const fs = await docs(root);
		expect(errors(fs)).toEqual([]);
		expect(finding(fs, "docs-outdated")).toMatchObject({ severity: "warn", file: PT });
		expect((await runStatus(loadCtx(root))).find((r) => r.surface === "docs")).toMatchObject({ fresh: 0, outdated: 1 });
	});

	test("the escape needs an issue URL, and an issue means nothing without it", async () => {
		const root = await feature();
		edit(root);
		const base = read(root, PT);
		put(root, PT, base.replace("source: getting-started.md\n", "source: getting-started.md\ntranslation: pending\n"));
		expect(finding(await docs(root), "docs-frontmatter")?.message).toContain("issue");
		put(root, PT, base.replace("source: getting-started.md\n", "source: getting-started.md\ntranslation: pending\nissue: soon\n"));
		expect(finding(await docs(root), "docs-frontmatter")?.message).toContain("issue");
		put(root, PT, base.replace("source: getting-started.md\n", `source: getting-started.md\nissue: ${ISSUE}\n`));
		expect(finding(await docs(root), "docs-frontmatter")?.message).toContain("only valid with");
	});

	test("a pending flag on a translation that is current is a warning to remove it", async () => {
		const root = await feature();
		edit(root);
		await restamp(root, `\ntranslation: pending\nissue: ${ISSUE}`);
		const fs = await docs(root);
		expect(errors(fs)).toEqual([]);
		expect(finding(fs, "docs-frontmatter")).toMatchObject({ severity: "warn" });
		expect(rulesOf(fs)).not.toContain("docs-outdated");
	});

	test("an English page the change does not touch stays a warning, even when it already lags behind main", async () => {
		const root = await makeRepo();
		edit(root); // the base already lags: English moved on, the translation did not
		must(root, "init", "-q", "-b", "main");
		must(root, "add", "-A");
		must(root, "commit", "-q", "-m", "base");
		must(root, "checkout", "-q", "-b", "feature");
		put(root, "docs/crew.md", "# Crew\n");
		put(root, "docs/pt-BR/crew.md", `---\ntitle: "Crew"\nsource: crew.md\nsource_sha: "${await blobSha(root, "docs/crew.md")}"\n---\n\n# Crew\n`);
		const fs = await docs(root);
		expect(finding(fs, "docs-outdated")).toMatchObject({ severity: "warn", file: PT });
		expect(errors(await check(root, "docs"))).toEqual([]);
	});

	test("a new English page needs its translation in the same change", async () => {
		const root = await feature();
		put(root, "docs/crew.md", "# Crew\n");
		expect(finding(await docs(root), "docs-missing")).toMatchObject({ severity: "error", file: "docs/pt-BR/crew.md" });
		put(root, "docs/pt-BR/crew.md", `---\ntitle: "Crew"\nsource: crew.md\nsource_sha: "${await blobSha(root, "docs/crew.md")}"\n---\n\n# Crew\n`);
		expect(await docs(root)).toEqual([]);
	});

	test("without a git base (a tarball, a detached CI run) it stays the old warning", async () => {
		const root = await makeRepo();
		edit(root);
		expect(finding(await docs(root), "docs-outdated")).toMatchObject({ severity: "warn" });
		expect(errors(await check(root, "docs"))).toEqual([]);
	});
});
