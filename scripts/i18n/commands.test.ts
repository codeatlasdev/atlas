// i18n:find, i18n:add, i18n:mv: what they change, what they refuse (writing nothing), and that the result passes i18n:check.
import { afterAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { addMessage } from "./add.ts";
import { find } from "./find.ts";
import { moveKey, rewriteUses } from "./mv.ts";
import { loadCtx } from "./types.ts";
import { CLI_EN, CLI_META, EN, META, PT, addKey, area, check, cleanup, editJson, errors, finding, makeRepo, put, read, rulesOf } from "./testkit.ts";

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

	test("an unknown area needs --new-area and a surface; with both it creates the folder and lists it in surfaces.json", async () => {
		const root = await makeRepo();
		const before = snapshot(root);
		expect(() => addMessage(loadCtx(root), { key: "billing_plan_title", english: "Plan", description: "Title of the plan card" })).toThrow("create billing with --new-area --surface <desktop|rust>");
		expect(() => addMessage(loadCtx(root), { key: "billing_plan_title", english: "Plan", description: "Title", newArea: true })).toThrow("--new-area needs --surface");
		expect(snapshot(root)).toBe(before);
		const r = addMessage(loadCtx(root), { key: "billing_plan_title", english: "Plan", pt: "Plano", description: "Title of the plan card", newArea: true, surface: "desktop" });
		expect(r).toMatchObject({ area: "billing", createdArea: true });
		expect(JSON.parse(read(root, "i18n/messages/billing/en.json"))).toEqual({ billing_plan_title: "Plan" });
		expect(JSON.parse(read(root, "i18n/surfaces.json")).surfaces.desktop.areas).toEqual(["app", "billing"]);
		expect(read(root, "i18n/surfaces.json")).toContain('"desktop": { "areas": ["app", "billing"], "paraglide": "apps/desktop/.i18n" }');
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

	test("--new-area creates the folder; an emptied area is reported", async () => {
		const root = await makeRepo();
		const r = moveKey(loadCtx(root), "app_welcome", "greeting_welcome", { newArea: true, surface: "desktop" });
		expect(r.toArea).toBe("greeting");
		expect(JSON.parse(read(root, "i18n/messages/greeting/en.json"))).toEqual({ greeting_welcome: "Welcome to Atlas, {name}" });
		expect(JSON.parse(read(root, "i18n/surfaces.json")).surfaces.desktop.areas).toEqual(["app", "greeting"]);
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
