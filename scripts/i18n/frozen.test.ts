// The frozen-message rule: a message function evaluated once (module scope, `<script module>`, a class field) keeps the
// language of that moment, so a live language switch leaves it behind. Incident: batch S10 found command labels,
// module-level lists and settings options frozen in the first language.
import { afterAll, describe, expect, test } from "bun:test";
import { scanFrozenSource } from "./hardcoded/frozen.ts";
import { check, cleanup, makeRepo, rulesOf } from "./testkit.ts";

afterAll(cleanup);

const M = 'import { m } from "#lib/i18n/m.ts";\n';
const lines = (file: string, src: string) => scanFrozenSource(file, src).map((v) => v.line);

describe("frozen shapes are errors", () => {
	test.each([
		["a module constant", "export const TITLE = m.shell_title();"],
		["a message with arguments", "export const SHOWN = m.files_count({ count: 2 });"],
		["a bracket access", 'export const TITLE = m["shell_title"]();'],
		["an optional call", "export const TITLE = m?.shell_title();"],
		["a rich message's parts", "export const PARTS = m.shell_title.parts();"],
		["an object literal of labels", "export const ITEMS = { open: { label: m.shell_open() } };"],
		["an array of objects of labels", "export const ITEMS = [{ id: 1 }, { label: m.shell_close() }];"],
		["a template literal", "export const TITLE = `Atlas: ${m.shell_title()}`;"],
		["a command registration run at startup", "register({ id: 'x', label: m.shell_open(), run: () => {} });"],
		["a concatenation", "const TITLE = 'a' + m.shell_title();"],
		["a store's initial value", "export const store = $state({ title: m.shell_title() });"],
		["a $state initial value", "let title = $state(m.shell_title());"],
		["a Map built at load time", "const NAMES = new Map([['a', m.shell_a()]]);"],
		["an immediately invoked function", "const T = (() => m.shell_title())();"],
		["an if block at module level", "if (FLAG) { log(m.shell_title()); }"],
		["a class field", "class Store { title = m.shell_title(); }"],
		["a static class field", "class Store { static TITLE = m.shell_title(); }"],
	])("%s", (_n, body) => {
		expect(lines("apps/desktop/src/lib/x.ts", M + body)).toEqual([2]);
	});

	test("a conditional reports both arms, a destructured message is a message call", () => {
		expect(lines("apps/desktop/src/lib/x.ts", `${M}const T = FLAG ? m.shell_a() : m.shell_b();`)).toEqual([2, 2]);
		expect(lines("apps/desktop/src/lib/x.ts", `${M}const { shell_title } = m;\nexport const T = shell_title();`)).toEqual([3]);
		expect(lines("apps/desktop/src/lib/x.ts", `${M}const { shell_title: title } = m;\nexport const T = title();`)).toEqual([3]);
	});

	test("a renamed namespace import and a message imported by name", () => {
		expect(lines("apps/desktop/src/lib/x.ts", 'import * as msg from "$lib/paraglide/messages.js";\nconst A = msg.shell_title();')).toEqual([2]);
		expect(lines("site/src/x.ts", 'import { m as t } from "./paraglide/messages.js";\nconst A = t.shell_title();')).toEqual([2]);
		expect(lines("site/src/x.ts", 'import { shell_title as title } from "./paraglide/messages.js";\nconst A = title();')).toEqual([2]);
	});

	test("a .svelte.ts store and a <script module> block", () => {
		expect(lines("apps/desktop/src/lib/x.svelte.ts", `${M}export const t = m.a();`)).toEqual([2]);
		expect(lines("apps/desktop/src/lib/X.svelte", `<script module lang="ts">\n${M}export const ITEMS = [{ label: m.a() }];\n</script>\n<p>x</p>`)).toEqual([3]);
		expect(lines("apps/desktop/src/lib/X.svelte", `<script context="module">\n${M}export const T = m.a();\n</script>`)).toEqual([3]);
	});

	test("every frozen call of a file is reported with its line", () => {
		const src = `${M}export const A = m.a();\n\nexport const B = [\n  m.b(),\n];\nexport const F = () => m.c();\n`;
		expect(lines("apps/desktop/src/lib/x.ts", src)).toEqual([2, 5]);
	});

	test("the root layout's own script freezes a value for the whole session", () => {
		const src = `<script lang="ts">\n${M}const title = m.a();\n</script>\n<p>{title}</p>`;
		expect(lines("apps/desktop/src/routes/+layout.svelte", src)).toEqual([3]);
	});
});

describe("a command registered once (the palette reads `label` on every listing)", () => {
	const KEYMAP = 'import { register, type Command } from "#lib/keymap.svelte.ts";\n';
	const cmd = (label: string) => `register([{ id: "atlas::A", label: ${label}, run: () => {} }]);`;

	test("a label computed when the component mounts is frozen", () => {
		const src = `<script lang="ts">\n${M}${KEYMAP}import { onMount } from "svelte";\nonMount(() => ${cmd("m.shell_open()").replace(/;$/, "")});\n</script>`;
		expect(lines("apps/desktop/src/lib/components/Sidebar.svelte", src)).toEqual([5]);
	});

	test("register inside a function of a module is frozen too, and a renamed import is followed", () => {
		const src = `${M}import { register as add } from "#lib/keymap.svelte.ts";\nexport function start() {\n  add([{ id: "a", label: m.shell_open(), run: () => {} }]);\n}`;
		expect(lines("apps/desktop/src/lib/shell/x.ts", src)).toEqual([4]);
	});

	test("a function returning Command[] runs once, to register", () => {
		const src = `${M}${KEYMAP}export function shellCommands(): Command[] {\n  return [{ id: "a", label: m.shell_open(), run: () => {} }];\n}`;
		expect(lines("apps/desktop/src/lib/shell/x.ts", src)).toEqual([4]);
	});

	test("a getter or a thunk read at listing time is fine", () => {
		const getter = `${M}${KEYMAP}export function cmds(): Command[] {\n  return [{ id: "a", get label() { return m.shell_open(); }, run: () => {} }];\n}`;
		const thunk = `${M}${KEYMAP}function f() { ${cmd("(() => m.shell_open())")} }`;
		expect(lines("apps/desktop/src/lib/shell/x.ts", getter)).toEqual([]);
		expect(lines("apps/desktop/src/lib/shell/x.ts", thunk)).toEqual([]);
	});
});

describe("reactive shapes are fine", () => {
	test.each([
		["inside a function", "export function title() { return m.shell_title(); }"],
		["inside an arrow function", "export const title = () => m.shell_title();"],
		["a method", "export const o = { title() { return m.shell_title(); } };"],
		["a getter on an object literal", "export const o = { get label() { return m.shell_title(); } };"],
		["a getter on a class", "class S { get title() { return m.shell_title(); } }"],
		["a class method", "class S { title() { return m.shell_title(); } }"],
		// Imperative widgets (a CodeMirror panel) are built when opened; their constructor reads the language of that moment.
		["a constructor", "class Panel { dom: string; constructor() { this.dom = m.shell_title(); } }"],
		["palette items built when it opens", "export function items() { return [{ id: 'a', label: m.shell_open(), run: () => {} }]; }"],
		["a register that is not the keymap's", "function register(x: unknown) {}\nfunction f() { register([{ id: 'a', label: m.shell_open() }]); }"],
		["a label thunk", "export const ITEMS = [{ label: () => m.shell_open() }];"],
		["a message function kept uncalled", "export const ITEMS = [{ label: m.shell_open }];"],
		["a $derived", "let title = $derived(m.shell_title());"],
		["a $derived on a class field", "class S { title = $derived(m.shell_title()); }"],
		["a $derived.by", "let title = $derived.by(() => m.shell_title());"],
		["an $effect", "$effect(() => { document.title = m.shell_title(); });"],
		["a function default parameter", "export function f(t = m.shell_title()) { return t; }"],
		["a local variable shadowing m", "export const A = ((m) => m.title)({ title: 'x' });\nfunction g(m: { t(): string }) { return m.t(); }"],
		["a file with no messages binding", "const m = { title: () => 'x' };\nexport const A = m.title();"],
	])("%s", (_n, body) => {
		const src = /no messages binding/.test(_n) ? body : M + body;
		expect(lines("apps/desktop/src/lib/x.ts", src)).toEqual([]);
	});

	test("an instance script of an ordinary component runs once per mount", () => {
		const src = `<script lang="ts">\n${M}const title = m.a();\n</script>\n<p>{title} {m.b()}</p>`;
		expect(lines("apps/desktop/src/lib/components/Row.svelte", src)).toEqual([]);
	});

	test("a layout's template and $derived are reactive", () => {
		const src = `<script lang="ts">\n${M}const title = $derived(m.a());\nfunction open() { return m.c(); }\n</script>\n<p>{title} {m.b()}</p>`;
		expect(lines("apps/desktop/src/routes/+layout.svelte", src)).toEqual([]);
	});

	test("tests, fixtures and generated message code are not looked at", () => {
		const src = `${M}export const A = m.a();`;
		expect(scanFrozenSource("apps/desktop/src/lib/x.test.ts", src)).toEqual([]);
		expect(scanFrozenSource("apps/desktop/src/lib/i18n/messages.gen.ts", 'import { m } from "./m.ts";\nconst A = m.a();')).toEqual([]);
	});
});

describe("the waiver", () => {
	test("`// i18n-allow frozen: <why>` on the line or the line above waives that call", () => {
		const above = `${M}// i18n-allow frozen: English-only text for the machine log\nexport const A = m.a();`;
		const same = `${M}export const A = m.a(); // i18n-allow frozen: English-only text for the machine log`;
		expect(lines("apps/desktop/src/lib/x.ts", above)).toEqual([]);
		expect(lines("apps/desktop/src/lib/x.ts", same)).toEqual([]);
	});

	test("a waiver covers its own line only", () => {
		const src = `${M}// i18n-allow frozen: first one is fine\nexport const A = m.a();\nexport const B = m.b();`;
		expect(lines("apps/desktop/src/lib/x.ts", src)).toEqual([4]);
	});

	test("a waiver with no reason does not waive", () => {
		const src = `${M}// i18n-allow frozen:\nexport const A = m.a();`;
		const found = scanFrozenSource("apps/desktop/src/lib/x.ts", src);
		expect(found.map((v) => v.line)).toEqual([3]);
	});

	test("a waiver works in a <script module> block", () => {
		const src = `<script module lang="ts">\n${M}// i18n-allow frozen: machine text\nexport const A = m.a();\n</script>`;
		expect(lines("apps/desktop/src/lib/X.svelte", src)).toEqual([]);
	});
});

describe("the check", () => {
	test("a frozen label in a desktop module is a `frozen` error naming the line and the fix", async () => {
		const root = await makeRepo({ "apps/desktop/src/lib/items.ts": `${M}export const ITEMS = [{ label: m.app_save_button() }];\n` });
		const found = await check(root);
		const f = found.filter((x) => x.rule === "frozen");
		expect(f).toHaveLength(1);
		expect(f[0]).toMatchObject({ severity: "error", file: "apps/desktop/src/lib/items.ts", line: 2 });
		expect(f[0].message).toContain("() => m.");
	});

	test("the same label behind a getter passes", async () => {
		const root = await makeRepo({ "apps/desktop/src/lib/items.ts": `${M}export const ITEMS = [{ get label() { return m.app_save_button(); } }];\n` });
		expect(rulesOf(await check(root))).not.toContain("frozen");
	});

	test("the site is scanned as well, and --surface limits it", async () => {
		const root = await makeRepo({ "site/src/lib/x.ts": 'import { m } from "../paraglide/messages.js";\nexport const A = m.app_save_button();\n' });
		expect((await check(root)).filter((x) => x.rule === "frozen").map((x) => x.file)).toEqual(["site/src/lib/x.ts"]);
		expect((await check(root, "desktop")).filter((x) => x.rule === "frozen")).toEqual([]);
	});

	test("a file that cannot be parsed is an error, never a clean pass", async () => {
		const root = await makeRepo({ "apps/desktop/src/lib/broken.ts": `${M}export const A = ;\n` });
		const f = (await check(root)).filter((x) => x.rule === "frozen");
		expect(f).toHaveLength(1);
		expect(f[0].message).toContain("cannot parse");
	});
});
