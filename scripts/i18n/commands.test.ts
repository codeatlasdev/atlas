// i18n:find, i18n:add, i18n:mv: what they change, what they refuse (writing nothing), and that the result passes i18n:check.
import { afterAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { addMessage } from "./add.ts";
import { find } from "./find.ts";
import { moveKey, rewriteUses } from "./mv.ts";
import { loadSurfacesConfig } from "./source.ts";
import { loadCtx } from "./types.ts";
import { CLI_EN, CLI_META, EN, META, PT, SURFACES, addKey, area, check, cleanup, editJson, errors, finding, makeRepo, put, read, rulesOf } from "./testkit.ts";

afterAll(cleanup);

const LOCK = "i18n/messages/app/lock.json";
const cli = async (script: string, root: string, args: string[]) => {
	const p = Bun.spawn(["bun", join(import.meta.dir, script), "--root", root, ...args], { stdout: "pipe", stderr: "pipe" });
	const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
	return { out, err, code };
};
const snapshot = (root: string) => [EN, PT, META, LOCK, "i18n/surfaces.json", "apps/desktop/src/App.svelte"].map((f) => read(root, f)).join("\u0000");

describe("find", () => {
	test("a key gives its file:line, its values, its description and where the sources use it", async () => {
		const root = await makeRepo();
		const r = find(loadCtx(root), "app_save_button");
		expect(r.mode).toBe("key");
		expect(r.matches).toHaveLength(1);
		expect(r.matches[0]).toMatchObject({ key: "app_save_button", area: "app", file: EN, how: "key", values: { en: "Save", "pt-BR": "Salvar" } });
		expect(r.matches[0].line).toBeGreaterThan(0);
		expect(r.matches[0].description).toContain("saves the open file");
		expect(r.usages).toEqual([{ surface: "desktop", file: "apps/desktop/src/App.svelte", line: 5 }]);
	});

	test("the Rust usages are found too, and a key used nowhere says so", async () => {
		const root = await makeRepo();
		expect(find(loadCtx(root), "cli_lang_current").usages).toEqual([{ surface: "rust", file: "crates/atlas-i18n/src/lib.rs", line: 2 }]);
		addKey(root, "app_unused_one", "Unused", "Nao usada", "Nothing uses it");
		expect(find(loadCtx(root), "app_unused_one").usages).toEqual([{ surface: "desktop", file: "apps/desktop/src/App.svelte", line: 7 }]);
	});

	test("a key prefix lists the keys under it", async () => {
		const root = await makeRepo();
		const r = find(loadCtx(root), "app_open_");
		expect(r.matches.map((m) => m.key)).toEqual(["app_open_branch", "app_open_docs"]);
		expect(r.matches.every((m) => m.how === "prefix")).toBe(true);
	});

	test("a text: equal first, then contains, then similar; any locale's text counts", async () => {
		const root = await makeRepo();
		const equal = find(loadCtx(root), "save");
		expect(equal.mode).toBe("text");
		expect(equal.matches[0]).toMatchObject({ key: "app_save_button", how: "equal" });
		expect(find(loadCtx(root), "salvar").matches[0]).toMatchObject({ key: "app_save_button", how: "equal" });
		expect(find(loadCtx(root), "branch").matches.map((m) => m.key)).toContain("app_open_branch");
		const similar = find(loadCtx(root), "Welcome to the Atlas app");
		expect(similar.matches[0]).toMatchObject({ key: "app_welcome", how: "similar" });
		expect(find(loadCtx(root), "Cancel").matches[0]).toMatchObject({ key: "common_action_cancel", how: "equal", area: "common" });
	});

	test("nothing matches: no result, and an empty query is a CommandError naming the add command", async () => {
		const root = await makeRepo();
		expect(find(loadCtx(root), "zzzz qqqq").matches).toEqual([]);
		expect(() => find(loadCtx(root), "  ")).toThrow("bun run i18n:add");
	});

	test("the CLI: --json is parseable, a miss exits 1 with the add command, a hit exits 0", async () => {
		const root = await makeRepo();
		const hit = await cli("find.ts", root, ["Save", "--json"]);
		expect(hit.code).toBe(0);
		expect(JSON.parse(hit.out).matches[0].key).toBe("app_save_button");
		const miss = await cli("find.ts", root, ["Totally new words here", "--json"]);
		expect(miss.code).toBe(1);
		expect(JSON.parse(miss.err).error).toContain('bun run i18n:add <key> "Totally new words here" --desc');
		const human = await cli("find.ts", root, ["app_welcome"]);
		expect(human.out).toContain("app_welcome  [key]  i18n/messages/app/en.json:");
		expect(human.out).toContain("en: Welcome to Atlas, {name}");
	});
});

describe("add", () => {
	const base = { description: "Label of the new field in Settings" };

	test("adds to the area of the key (longest prefix), sorted, with meta and a stamped lock; then check passes", async () => {
		const root = await makeRepo();
		const r = addMessage(loadCtx(root), { key: "app_new_field", english: "New field", pt: "Novo campo", ...base });
		expect(r).toMatchObject({ key: "app_new_field", area: "app", createdArea: false });
		expect(Object.keys(JSON.parse(read(root, EN)))).toEqual([...Object.keys(JSON.parse(read(root, EN)))].sort());
		expect(JSON.parse(read(root, EN)).app_new_field).toBe("New field");
		expect(JSON.parse(read(root, PT)).app_new_field).toBe("Novo campo");
		expect(JSON.parse(read(root, META)).app_new_field).toBe(base.description);
		expect(JSON.parse(read(root, LOCK))["pt-BR"].app_new_field).toMatch(/^[0-9a-f]{12}$/);
		put(root, "apps/desktop/src/App.svelte", `${read(root, "apps/desktop/src/App.svelte")}<i>{m.app_new_field()}</i>\n`);
		expect(errors(await check(root))).toEqual([]);
	});

	test("without --pt-BR the English is added and check asks for the translation with its command", async () => {
		const root = await makeRepo();
		addMessage(loadCtx(root), { key: "app_new_field", english: "New field", ...base });
		const f = finding(await check(root), "missing", "app_new_field");
		expect(f?.message).toContain("bun run i18n:translate --locale pt-BR --area app");
	});

	test("a common area key is allowed, and its text becomes available everywhere", async () => {
		const root = await makeRepo();
		addMessage(loadCtx(root), { key: "common_action_close", english: "Close", pt: "Fechar", description: "Button that closes a dialog" });
		expect(JSON.parse(read(root, area("common", "en"))).common_action_close).toBe("Close");
		expect(errors(await check(root))).toEqual([]);
	});

	test.each([
		["an existing key", { key: "app_save_button", english: "Other" }, "already exists in i18n/messages/app/en.json: see it with bun run i18n:find app_save_button"],
		["a key that is not snake_case", { key: "AppThing", english: "X" }, "is not a key"],
		["an empty English text", { key: "app_thing", english: " " }, "English text is empty"],
		["a missing description", { key: "app_thing", english: "Thing", description: "" }, "--desc"],
		["a text that equals a common message", { key: "app_dismiss", english: "Cancel" }, 'is already common_action_cancel: use it (m.common_action_cancel() / t!("common_action_cancel")), or add it with --distinct'],
		["a key with no area", { key: "zzz_thing", english: "Thing" }, "no area for"],
		["a translation that drops a variable", { key: "app_hello", english: "Hello {name}", pt: "Olá" }, "placeholder"],
		["a pt-PT translation", { key: "app_thing", english: "Thing", pt: "Guardar" }, "pt-pt"],
		["a surface that does not consume the area", { key: "cli_thing", english: "Thing", surface: "desktop" }, "is not consumed by desktop"],
	])("refuses %s and writes nothing", async (_n, o, message) => {
		const root = await makeRepo();
		const before = snapshot(root);
		expect(() => addMessage(loadCtx(root), { description: "A description", ...o })).toThrow(message);
		expect(snapshot(root)).toBe(before);
	});

	test("--distinct lets a text equal a common one in, and records why", async () => {
		const root = await makeRepo();
		addMessage(loadCtx(root), { key: "app_dismiss", english: "Cancel", pt: "Cancelar", description: "Stops a running download", distinct: "it stops a process" });
		expect(JSON.parse(read(root, META)).app_dismiss).toEqual({ description: "Stops a running download", distinct: "it stops a process" });
		put(root, "apps/desktop/src/App.svelte", `${read(root, "apps/desktop/src/App.svelte")}<i>{m.app_dismiss()}</i>\n`);
		expect(errors(await check(root))).toEqual([]);
	});

	test("an unknown area needs --new-area and a surface; with both it creates the folder with an area.json naming the surface; surfaces.json is not touched", async () => {
		const root = await makeRepo();
		const before = snapshot(root);
		expect(() => addMessage(loadCtx(root), { key: "billing_plan_title", english: "Plan", description: "Title of the plan card" })).toThrow("create billing with --new-area --surface <desktop|rust>");
		expect(() => addMessage(loadCtx(root), { key: "billing_plan_title", english: "Plan", description: "Title", newArea: true })).toThrow("--new-area needs --surface");
		expect(snapshot(root)).toBe(before);
		const r = addMessage(loadCtx(root), { key: "billing_plan_title", english: "Plan", pt: "Plano", description: "Title of the plan card", newArea: true, surface: "desktop" });
		expect(r).toMatchObject({ area: "billing", createdArea: true });
		expect(JSON.parse(read(root, "i18n/messages/billing/en.json"))).toEqual({ billing_plan_title: "Plan" });
		expect(JSON.parse(read(root, "i18n/messages/billing/area.json"))).toEqual({ surface: "desktop" });
		expect(loadSurfacesConfig(root).config.surfaces.desktop.areas).toEqual(["app", "billing"]);
		expect(read(root, "i18n/surfaces.json")).toBe(SURFACES);
		put(root, "apps/desktop/src/B.svelte", '<script>\n  import * as m from "$lib/paraglide/messages";\n</script>\n{m.billing_plan_title()}\n');
		expect(errors(await check(root))).toEqual([]);
	});

	test("the CLI: --json on success, a refusal exits 1 with the fix in the error", async () => {
		const root = await makeRepo();
		const ok = await cli("add.ts", root, ["app_cli_made", "Made by the CLI", "--desc", "A probe", "--pt-BR", "Feito pela CLI", "--json"]);
		expect(ok.code).toBe(0);
		expect(JSON.parse(ok.out)).toMatchObject({ key: "app_cli_made", area: "app" });
		const bad = await cli("add.ts", root, ["app_cli_made", "Again", "--desc", "x", "--json"]);
		expect(bad.code).toBe(1);
		expect(JSON.parse(bad.err).error).toContain("bun run i18n:find app_cli_made");
		const human = await cli("add.ts", root, ["app_other_thing", "Other thing", "--desc", "A probe"]);
		expect(human.out).toContain("pt-BR is required: bun run i18n:translate --locale pt-BR --area app");
	});
});

describe("mv", () => {
	test("renames within an area: values, meta and the lock stamp move; a stale translation stays stale; call sites follow", async () => {
		const root = await makeRepo();
		editJson(root, EN, (o) => void (o.app_save_button = "Save it"));
		const stamp = JSON.parse(read(root, LOCK))["pt-BR"].app_save_button;
		const r = moveKey(loadCtx(root), "app_save_button", "app_save_action");
		expect(r).toMatchObject({ fromArea: "app", toArea: "app" });
		expect(JSON.parse(read(root, EN)).app_save_action).toBe("Save it");
		expect(JSON.parse(read(root, PT)).app_save_action).toBe("Salvar");
		expect(JSON.parse(read(root, META)).app_save_action).toContain("saves the open file");
		expect(Object.keys(JSON.parse(read(root, EN)))).not.toContain("app_save_button");
		const lock = JSON.parse(read(root, LOCK))["pt-BR"];
		expect(lock.app_save_action).toBe(stamp);
		expect(lock.app_save_button).toBeUndefined();
		expect(read(root, "apps/desktop/src/App.svelte")).toContain("{m.app_save_action()}");
		expect(r.updated).toEqual([{ file: "apps/desktop/src/App.svelte", line: 5 }]);
		expect(finding(await check(root), "outdated", "app_save_action")).toBeDefined();
		expect(rulesOf(errors(await check(root)))).toEqual(["outdated"]);
	});

	test("moves across areas, in every locale, and rewrites t!(\"old\") in Rust", async () => {
		const root = await makeRepo();
		const r = moveKey(loadCtx(root), "cli_lang_current", "app_lang_current");
		expect(r).toMatchObject({ fromArea: "cli", toArea: "app" });
		expect(Object.keys(JSON.parse(read(root, CLI_EN)))).toEqual(["cli_files_count"]);
		expect(JSON.parse(read(root, EN)).app_lang_current).toBe("Language: {lang}");
		expect(JSON.parse(read(root, PT)).app_lang_current).toBe("Idioma: {lang}");
		expect(JSON.parse(read(root, META)).app_lang_current).toContain("atlas config lang");
		expect(JSON.parse(read(root, CLI_META)).cli_lang_current).toBeUndefined();
		expect(Object.keys(JSON.parse(read(root, "i18n/messages/cli/lock.json"))["pt-BR"])).toEqual(["cli_files_count"]);
		expect(read(root, "crates/atlas-i18n/src/lib.rs")).toContain('t!("app_lang_current", lang = "en")');
		expect(r.updated).toContainEqual({ file: "crates/atlas-i18n/src/lib.rs", line: 2 });
	});

	test("rewrites a t_in!(tag, \"old\") call (the desktop shell's explicit-language form) like t!", () => {
		const src = 'fn f(tag: &str) { t_in!(tag, "cli_lang_current", lang = "en"); t!("cli_lang_current"); }\n';
		expect(rewriteUses(src, "crates/atlas-desktop/src/x.rs", "cli_lang_current", "app_lang_now")).toEqual({
			text: 'fn f(tag: &str) { t_in!(tag, "app_lang_now", lang = "en"); t!("app_lang_now"); }\n',
			lines: [1],
		});
	});

	test("mentions it cannot prove are reported, not edited", async () => {
		const root = await makeRepo();
		put(root, "crates/atlas-cli/src/x.rs", '// see cli_lang_current\nconst K: &str = "cli_lang_current";\nfn f() { t!("cli_lang_current"); }\n');
		put(root, "apps/desktop/src/note.ts", "// i18n-keys: cli_lang_current\n");
		const r = moveKey(loadCtx(root), "cli_lang_current", "cli_lang_now");
		expect(r.updated).toContainEqual({ file: "crates/atlas-cli/src/x.rs", line: 3 });
		expect(r.unproven).toEqual([
			{ file: "apps/desktop/src/note.ts", line: 1 },
			{ file: "crates/atlas-cli/src/x.rs", line: 1 },
			{ file: "crates/atlas-cli/src/x.rs", line: 2 },
		]);
		expect(read(root, "crates/atlas-cli/src/x.rs")).toContain('const K: &str = "cli_lang_current";');
	});

	test("{@old} references inside other messages follow, in every locale", async () => {
		const root = await makeRepo();
		addKey(root, "app_gone", "Moved to {@common_os_trash}", "Movido para {@common_os_trash}", "Toast");
		const r = moveKey(loadCtx(root), "common_os_trash", "common_os_bin");
		expect(JSON.parse(read(root, EN)).app_gone).toBe("Moved to {@common_os_bin}");
		expect(JSON.parse(read(root, PT)).app_gone).toBe("Movido para {@common_os_bin}");
		expect(r.updated.map((u) => u.file)).toEqual(expect.arrayContaining([`${EN}#app_gone`, `${PT}#app_gone`]));
		expect(errors(await check(root))).toEqual([]);
	});

	test("m[\"old\"] and m.old are both rewritten, only in files that import the messages", async () => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/a.ts", 'import * as m from "$lib/paraglide/messages";\nm["app_welcome"]({ name: "x" });\nm.app_welcome({ name: "y" });\n');
		put(root, "apps/desktop/src/b.ts", "const m = new Map();\nm.app_welcome;\n");
		moveKey(loadCtx(root), "app_welcome", "app_greeting");
		expect(read(root, "apps/desktop/src/a.ts")).toContain('m["app_greeting"]({ name: "x" });\nm.app_greeting({ name: "y" });');
		expect(read(root, "apps/desktop/src/b.ts")).toContain("m.app_welcome;");
	});

	// Regression: the desktop imports `m` from `#lib/i18n/m.ts`, not from Paraglide, so i18n:mv renamed the catalogs and left every
	// `m.old()` of apps/desktop dangling (undefined at runtime, a type error at best).
	test("rewrites every way the desktop and the site reach a message: wrapper imports, aliases, namespaces, optional chaining, templates, destructuring", async () => {
		const root = await makeRepo();
		const src = "apps/desktop/src";
		put(root, `${src}/Wrapper.svelte`, '<script lang="ts">\n  import { m } from "#lib/i18n/m.ts";\n</script>\n<p>{m.app_welcome({ name })}</p>\n<p>{@html m["app_welcome"]({ name })}</p>\n<p>{m?.app_welcome({ name })}</p>\n');
		put(root, `${src}/lib/viaLib.ts`, 'import { m } from "$lib/i18n/m.ts";\nexport const a = () => m.app_welcome({ name: "x" });\n');
		put(root, `${src}/lib/i18n/relative.ts`, 'import { m } from "./m.ts";\nexport const a = () => m.app_welcome({ name: "x" });\n');
		put(root, `${src}/lib/aliased.ts`, 'import { m as msg } from "#lib/i18n/m.ts";\nexport const a = () => msg.app_welcome({ name: "x" }) + msg["app_welcome"]({ name: "y" });\n');
		put(root, `${src}/lib/compiled.ts`, 'import * as compiled from "#lib/paraglide/messages.js";\nexport const a = compiled.app_welcome({ name: "x" });\n');
		put(root, `${src}/lib/multiline.ts`, 'import {\n  m,\n} from "#lib/i18n/m.ts";\nexport const a = m.app_welcome({ name: "x" });\n');
		put(root, `${src}/lib/destructured.ts`, 'import { m } from "#lib/i18n/m.ts";\nconst { app_welcome: hello, app_save_button: save } = m;\nexport const a = hello({ name: "x" }) + save();\n');
		put(root, `${src}/lib/namedAlias.ts`, 'import { app_welcome as hello } from "#lib/paraglide/messages.js";\nexport const a = hello({ name: "x" });\n');
		put(root, `${src}/lib/other.ts`, 'import { m } from "#lib/i18n/m.ts";\nexport const a = m.app_save_button() + m.app_welcome_back();\n');
		put(root, `${src}/lib/shadow.ts`, "const m = new Map();\nm.app_welcome;\n");
		const r = moveKey(loadCtx(root), "app_welcome", "app_greeting");
		const text = (f: string) => read(root, `${src}/${f}`);
		expect(text("Wrapper.svelte")).toContain('<p>{m.app_greeting({ name })}</p>\n<p>{@html m["app_greeting"]({ name })}</p>\n<p>{m?.app_greeting({ name })}</p>');
		expect(text("lib/viaLib.ts")).toContain("m.app_greeting({");
		expect(text("lib/i18n/relative.ts")).toContain("m.app_greeting({");
		expect(text("lib/aliased.ts")).toContain('msg.app_greeting({ name: "x" }) + msg["app_greeting"]({ name: "y" })');
		expect(text("lib/compiled.ts")).toContain("compiled.app_greeting({");
		expect(text("lib/multiline.ts")).toContain("m.app_greeting({");
		expect(text("lib/destructured.ts")).toContain("const { app_greeting: hello, app_save_button: save } = m;");
		expect(text("lib/namedAlias.ts")).toContain('import { app_greeting as hello } from "#lib/paraglide/messages.js";');
		// another key that merely starts with the old name, and a local `m` that is no message module, stay as they are
		expect(text("lib/other.ts")).toContain("m.app_welcome_back()");
		expect(text("lib/shadow.ts")).toContain("m.app_welcome;");
		expect(r.updated.filter((u) => u.file.startsWith(`${src}/Wrapper`))).toEqual([
			{ file: `${src}/Wrapper.svelte`, line: 4 },
			{ file: `${src}/Wrapper.svelte`, line: 5 },
			{ file: `${src}/Wrapper.svelte`, line: 6 },
		]);
		expect(r.dynamic).toEqual([]);
		expect(r.unproven).toEqual([{ file: `${src}/lib/shadow.ts`, line: 2 }]);
	});

	test("Rust: t!, t_in! and the lookup / translate functions, on one line or split across lines", async () => {
		const root = await makeRepo();
		put(
			root,
			"crates/atlas-cli/src/calls.rs",
			'fn f(tag: &str) {\n    t!(\n        "cli_lang_current",\n        lang = "en"\n    );\n    atlas_i18n::t_in!(tag, "cli_lang_current");\n    atlas_i18n::lookup(lang, "cli_lang_current", None);\n    atlas_i18n::lookup_on(tag, "cli_lang_current", None, "mac");\n    atlas_i18n::translate(tag, "cli_lang_current", None);\n}\n',
		);
		const r = moveKey(loadCtx(root), "cli_lang_current", "app_lang_now");
		expect(read(root, "crates/atlas-cli/src/calls.rs")).toBe(
			'fn f(tag: &str) {\n    t!(\n        "app_lang_now",\n        lang = "en"\n    );\n    atlas_i18n::t_in!(tag, "app_lang_now");\n    atlas_i18n::lookup(lang, "app_lang_now", None);\n    atlas_i18n::lookup_on(tag, "app_lang_now", None, "mac");\n    atlas_i18n::translate(tag, "app_lang_now", None);\n}\n',
		);
		expect(r.dynamic).toEqual([]);
		expect(r.unproven).toEqual([]);
	});

	test("a reference it cannot rewrite is listed with file:line and why: computed access, shorthand or rest destructuring, a bare named import, an i18n-keys glob, a built Rust key", async () => {
		const root = await makeRepo();
		const src = "apps/desktop/src";
		put(root, `${src}/lib/computed.ts`, 'import { m } from "#lib/i18n/m.ts";\nexport const a = (k: "app_welcome") => m[k]({ name: "x" });\n');
		put(root, `${src}/lib/template.ts`, 'import { m } from "#lib/i18n/m.ts";\nexport const a = (s: string) => m[`app_${s}`]();\nexport const b = (s: string) => m[`other_${s}`]();\nexport const c = (s: string) => m["app_" + s]();\n');
		put(root, `${src}/lib/walk.test.ts`, 'import { m } from "#lib/i18n/m.ts";\nfor (const k of Object.keys(m)) m[k]();\n');
		put(root, `${src}/lib/rest.ts`, 'import { m } from "#lib/i18n/m.ts";\nconst { app_save_button: save, ...others } = m;\n');
		// a local `m` inside a file that also imports the messages: its m[1] is no message access
		put(root, `${src}/lib/shadowed.ts`, 'import { m } from "#lib/i18n/m.ts";\nexport const a = (xs: string[][]) => xs.map((m) => m[1]);\nexport const b = m.app_save_button();\n');
		put(root, `${src}/lib/shorthand.ts`, 'import { m } from "#lib/i18n/m.ts";\nconst { app_welcome } = m;\n');
		put(root, `${src}/lib/named.ts`, 'import { app_welcome } from "#lib/paraglide/messages.js";\n');
		put(root, `${src}/lib/glob.ts`, "// i18n-keys: app_wel*\n");
		const r2 = moveKey(loadCtx(root), "app_welcome", "app_greeting");
		expect(r2.dynamic.map((d) => `${d.file}:${d.line}`)).toEqual([
			`${src}/lib/computed.ts:2`,
			`${src}/lib/glob.ts:1`,
			`${src}/lib/named.ts:1`,
			`${src}/lib/rest.ts:2`,
			`${src}/lib/shorthand.ts:2`,
			`${src}/lib/template.ts:2`,
			`${src}/lib/template.ts:4`,
		]);
		expect(r2.dynamic.every((d) => d.reason.length > 10)).toBe(true);
		// the static prefix `other_` cannot produce app_welcome: not reported; and nothing was rewritten in these files
		expect(read(root, `${src}/lib/shorthand.ts`)).toContain("const { app_welcome } = m;");
		// a Rust key built with format! next to a lookup; another key family is not touched by it
		const rust = await makeRepo();
		put(rust, "crates/atlas-cli/src/built.rs", 'fn f(tag: &str, what: &str) -> Option<String> {\n    atlas_i18n::lookup(tag, &format!("cli_lang_{what}"), None)\n}\n');
		expect(moveKey(loadCtx(rust), "cli_lang_current", "cli_lang_now").dynamic).toEqual([expect.objectContaining({ file: "crates/atlas-cli/src/built.rs", line: 2 })]);
		expect(moveKey(loadCtx(rust), "cli_files_count", "app_files_total").dynamic).toEqual([]);
	});

	test("the CLI exits 1 and prints file:line for every leftover (after moving what it could), 0 when nothing is left", async () => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/lib/computed.ts", 'import { m } from "#lib/i18n/m.ts";\nexport const a = (k: string) => m[k]();\nexport const b = m.app_welcome({ name: "x" });\n');
		put(root, "crates/atlas-cli/src/x.rs", 'const K: &str = "app_welcome";\n');
		const bad = await cli("mv.ts", root, ["app_welcome", "app_greeting"]);
		expect(bad.code).toBe(1);
		expect(bad.out).toContain("moved app_welcome (app) -> app_greeting (app)");
		expect(bad.out).toContain("updated apps/desktop/src/lib/computed.ts:3");
		expect(bad.err).toContain("apps/desktop/src/lib/computed.ts:2");
		expect(bad.err).toContain("crates/atlas-cli/src/x.rs:1");
		expect(read(root, "apps/desktop/src/lib/computed.ts")).toContain("m.app_greeting({");
		const json = await cli("mv.ts", root, ["app_greeting", "app_welcome", "--json"]);
		expect(json.code).toBe(1);
		expect(JSON.parse(json.out).dynamic).toEqual([expect.objectContaining({ file: "apps/desktop/src/lib/computed.ts", line: 2 })]);
		expect(JSON.parse(json.err).error).toContain("not rewritten");
		const clean = await makeRepo();
		expect((await cli("mv.ts", clean, ["app_welcome", "app_greeting"])).code).toBe(0);
	});

	test("--new-area creates the folder; an emptied area is reported", async () => {
		const root = await makeRepo();
		const r = moveKey(loadCtx(root), "app_welcome", "greeting_welcome", { newArea: true, surface: "desktop" });
		expect(r.toArea).toBe("greeting");
		expect(JSON.parse(read(root, "i18n/messages/greeting/en.json"))).toEqual({ greeting_welcome: "Welcome to Atlas, {name}" });
		expect(loadSurfacesConfig(root).config.surfaces.desktop.areas).toEqual(["app", "greeting"]);
		const emptied = moveKey(loadCtx(root), "cli_lang_current", "cli_lang_now");
		expect(emptied.notes).toEqual([]);
		moveKey(loadCtx(root), "cli_files_count", "app_files_total");
		const last = moveKey(loadCtx(root), "cli_lang_now", "app_lang_now");
		expect(last.notes.join(" ")).toContain("area cli is empty now");
	});

	test.each([
		["an unknown key", ["app_nope", "app_new"], "no message \"app_nope\": find it with bun run i18n:find app_nope"],
		["a key that is not snake_case", ["app_welcome", "Welcome"], "is not a key"],
		["a new key that exists", ["app_welcome", "app_open_docs"], "already exists in i18n/messages/app/en.json"],
		["a new key with no area", ["app_welcome", "zzz_welcome"], "no area for"],
	])("refuses %s and writes nothing", async (_n, [from, to], message) => {
		const root = await makeRepo();
		const before = snapshot(root);
		expect(() => moveKey(loadCtx(root), from, to)).toThrow(message);
		expect(snapshot(root)).toBe(before);
	});

	test("the CLI: --json reports updated and unproven, a refusal exits 1 with the fix", async () => {
		const root = await makeRepo();
		const ok = await cli("mv.ts", root, ["app_welcome", "app_greeting", "--json"]);
		expect(ok.code).toBe(0);
		expect(JSON.parse(ok.out)).toMatchObject({ from: "app_welcome", to: "app_greeting", fromArea: "app", toArea: "app", unproven: [] });
		const bad = await cli("mv.ts", root, ["app_missing", "app_x", "--json"]);
		expect(bad.code).toBe(1);
		expect(JSON.parse(bad.err).error).toContain("bun run i18n:find app_missing");
		const human = await cli("mv.ts", root, ["app_greeting", "app_welcome"]);
		expect(human.out).toContain("moved app_greeting (app) -> app_welcome (app)");
	});
});
