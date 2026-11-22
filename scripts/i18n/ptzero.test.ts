// pt-BR puts 0 in the CLDR category `one` (i = 0..1), so a pt-BR plural whose `one` variant prints the count says
// "0 arquivo" at zero. Rule `pt-zero` (error): such a message needs an exact-zero arm (`local countExact = count`,
// match `countExact=0, countPlural=*`) or `i18n-allow pt-zero: <why>` in its description. Incident: batch S06's
// review found "0 arquivo" and "0 coluna" in three messages the old `plural` rule let through.
import { afterAll, describe, expect, test } from "bun:test";
import { fix } from "./fix.ts";
import { loadCtx } from "./types.ts";
import { EN, META, PLURAL, PLURAL_ZERO, PT, addKey, check, cleanup, editJson, errors, finding, makeRepo, rulesOf } from "./testkit.ts";

afterAll(cleanup);

describe("pt-zero", () => {
	test("a one variant that prints the count and no exact zero is an error naming the fix", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void (o.app_files_count = PLURAL("{count} arquivo", "{count} arquivos")));
		fix(loadCtx(root));
		const f = finding(await check(root), "pt-zero", "app_files_count");
		expect(f).toMatchObject({ severity: "error", file: PT });
		expect(f?.message).toContain("countExact");
		expect(f?.message).toContain("i18n-allow pt-zero");
	});

	test("an exact zero (countExact=0) makes it valid, and so does keeping it in the English too", async () => {
		const root = await makeRepo();
		expect(errors(await check(root))).toEqual([]);
		addKey(root, "app_rows", PLURAL_ZERO("No rows", "{count} row", "{count} rows"), PLURAL_ZERO("Nenhuma linha", "{count} linha", "{count} linhas"), "Row count under the table; plural");
		expect(errors(await check(root))).toEqual([]);
	});

	test("an exact zero only in pt-BR needs no English twin (English says \"0 files\" fine)", async () => {
		const root = await makeRepo();
		const fs = await check(root);
		expect(rulesOf(fs, "error")).toEqual([]);
		expect(rulesOf(fs, "warn")).toEqual([]);
	});

	test("a one variant that does not print the count needs no zero", async () => {
		const root = await makeRepo();
		addKey(root, "app_one_left", PLURAL("One item left", "{count} items left"), PLURAL("Falta um item", "Faltam {count} itens"), "Hint under the list");
		expect(errors(await check(root))).toEqual([]);
	});

	test("a waiver with a reason in the description passes; a waiver with no reason does not", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void (o.app_files_count = PLURAL("{count} arquivo", "{count} arquivos")));
		editJson(root, META, (o) => void (o.app_files_count = "Count of changed files; plural. i18n-allow pt-zero: the commit bar hides itself at 0 (CommitBar.svelte, `{#if files.length}`)"));
		fix(loadCtx(root));
		expect(errors(await check(root))).toEqual([]);
		editJson(root, META, (o) => void (o.app_files_count = "Count of changed files; plural. i18n-allow pt-zero:"));
		fix(loadCtx(root));
		expect(finding(await check(root), "pt-zero", "app_files_count")?.message).toContain("reason");
	});

	test("the English is never judged: \"0 files\" is fine in English", async () => {
		const root = await makeRepo();
		editJson(root, EN, (o) => void (o.app_files_count = PLURAL("{count} file", "{count} files")));
		fix(loadCtx(root));
		expect(rulesOf(errors(await check(root)))).not.toContain("pt-zero");
	});

	test("a one variant behind an os select (or a referenced term) is judged on every platform", async () => {
		const root = await makeRepo();
		const os = (one: string, other: string) => [
			{
				declarations: ["input os", "input count", "local countPlural = count: plural"],
				selectors: ["os", "countPlural"],
				match: { "os=mac, countPlural=one": one, "os=*, countPlural=one": one, "os=mac, countPlural=*": other, "os=*, countPlural=*": other },
			},
		];
		addKey(root, "app_moved", os("{count} item", "{count} items"), os("{count} item", "{count} itens"), "Toast after a delete; plural, os");
		expect(finding(await check(root), "pt-zero", "app_moved")).toBeDefined();
	});

	test("a number on the plural selector itself is an error: only categories match there (Rust compares categories)", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => {
			const v = PLURAL("{count} arquivo", "{count} arquivos");
			v[0].match["countPlural=0" as "countPlural=one"] = "Nenhum arquivo";
			o.app_files_count = v;
		});
		fix(loadCtx(root));
		const fs = await check(root);
		expect(finding(fs, "plural", "app_files_count")?.message).toContain("countExact");
		expect(finding(fs, "pt-zero", "app_files_count")).toBeDefined();
	});

	test("pt-BR may carry the exact-zero selector the English lacks (no plural shape error)", async () => {
		const root = await makeRepo();
		// the shape comparison must accept pt-BR's extra exact-zero selector, or the fix could never be written
		expect(rulesOf(await check(root))).not.toContain("plural");
	});
});
