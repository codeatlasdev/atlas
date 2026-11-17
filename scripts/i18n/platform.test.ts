// Platform wording: shortcuts are placeholders, OS nouns need an `os` select (or a platform flag), and the
// platform is sniffed in one place only.
import { afterAll, describe, expect, test } from "bun:test";
import { readBaseline } from "./baseline.ts";
import { fix } from "./fix.ts";
import { loadCtx } from "./types.ts";
import { EN, META, OS, PT, addKey, check, cleanup, editJson, errors, finding, makeRepo, put, read, rulesOf } from "./testkit.ts";

afterAll(cleanup);

const OS_EN = OS("Reveal in Finder", "Reveal in File Explorer", "Reveal in the file manager");
const OS_PT = OS("Mostrar no Finder", "Mostrar no Explorador de Arquivos", "Mostrar no gerenciador de arquivos");
const COMMON_OS = "i18n/messages/common_os";

describe("shortcuts in catalog values", () => {
	test.each([["⌘K"], ["⌃C"], ["⌥⇧P"], ["Cmd+K"], ["Ctrl+S"], ["Shift + Tab"], ["Alt+Enter"], ["Option+Click"]])("%s is an error", async (chord) => {
		const root = await makeRepo();
		addKey(root, "app_probe", `Press ${chord} to open`, `Pressione ${chord} para abrir`, "A probe");
		const f = (await check(root)).filter((x) => x.rule === "shortcut" && x.key === "app_probe");
		expect(f.map((x) => x.file).sort()).toEqual([EN, PT]);
		expect(f[0].severity).toBe("error");
	});

	test("a {key} placeholder passes; words like Shift or Control without a plus do too", async () => {
		const root = await makeRepo();
		addKey(root, "app_probe", "Press {key} to open", "Pressione {key} para abrir", "A probe");
		addKey(root, "app_probe_two", "Hold Shift to select", "Segure Shift para selecionar", "A probe");
		expect(rulesOf(await check(root))).not.toContain("shortcut");
	});

	test("a shortcut inside a code span or URL is not flagged", async () => {
		const root = await makeRepo();
		addKey(root, "app_probe", "Run `atlas Ctrl+C` or see https://x.example/Cmd+K", "Execute `atlas Ctrl+C` ou veja https://x.example/Cmd+K", "A probe");
		expect(rulesOf(await check(root))).not.toContain("shortcut");
	});
});

describe("OS nouns in catalog values", () => {
	test.each([["Mac"], ["macOS"], ["Finder"], ["Windows"], ["Explorer"]])("%s without an os select is an error", async (noun) => {
		const root = await makeRepo();
		addKey(root, "app_probe", `Open in ${noun}`, `Abrir no ${noun}`, "A probe");
		const f = finding(await check(root), "platform-noun", "app_probe");
		expect(f?.severity).toBe("error");
		expect(f?.message).toContain(noun);
	});

	test("a message that selects on os may name every platform's noun, and its pt-BR must select too", async () => {
		const root = await makeRepo();
		addKey(root, "app_reveal", OS_EN, OS_PT, "Menu item that reveals a file in the OS file manager");
		expect(errors(await check(root))).toEqual([]);
		editJson(root, PT, (o) => void (o.app_reveal = "Mostrar no Finder"));
		fix(loadCtx(root));
		const rules = rulesOf(errors(await check(root)));
		expect(rules).toContain("platform-noun");
		expect(rules).toContain("plural");
	});

	test("the os selector may be an alias declared from input os", async () => {
		const root = await makeRepo();
		const aliased = (m: Record<string, string>) => [{ declarations: ["input os", "local platform = os: string"], selectors: ["platform"], match: m }];
		addKey(root, "app_reveal", aliased({ "platform=mac": "Finder", "platform=*": "Files" }), aliased({ "platform=mac": "Finder", "platform=*": "Arquivos" }), "Reveal");
		expect(rulesOf(await check(root))).not.toContain("platform-noun");
	});

	test("a select on something else does not license a noun", async () => {
		const root = await makeRepo();
		const other = (a: string, b: string) => [{ declarations: ["input kind"], selectors: ["kind"], match: { "kind=a": a, "kind=*": b } }];
		addKey(root, "app_probe", other("Open in Finder", "Open"), other("Abrir no Finder", "Abrir"), "A probe");
		expect(rulesOf(await check(root))).toContain("platform-noun");
	});

	test("meta `platform` licenses the nouns of exactly one platform", async () => {
		const root = await makeRepo();
		addKey(root, "app_probe", "Open in Finder", "Abrir no Finder", "A probe");
		editJson(root, META, (o) => void (o.app_probe = { description: "A probe", platform: "mac" }));
		expect(rulesOf(await check(root))).not.toContain("platform-noun");
		editJson(root, EN, (o) => void (o.app_probe = "Open in Finder or Explorer"));
		editJson(root, PT, (o) => void (o.app_probe = "Abrir no Finder ou Explorer"));
		fix(loadCtx(root), { restamp: ["app_probe"] });
		const f = (await check(root)).filter((x) => x.rule === "platform-noun");
		expect(f.length).toBeGreaterThan(0);
		expect(f[0].message).toContain("flagged mac");
	});

	test("meta problems: an unknown platform, an unknown field, a description of a key that is gone", async () => {
		const root = await makeRepo();
		editJson(root, META, (o) => void (o.app_save_button = { description: "x", platform: "amiga" }));
		const says = async (text: string) => (await check(root)).some((f) => f.rule === "description" && f.key === "app_save_button" && f.message.includes(text));
		expect(await says("platform must be one of")).toBe(true);
		editJson(root, META, (o) => void (o.app_save_button = { description: "x", colour: "red" }));
		expect(await says("unknown meta field")).toBe(true);
		editJson(root, META, (o) => void (o.app_gone = "Nothing"));
		expect((await check(root)).some((f) => f.rule === "description" && f.key === "app_gone")).toBe(true);
	});

	test("lowercase windows (the plural of window) and a noun in a code span are not platform nouns", async () => {
		const root = await makeRepo();
		addKey(root, "app_probe", "Close all windows or run `open Finder`", "Feche todas as janelas ou execute `open Finder`", "A probe");
		expect(rulesOf(await check(root))).not.toContain("platform-noun");
	});

	test("a reference to a common_os term composes: the message becomes an os select and may name no noun itself", async () => {
		const root = await makeRepo();
		editJson(root, `${COMMON_OS}/en.json`, (o) => void (o.common_os_file_manager = OS("Finder", "File Explorer", "the file manager")));
		editJson(root, `${COMMON_OS}/pt-BR.json`, (o) => void (o.common_os_file_manager = OS("o Finder", "o Explorador de Arquivos", "o gerenciador de arquivos")));
		editJson(root, `${COMMON_OS}/meta.json`, (o) => void (o.common_os_file_manager = "The OS app that shows files and folders"));
		addKey(root, "app_reveal", "Reveal in {@common_os_file_manager}", "Mostrar em {@common_os_file_manager}", "Menu item that reveals a file");
		expect(errors(await check(root))).toEqual([]);
		editJson(root, EN, (o) => void (o.app_reveal = "Reveal in Finder"));
		fix(loadCtx(root), { restamp: ["app_reveal"] });
		expect(rulesOf(errors(await check(root)))).toContain("platform-noun");
	});
});

const KEYS = "apps/desktop/src/lib/keys.ts";

describe("shortcuts and platform sniffing in source (ratchets)", () => {
	test.each([
		["Svelte text", "apps/desktop/src/A.svelte", "<p>Press ⌘K to search</p>\n"],
		["a Svelte attribute", "apps/desktop/src/A.svelte", '<button title="Toggle  Ctrl+B">x</button>\n'],
		["a TypeScript string", "apps/desktop/src/a.ts", 'export const hint = "Cmd+Shift+P";\n'],
		["a site island", "site/src/components/X.svelte", "<kbd>⌘K</kbd>\n"],
		["a Rust string", "crates/atlas-tui/src/lib.rs", 'pub fn hint() -> &\'static str { "Press Ctrl+C to quit" }\n'],
	])("%s with a shortcut is an error at its line", async (_n, file, text) => {
		const root = await makeRepo();
		put(root, file, text);
		const f = finding(await check(root), "shortcut");
		expect(f).toMatchObject({ severity: "error", file, line: 1 });
		expect(f?.message).toContain("kbd()");
	});

	test("keys.ts, tests, comments and i18n-ignore are exempt", async () => {
		const root = await makeRepo();
		put(root, KEYS, 'export const GLYPH = "⌘";\n');
		put(root, "apps/desktop/src/lib/keys.test.ts", 'expect(label("Cmd+K")).toBe("⌘K");\n');
		put(root, "apps/desktop/src/b.ts", '// press Ctrl+C here\n/* ⌘K */\n// i18n-ignore\nconst x = "Cmd+K";\nconst y = "Ctrl+Z"; // i18n-ignore\n');
		put(root, "crates/atlas-tui/src/lib.rs", '#[cfg(test)]\nmod tests {\n    fn t() { let _ = "Ctrl+C"; }\n}\n');
		expect(rulesOf(await check(root))).not.toContain("shortcut");
	});

	test("the baseline tolerates today's lines, fails a new one, and shrinks with fix", async () => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/a.ts", 'const a = "Cmd+K";\nconst b = "Cmd+J";\n');
		put(root, "i18n/hardcoded-baseline.json", JSON.stringify({ shortcut: { "apps/desktop/src/a.ts": 2 } }));
		expect(rulesOf(await check(root))).not.toContain("shortcut");
		put(root, "apps/desktop/src/a.ts", 'const a = "Cmd+K";\nconst b = "Cmd+J";\nconst c = "Cmd+L";\n');
		expect((await check(root)).filter((f) => f.rule === "shortcut")).toHaveLength(3);
		put(root, "apps/desktop/src/a.ts", 'const a = "Cmd+K";\n');
		expect(finding(await check(root), "shortcut-baseline")?.key).toBe("apps/desktop/src/a.ts");
		fix(loadCtx(root));
		expect(readBaseline(root).shortcut).toEqual({ "apps/desktop/src/a.ts": 1 });
		expect(rulesOf(await check(root))).not.toContain("shortcut-baseline");
	});

	test.each([
		["navigator.platform", 'const p = navigator.platform;\n'],
		["an isMac assignment", "const isMac = /Mac/.test(ua);\n"],
		["in markup", '<script>let isMac = false;</script>\n'],
	])("%s outside keys.ts is a platform-check error; inside keys.ts it is fine", async (_n, text) => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/lib/nav.ts", text);
		put(root, KEYS, text);
		const fs = (await check(root)).filter((f) => f.rule === "platform-check");
		expect(fs.map((f) => f.file)).toEqual(["apps/desktop/src/lib/nav.ts"]);
		expect(fs[0].message).toContain("keys.ts");
	});

	test("the real Atlas wording passes: IS_MAC and an os argument are not sniffing", async () => {
		const root = await makeRepo();
		put(root, "apps/desktop/src/lib/x.ts", 'import { IS_MAC } from "./keys";\nexport const key = IS_MAC ? "a" : "b";\nconst isMacOs = true;\n');
		expect(rulesOf(await check(root))).not.toContain("platform-check");
	});
});
