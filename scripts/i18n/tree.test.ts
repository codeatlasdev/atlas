// The rules that look across the source tree: each goes red on a broken real temp tree and green when repaired.
// (Per-area rules are in rules.test.ts, platform wording in platform.test.ts.)
import { afterAll, describe, expect, test } from "bun:test";
import { fix } from "./fix.ts";
import { type Finding, loadCtx } from "./types.ts";
import { CLI_EN, CLI_META, EN, META, PT, addKey, area, check, cleanup, editJson, errors, finding, json, makeRepo, put, read, rulesOf } from "./testkit.ts";

afterAll(cleanup);

const SURFACES = "i18n/surfaces.json";
const surfaces = (root: string, edit: (j: { budget: { keys: number; bytes: number }; generatedAreas: Record<string, string>; surfaces: Record<string, Record<string, unknown>> }) => void) => {
	const j = JSON.parse(read(root, SURFACES));
	edit(j);
	put(root, SURFACES, `${JSON.stringify(j, null, "\t")}\n`);
};
const all = (fs: Finding[], rule: string) => fs.filter((f) => f.rule === rule);

describe("dup-common: an English text equal to a common message", () => {
	test.each([["Cancel"], ["cancel"], ["  CANCEL "], ["Cancel."], ["Cancel…"]])("%p is an error that names the common key", async (text) => {
		const root = await makeRepo();
		addKey(root, "app_dismiss", text, "Dispensar", "Dismiss button of the form");
		const f = finding(await check(root), "dup-common", "app_dismiss");
		expect(f).toMatchObject({ severity: "error", file: EN });
		expect(f?.message).toContain("common_action_cancel");
	});

	test("the meta `distinct` field or `i18n-distinct:` in the description says why it differs", async () => {
		const root = await makeRepo();
		addKey(root, "app_dismiss", "Cancel", "Cancelar", "Cancels a running download, unlike the generic Cancel. i18n-distinct: it stops a process");
		expect(rulesOf(await check(root))).not.toContain("dup-common");
		editJson(root, META, (o) => void (o.app_dismiss = { description: "Cancels a running download", distinct: "it stops a process" }));
		expect(rulesOf(await check(root))).not.toContain("dup-common");
		editJson(root, META, (o) => void (o.app_dismiss = "Cancels a running download"));
		expect(rulesOf(await check(root))).toContain("dup-common");
	});

	test("a different text, a variants message, and the common areas themselves are not flagged", async () => {
		const root = await makeRepo();
		addKey(root, "app_dismiss", "Cancel upload", "Cancelar envio", "Button that cancels the upload");
		expect(rulesOf(await check(root))).not.toContain("dup-common");
	});
});

describe("dup-value: the same English text under two non-common keys", () => {
	test("is a warning that names the first key; marking one distinct silences it", async () => {
		const root = await makeRepo();
		addKey(root, "app_first_label", "Project name", "Nome do projeto", "Label of the name field");
		addKey(root, "app_second_label", "Project name", "Nome do projeto", "Label of the name field in the dialog");
		const f = finding(await check(root), "dup-value", "app_second_label");
		expect(f?.severity).toBe("warn");
		expect(f?.message).toContain("app_first_label");
		expect(errors(await check(root))).toEqual([]);
		editJson(root, META, (o) => void (o.app_second_label = { description: "Label in the dialog", distinct: "a different field" }));
		expect(rulesOf(await check(root))).not.toContain("dup-value");
	});
});

describe("budget: keys or bytes per area file", () => {
	test("too many keys names the area and suggests sub-areas by prefix", async () => {
		const root = await makeRepo();
		surfaces(root, (j) => void (j.budget.keys = 6));
		for (const g of ["alpha", "beta"]) for (let i = 0; i < 3; i++) addKey(root, `app_${g}_${i}`, `${g} text ${i}`, `${g} texto ${i}`, `${g} description ${i}`);
		const f = all(await check(root), "budget");
		expect(f.length).toBeGreaterThan(0);
		expect(f.every((x) => x.severity === "error")).toBe(true);
		expect(f[0].message).toContain("app_alpha (3 keys)");
		expect(f[0].message).toContain("i18n:mv");
		expect(f.map((x) => x.file)).toEqual(expect.arrayContaining([EN, PT]));
	});

	test("too many bytes is the same error; under the budget is clean", async () => {
		const root = await makeRepo();
		expect(rulesOf(await check(root))).not.toContain("budget");
		surfaces(root, (j) => void (j.budget.bytes = 300));
		expect(all(await check(root), "budget").some((f) => /bytes \(budget/.test(f.message))).toBe(true);
	});
});

describe("key-area and key naming", () => {
	test("a key that does not start with its area is key-area; one that is not snake_case is key", async () => {
		const root = await makeRepo();
		editJson(root, EN, (o) => void (o.other_thing = "X"));
		const f = finding(await check(root), "key-area", "other_thing");
		expect(f?.message).toContain('starts with "app_"');
		editJson(root, EN, (o) => void delete o.other_thing);
		editJson(root, EN, (o) => void (o.AppThing = "X"));
		expect(finding(await check(root), "key", "AppThing")).toBeDefined();
	});

	test("the longest area prefix owns the key: common_os_* does not belong in common", async () => {
		const root = await makeRepo();
		editJson(root, area("common", "en"), (o) => void (o.common_os_other = "Other"));
		const f = finding(await check(root), "key-area", "common_os_other");
		expect(f?.message).toContain('area "common_os"');
	});
});

describe("dup-key: a key defined in two areas", () => {
	test("is an error on both, naming both areas", async () => {
		const root = await makeRepo();
		editJson(root, CLI_EN, (o) => void (o.app_save_button = "Save"));
		const fs = all(await check(root), "dup-key");
		expect(fs.map((f) => f.file).sort()).toEqual([CLI_EN, EN].sort());
		expect(fs[0].message).toContain("app and cli");
		expect(fs[0].message).toContain("i18n:mv");
	});
});

describe("common areas hold terms, not sentences", () => {
	const common = area("common", "en");
	test.each([
		["more than four words", "This is a very long text"],
		["a sentence end inside", "Done. Next"],
		["a closing full stop", "All good."],
		["a question inside", "Sure? Go"],
	])("%s is an error unless meta says why", async (_n, text) => {
		const root = await makeRepo();
		editJson(root, common, (o) => void (o.common_state_note = text));
		editJson(root, area("common", "pt-BR"), (o) => void (o.common_state_note = "Texto"));
		editJson(root, area("common", "meta"), (o) => void (o.common_state_note = "A note"));
		expect(finding(await check(root), "common-sentence", "common_state_note")?.severity).toBe("error");
		editJson(root, area("common", "meta"), (o) => void (o.common_state_note = { description: "A note", sentence: "it is the empty-state hint" }));
		expect(rulesOf(await check(root))).not.toContain("common-sentence");
	});

	test("terms, placeholders and ellipses pass; a `sentence` without a reason is a meta error", async () => {
		const root = await makeRepo();
		editJson(root, common, (o) => void (o.common_state_loading = "Loading…"));
		editJson(root, area("common", "pt-BR"), (o) => void (o.common_state_loading = "Carregando…"));
		editJson(root, area("common", "meta"), (o) => void (o.common_state_loading = "Shown while loading"));
		expect(rulesOf(await check(root))).not.toContain("common-sentence");
		editJson(root, area("common", "meta"), (o) => void (o.common_state_loading = { description: "x", sentence: "" }));
		expect((await check(root)).some((f) => f.rule === "description" && f.message.includes("sentence"))).toBe(true);
	});
});

describe("references between messages", () => {
	test("a reference to a message that does not exist is dangling-ref", async () => {
		const root = await makeRepo();
		addKey(root, "app_toast", "Moved to {@common_os_nothing}", "Movido para {@common_os_nothing}", "Toast");
		const f = finding(await check(root), "dangling-ref", "app_toast");
		expect(f?.message).toContain("common_os_nothing");
		editJson(root, EN, (o) => void (o.app_toast = "Moved to {@common_os_trash}"));
		editJson(root, PT, (o) => void (o.app_toast = "Movido para {@common_os_trash}"));
		fix(loadCtx(root), { restamp: ["app_toast"] });
		expect(rulesOf(errors(await check(root)))).toEqual([]);
	});

	test("a cycle is dangling-ref and names the loop", async () => {
		const root = await makeRepo();
		addKey(root, "app_a_loop", "A {@app_b_loop}", "A {@app_b_loop}", "First");
		addKey(root, "app_b_loop", "B {@app_a_loop}", "B {@app_a_loop}", "Second");
		const f = all(await check(root), "dangling-ref");
		expect(f.length).toBeGreaterThan(0);
		expect(f[0].message).toContain("cycle");
	});

	test("a reference may only reach a common area or an area the same surfaces consume", async () => {
		const root = await makeRepo();
		addKey(root, "app_uses_cli", "Hello {@cli_lang_current}", "Olá {@cli_lang_current}", "Mixes a CLI line into a screen");
		const f = finding(await check(root), "dangling-ref", "app_uses_cli");
		expect(f?.message).toContain("desktop does not consume");
	});

	test("the cross product: a reference inside a plural message composes with it", async () => {
		const root = await makeRepo();
		const en = [{ declarations: ["input count", "local countPlural = count: plural"], selectors: ["countPlural"], match: { "countPlural=one": "{count} file moved to {@common_os_trash}", "countPlural=*": "{count} files moved to {@common_os_trash}" } }];
		const pt = [{ declarations: ["input count", "local countPlural = count: plural"], selectors: ["countPlural"], match: { "countPlural=one": "{count} arquivo movido para {@common_os_trash}", "countPlural=*": "{count} arquivos movidos para {@common_os_trash}" } }];
		addKey(root, "app_moved", en, pt, "Toast after deleting files");
		expect(errors(await check(root))).toEqual([]);
	});
});

describe("areas and surfaces", () => {
	test("an area no surface lists is unconsumed-area; listing it fixes it", async () => {
		const root = await makeRepo();
		put(root, area("orphaned", "en"), json({ orphaned_thing: "Thing" }));
		put(root, area("orphaned", "pt-BR"), json({ orphaned_thing: "Coisa" }));
		put(root, area("orphaned", "meta"), json({ orphaned_thing: "A thing" }));
		fix(loadCtx(root));
		expect(finding(await check(root), "unconsumed-area")?.file).toBe("i18n/messages/orphaned");
		surfaces(root, (j) => void j.surfaces.desktop.areas.push("orphaned"));
		expect(rulesOf(await check(root))).not.toContain("unconsumed-area");
	});

	test("a surface listing an area that is not there, or a common one, is a config error", async () => {
		const root = await makeRepo();
		surfaces(root, (j) => void (j.surfaces.desktop.areas = ["app", "nope", "common"]));
		const msgs = all(await check(root), "config").map((f) => f.message);
		expect(msgs.some((m) => m.includes('"nope"'))).toBe(true);
		expect(msgs.some((m) => m.includes("consumed by every surface"))).toBe(true);
	});

	test("surfaces.json that is not JSON, or a surface with two outputs, is a config error, not a crash", async () => {
		const root = await makeRepo();
		put(root, SURFACES, "{ nope");
		expect(finding(await check(root), "config")?.message).toContain("invalid JSON");
		put(root, SURFACES, JSON.stringify({ surfaces: { desktop: { areas: ["app"], paraglide: "a", rust: "b" } } }));
		expect((await check(root)).some((f) => f.rule === "config" && f.message.includes("exactly one of"))).toBe(true);
	});

	test("an area directory that is not snake_case is a config error", async () => {
		const root = await makeRepo();
		put(root, "i18n/messages/Bad-Area/en.json", "{}\n");
		expect((await check(root)).some((f) => f.rule === "config" && f.file === "i18n/messages/Bad-Area")).toBe(true);
	});

	test("--surface limits the area rules to the areas the surface consumes (common included)", async () => {
		const root = await makeRepo();
		editJson(root, CLI_EN, (o) => void delete o.cli_lang_current);
		expect(errors(await check(root, "desktop")).some((f) => f.file === CLI_EN)).toBe(false);
		expect(errors(await check(root, "rust")).some((f) => f.file.includes("cli"))).toBe(true);
	});
});

describe("unused is per consuming surface", () => {
	test("a key referenced only by a surface that does not consume its area is unused", async () => {
		const root = await makeRepo();
		// app_save_button is a desktop key: a Rust string literal of the same name does not keep it alive
		put(root, "apps/desktop/src/App.svelte", read(root, "apps/desktop/src/App.svelte").replace("{m.app_save_button()}", ""));
		put(root, "crates/atlas-cli/src/lit.rs", 'const NAME: &str = "app_save_button";\n');
		expect(finding(await check(root), "unused", "app_save_button")).toMatchObject({ severity: "error", file: EN });
		// ...and the desktop referencing it again clears it
		put(root, "apps/desktop/src/App.svelte", `${read(root, "apps/desktop/src/App.svelte")}<b>{m.app_save_button()}</b>\n`);
		expect(rulesOf(await check(root))).not.toContain("unused");
	});

	test("an i18n-keys annotation keeps a dynamically used key alive", async () => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/App.svelte", read(root, "apps/desktop/src/App.svelte").replace("{m.app_open_branch()}", ""));
		expect(finding(await check(root), "unused", "app_open_branch")).toBeDefined();
		put(root, "apps/desktop/src/dyn.ts", "// i18n-keys: app_open_*\n");
		expect(rulesOf(await check(root))).not.toContain("unused");
	});

	test("common areas and generated areas are exempt", async () => {
		const root = await makeRepo();
		expect(rulesOf(await check(root))).not.toContain("unused");
		surfaces(root, (j) => void (j.generatedAreas.cli = "UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n"));
		put(root, "crates/atlas-i18n/src/lib.rs", "pub fn nothing() {}\n");
		expect(rulesOf(await check(root))).not.toContain("unused");
		editJson(root, CLI_EN, (o) => void (o.cli_orphan_line = "x"));
		const hint = finding(await check(root), "orphan");
		expect(hint).toBeUndefined();
		editJson(root, area("cli", "pt-BR"), (o) => void (o.cli_only_pt = "x"));
		expect(finding(await check(root), "orphan", "cli_only_pt")?.message).toContain("UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n");
	});

	test("cli_* areas are exempt (their ids are computed from the clap tree)", async () => {
		const root = await makeRepo();
		for (const f of ["en", "pt-BR", "meta"]) {
			put(root, area("cli_help", f), json({ cli_help_about: f === "en" ? "Show help" : f === "pt-BR" ? "Mostra a ajuda" : "Help text" }));
		}
		surfaces(root, (j) => void (j.surfaces.rust.areas as string[]).push("cli_help"));
		fix(loadCtx(root));
		expect(rulesOf(await check(root))).not.toContain("unused");
	});
});

describe("undefined-key and unconsumed-key: references in the sources", () => {
	test("m.key( with a key that is not in the tree names file:line and the commands to find or add it", async () => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/Typo.svelte", '<script>\n  import * as m from "$lib/paraglide/messages";\n</script>\n<p>{m.app_save_buton()}</p>\n');
		const f = finding(await check(root), "undefined-key", "app_save_buton");
		expect(f).toMatchObject({ severity: "error", file: "apps/desktop/src/Typo.svelte", line: 4 });
		expect(f?.message).toContain("bun run i18n:find app_save_buton");
		expect(f?.message).toContain("bun run i18n:add app_save_buton");
	});

	test('m["key"] and the { m } import form are seen too; a file that does not import the messages is not scanned', async () => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/a.ts", 'import { m } from "$lib/paraglide/messages.js";\nexport const x = m["app_nothing_here"]();\n');
		put(root, "apps/desktop/src/b.ts", "const m = new Map();\nm.app_not_a_message();\n");
		const fs = all(await check(root), "undefined-key");
		expect(fs.map((f) => f.file)).toEqual(["apps/desktop/src/a.ts"]);
	});

	test("t!(\"key\") in Rust: undefined is an error; comments, test code and i18n-ignore are skipped", async () => {
		const root = await makeRepo();
		put(root, "crates/atlas-cli/src/x.rs", 'pub fn f() {\n    let a = t!("cli_missing_one");\n    // t!("cli_in_comment")\n    /// t!("cli_in_doc")\n    let b = t!("cli_ignored"); // i18n-ignore\n}\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = t!("cli_in_test"); }\n}\n');
		put(root, "crates/atlas-cli/tests/y.rs", 't!("cli_in_tests_dir");\n');
		const fs = all(await check(root), "undefined-key");
		expect(fs.map((f) => `${f.key}@${f.line}`)).toEqual(["cli_missing_one@2"]);
	});

	test("a key that exists in an area the surface does not consume is unconsumed-key", async () => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/Wrong.svelte", '<script>\n  import * as m from "$lib/paraglide/messages";\n</script>\n<p>{m.cli_lang_current({ lang: "x" })}</p>\n');
		const f = finding(await check(root), "unconsumed-key", "cli_lang_current");
		expect(f?.message).toContain("area cli");
		expect(f?.message).toContain("i18n/surfaces.json");
		expect(rulesOf(await check(root))).not.toContain("undefined-key");
	});

	test("a Rust t! of a common key is fine; so is one of its own area", async () => {
		const root = await makeRepo();
		put(root, "crates/atlas-cli/src/ok.rs", 'fn f() { t!("common_action_cancel"); t!("cli_lang_current"); }\n');
		expect(rulesOf(await check(root))).not.toContain("undefined-key");
	});
});

describe("supported-sync: every list of languages agrees with i18n/locales.json", () => {
	test("lang.rs, Info.plist and the project settings each go red alone, with the command", async () => {
		const root = await makeRepo();
		expect(rulesOf(await check(root))).not.toContain("supported-sync");

		put(root, "crates/atlas-core/src/lang.rs", 'pub const SUPPORTED: &[&str] = &["en"];\n');
		const lang = finding(await check(root), "supported-sync");
		expect(lang?.file).toBe("crates/atlas-core/src/lang.rs");
		expect(lang?.message).toContain("missing pt-BR");
		expect(lang?.message).toContain("bun run i18n:add-locale");
		put(root, "crates/atlas-core/src/lang.rs", 'pub const SUPPORTED: &[&str] = &["en", "pt-BR"];\n');

		put(root, "crates/atlas-desktop/Info.plist", "<plist><dict><key>CFBundleLocalizations</key><array><string>en</string><string>pt-BR</string><string>fr</string></array></dict></plist>");
		const plist = finding(await check(root), "supported-sync");
		expect(plist?.file).toBe("crates/atlas-desktop/Info.plist");
		expect(plist?.message).toContain("extra fr");
		put(root, "crates/atlas-desktop/Info.plist", "<plist><dict><key>CFBundleLocalizations</key><array><string>en</string><string>pt-BR</string></array></dict></plist>");

		editJson(root, "apps/desktop/project.inlang/settings.json", (o) => void (o.locales = ["en"]));
		expect(finding(await check(root), "supported-sync")?.file).toBe("apps/desktop/project.inlang/settings.json");
		editJson(root, "apps/desktop/project.inlang/settings.json", (o) => void (o.locales = ["en", "pt-BR"]));
		expect(rulesOf(await check(root))).not.toContain("supported-sync");
	});

	test("a missing file, or a lang.rs without the const, is how a tree without that part looks, not an error", async () => {
		const root = await makeRepo({ "crates/atlas-core/src/lang.rs": null, "crates/atlas-desktop/Info.plist": null, "apps/desktop/project.inlang/settings.json": null });
		expect(rulesOf(await check(root))).not.toContain("supported-sync");
		put(root, "crates/atlas-core/src/lang.rs", "// no list here\n");
		expect(finding(await check(root), "supported-sync")?.message).toContain("is []");
	});
});

describe("meta.json: limits and flags", () => {
	test("maxLength fails a text that is too long, in English and in every translation", async () => {
		const root = await makeRepo();
		editJson(root, META, (o) => void (o.app_save_button = { description: "Toolbar button that saves", maxLength: 3 }));
		expect(all(await check(root), "length").map((f) => f.file).sort()).toEqual([EN, PT].sort());
		editJson(root, META, (o) => void (o.app_save_button = { description: "Toolbar button that saves", maxLength: 12 }));
		expect(rulesOf(await check(root))).not.toContain("length");
	});

	test("an English key without meta, and meta for a key that is gone, are description errors with the command", async () => {
		const root = await makeRepo();
		editJson(root, META, (o) => void delete o.app_welcome);
		expect(finding(await check(root), "description", "app_welcome")?.message).toContain("bun run i18n:add");
	});
});

describe("generated areas: the English is not edited by hand", () => {
	test("an outdated or orphan finding of a generated area points at the generator command", async () => {
		const root = await makeRepo();
		surfaces(root, (j) => void (j.generatedAreas.cli = "UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n"));
		editJson(root, CLI_EN, (o) => void (o.cli_lang_current = "Language is: {lang}"));
		editJson(root, CLI_META, () => {});
		const f = finding(await check(root), "outdated", "cli_lang_current");
		expect(f?.message).toContain("UPDATE_FIXTURES=1 cargo test -p atlas-cli cli_i18n");
		expect(f?.message).toContain("never by hand");
	});
});
