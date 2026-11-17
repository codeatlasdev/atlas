// The Rust surface: Fluent catalogs under crates/*/locales/<lang>/*.ftl, red on a bad file and green when repaired.
import { afterAll, describe, expect, test } from "bun:test";
import { fix } from "./fix.ts";
import { loadCtx } from "./types.ts";
import { check, cleanup, errors, finding, makeRepo, put, read, rulesOf } from "./testkit.ts";

afterAll(cleanup);

const EN = "crates/atlas-i18n/locales/en/cli.ftl";
const PT = "crates/atlas-i18n/locales/pt-BR/cli.ftl";

const ptWith = (root: string, edit: (s: string) => string) => put(root, PT, edit(read(root, PT)));

describe("Fluent catalogs", () => {
	test("a valid catalog (with an explicit zero variant) has no finding", async () => {
		const root = await makeRepo();
		expect((await check(root, "rust")).filter((f) => f.severity !== "info")).toEqual([]);
	});

	test("a message the locale lacks is missing; the lock-less rest stays quiet", async () => {
		const root = await makeRepo();
		ptWith(root, (s) => s.split("\n\n")[0]);
		expect(finding(await check(root, "rust"), "missing", "cli-files-count")).toMatchObject({ severity: "error", file: "crates/*/locales/pt-BR" });
	});

	test("changed English text is outdated; fix --restamp accepts a typo-only edit", async () => {
		const root = await makeRepo();
		put(root, EN, read(root, EN).replace("Language: { $lang }", "Language is { $lang }"));
		expect(finding(await check(root, "rust"), "outdated", "cli-lang-current")).toMatchObject({ severity: "error", file: PT });
		fix(loadCtx(root));
		expect(finding(await check(root, "rust"), "outdated", "cli-lang-current")).toBeDefined();
		fix(loadCtx(root), { restamp: ["cli-lang-current"] });
		expect(errors(await check(root, "rust"))).toEqual([]);
	});

	test("a comment edit or a layout change of the English does not outdate (the hash is of the message, not its comment)", async () => {
		const root = await makeRepo();
		put(root, EN, read(root, EN).replace("Header of the", "The header line of the").replace("{ $count ->", "{   $count ->"));
		expect(errors(await check(root, "rust"))).toEqual([]);
	});

	test("an orphan message is an error", async () => {
		const root = await makeRepo();
		put(root, PT, `${read(root, PT)}\ncli-gone = Sumiu\n`);
		expect(finding(await check(root, "rust"), "orphan", "cli-gone")).toBeDefined();
	});

	test("a dropped variable is a placeholder error", async () => {
		const root = await makeRepo();
		ptWith(root, (s) => s.replace("Idioma: { $lang }", "Idioma"));
		expect(finding(await check(root, "rust"), "placeholder", "cli-lang-current")?.message).toContain("{lang}");
	});

	test("pt-BR plural needs [one] and [other]", async () => {
		const root = await makeRepo();
		ptWith(root, (s) => s.replace("        [one] { $count } arquivo\n", "        [few] { $count } arquivinhos\n"));
		expect(finding(await check(root, "rust"), "plural", "cli-files-count")?.message).toContain("missing one");
		ptWith(root, (s) => s.replace("        [few] { $count } arquivinhos\n", ""));
		expect(finding(await check(root, "rust"), "plural", "cli-files-count")?.message).toContain("selector 1");
		ptWith(root, (s) => s.replace("{ $count ->", "{ $count }").replace(/\s+\[0\][\s\S]*$/, "\n"));
		expect(finding(await check(root, "rust"), "plural", "cli-files-count")).toBeDefined();
	});

	test("an English message without a description comment is an error; a comment equal to the text too", async () => {
		const root = await makeRepo();
		put(root, EN, read(root, EN).replace("# Header of the `atlas config lang` output\n", ""));
		expect(finding(await check(root, "rust"), "description", "cli-lang-current")?.severity).toBe("error");
		put(root, EN, `${read(root, EN)}\n# Save\ncli-save = Save\n`);
		expect(finding(await check(root, "rust"), "description", "cli-save")?.message).toContain("repeats");
	});

	test("Fluent syntax errors (junk) are errors with a line, not crashes", async () => {
		const root = await makeRepo();
		put(root, PT, `${read(root, PT)}\nbroken = { $oops\n`);
		const f = finding(await check(root, "rust"), "syntax");
		expect(f).toMatchObject({ severity: "error", file: PT });
		expect(f?.line).toBeGreaterThan(1);
	});

	test("an id defined twice in one locale is an error", async () => {
		const root = await makeRepo();
		put(root, PT, `${read(root, PT)}\ncli-lang-current = Outro: { $lang }\n`);
		expect(finding(await check(root, "rust"), "unique", "cli-lang-current")).toBeDefined();
	});

	test("ids are kebab-case", async () => {
		const root = await makeRepo();
		put(root, EN, `${read(root, EN)}\n# A probe\nCli_Bad = x\n`);
		expect(finding(await check(root, "rust"), "key", "Cli_Bad")).toBeDefined();
	});

	test("glossary, do-not-translate and pt-PT rules apply to .ftl too", async () => {
		const root = await makeRepo();
		put(root, EN, `${read(root, EN)}\n# Probe\ncli-probe = Open the branch\n`);
		put(root, PT, `${read(root, PT)}\ncli-probe = Guardar o ramo\n`);
		fix(loadCtx(root));
		const rules = rulesOf(await check(root, "rust"), "error");
		expect(rules).toContain("dnt");
		expect(rules).toContain("pt-pt");
	});

	test("a locale directory i18n/locales.json does not list is a config error", async () => {
		const root = await makeRepo();
		put(root, "crates/atlas-i18n/locales/fr/cli.ftl", "cli-lang-current = Langue: { $lang }\n");
		expect(finding(await check(root, "rust"), "config")?.message).toContain('"fr"');
	});

	test("attributes must match", async () => {
		const root = await makeRepo();
		put(root, EN, `${read(root, EN)}\n# A button with a tooltip\ncli-btn = Save\n    .title = Save the file\n`);
		put(root, PT, `${read(root, PT)}\ncli-btn = Salvar\n`);
		fix(loadCtx(root));
		expect(finding(await check(root, "rust"), "placeholder", "cli-btn")?.message).toContain(".title");
	});

	test("no crates/*/locales at all: the surface is simply absent", async () => {
		const root = await makeRepo({ [EN]: null, [PT]: null });
		expect((await check(root, "rust")).filter((f) => f.rule !== "hardcoded")).toEqual([]);
	});
});
