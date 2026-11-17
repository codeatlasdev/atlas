// The hardcoded-text ratchet: what the scanners flag, and the baseline that may only shrink.
import { afterAll, describe, expect, test } from "bun:test";
import { existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import { readBaseline } from "./baseline.ts";
import { fix } from "./fix.ts";
import { scanRustSource } from "./hardcoded/rust.ts";
import { scanSvelteSource } from "./hardcoded/svelte.ts";
import { type Finding, loadCtx } from "./types.ts";
import { check, cleanup, finding, makeRepo, put, read, rulesOf } from "./testkit.ts";

afterAll(cleanup);

describe("Svelte scanner (svelte/compiler AST)", () => {
	test.each([
		["text node", "<p>Hello there</p>", 1],
		["static title attribute", '<button title="Save the file">{x}</button>', 1],
		["each listed attribute", '<Foo placeholder="a" aria-label="b" alt="c" label="d" tooltip="e" description="f" message="g" aria-description="h" title="i" />', 9],
		["text inside blocks", "{#if a}<b>Yes</b>{:else}<b>No</b>{/if}{#each xs as x}<i>Item</i>{/each}", 3],
		["text and attribute together", '<h1 title="Hi">Hello</h1>', 2],
	])("flags %s", (_name, src, n) => {
		expect(scanSvelteSource(src)).toHaveLength(n);
	});

	test.each([
		["an expression", "<p>{m.app_save_button()}</p>", 0],
		["a dynamic attribute", "<button title={m.x()} aria-label={`a ${b}`}>{x}</button>", 0],
		["text without letters", "<p>42 · 7 → {x} ×</p>", 0],
		["code, pre, kbd", "<code>atlas dev</code><pre>run it</pre><kbd>Cmd K</kbd>", 0],
		["script and style", "<script>const a = 'Hello world';</script><style>.a::after { content: 'Hello'; }</style>", 0],
		["the next sibling after an i18n-ignore comment", "<!-- i18n-ignore -->\n<p>Brand name</p>\n<p>Not ignored</p>", 1],
		["an unlisted attribute", '<a href="/docs" class="big" data-x="Hello" />', 0],
	])("does not flag %s", (_name, src, n) => {
		expect(scanSvelteSource(src)).toHaveLength(n);
	});

	test("reports the line of the violation", () => {
		expect(scanSvelteSource("<div>\n  <p>ok {x}</p>\n  <p>Hello</p>\n</div>").map((v) => v.line)).toEqual([2, 3]);
	});
});

describe("Rust scanner (format strings of println!/eprintln!/print!/eprint!/bail!/anyhow!)", () => {
	test.each([
		["println with text", 'println!("Hello world");', 1],
		["eprintln with a placeholder and text", 'eprintln!("error: {e}");', 1],
		["bail", 'bail!("no such file {}", p);', 1],
		["anyhow", 'let e = anyhow!("could not open {path}");', 1],
		["a raw string", 'println!(r#"Say "hi""#);', 1],
		["a multi-line literal", 'println!(\n    "Hello \\\n     world"\n);', 1],
		["a format spec next to text", 'println!("a: {x:>5}");', 1],
		["a code after a colorized prefix", 'println!("\\x1b[32m✓\\x1b[0m done");', 1],
	])("flags %s", (_n, src, count) => {
		expect(scanRustSource(src)).toHaveLength(count);
	});

	test.each([
		["machine output with no letters", 'println!("{}", json);', 0],
		["escaped braces only", 'println!("{{}}");', 0],
		["an expression instead of a literal", "bail!(err);\nanyhow!(e);\nprintln!();", 0],
		["a step line (parsed by the desktop)", 'println!("step: {key}");', 0],
		["only escape codes", 'print!("\\x1b[0m\\n");', 0],
		["a commented-out call", '// println!("Hello");', 0],
		["i18n-ignore on the line", 'println!("Hello"); // i18n-ignore', 0],
		["i18n-ignore on the line above", '// i18n-ignore: debug output\nprintln!("Hello");', 0],
		["other macros", 'let s = format!("Hello"); write!(f, "Hello")?;', 0],
		["the cfg(test) module tail", '#[cfg(test)]\nmod tests {\n    fn t() { println!("hello"); }\n}', 0],
	])("does not flag %s", (_n, src, count) => {
		expect(scanRustSource(src)).toHaveLength(count);
	});

	test("code before the test module still counts", () => {
		expect(scanRustSource('fn a() { println!("Hello"); }\n#[cfg(test)]\nmod tests {\n    fn t() { println!("x y"); }\n}')).toHaveLength(1);
	});

	test("a single #[cfg(test)] item does not hide the rest of the file", () => {
		expect(scanRustSource('#[cfg(test)]\nuse foo::bar;\nfn a() { println!("Hello"); }')).toHaveLength(1);
	});
});

const SVELTE = "apps/desktop/src/New.svelte";
const baselineOf = (root: string) => readBaseline(root);
const hardcoded = (fs: Finding[]) => fs.filter((f) => f.rule === "hardcoded");

describe("the baseline only shrinks", () => {
	test("a new violation is an error at its line; fix never adds it to the baseline", async () => {
		const root = await makeRepo();
		put(root, SVELTE, "<div>\n<p>Hello there</p>\n</div>\n");
		const f = finding(await check(root), "hardcoded");
		expect(f).toMatchObject({ severity: "error", file: SVELTE, line: 2 });
		fix(loadCtx(root));
		expect(baselineOf(root).svelte?.[SVELTE]).toBeUndefined();
		expect(hardcoded(await check(root))).toHaveLength(1);
	});

	test("a file at its baseline is quiet; one more violation fails; one fewer demands a shrink", async () => {
		const root = await makeRepo();
		put(root, SVELTE, "<p>One</p>\n<p>Two</p>\n");
		put(root, "i18n/hardcoded-baseline.json", JSON.stringify({ svelte: { [SVELTE]: 2 } }));
		expect(await check(root)).toEqual(expect.not.arrayContaining([expect.objectContaining({ rule: "hardcoded" })]));

		put(root, SVELTE, "<p>One</p>\n<p>Two</p>\n<p>Three</p>\n");
		expect(hardcoded(await check(root))).toHaveLength(3);

		put(root, SVELTE, "<p>One</p>\n");
		const stale = finding(await check(root), "hardcoded-baseline");
		expect(stale).toMatchObject({ severity: "error", key: SVELTE });
		expect(stale?.message).toContain("i18n:fix");

		fix(loadCtx(root));
		expect(baselineOf(root).svelte?.[SVELTE]).toBe(1);
		expect(rulesOf(await check(root), "error")).toEqual([]);
	});

	test("fix never raises a baseline, and drops the entry of a file that is clean or gone", async () => {
		const root = await makeRepo();
		put(root, SVELTE, "<p>One</p>\n<p>Two</p>\n<p>Three</p>\n");
		put(root, "i18n/hardcoded-baseline.json", JSON.stringify({ svelte: { [SVELTE]: 1, "apps/desktop/src/Gone.svelte": 4 } }));
		fix(loadCtx(root));
		expect(baselineOf(root).svelte).toEqual({ [SVELTE]: 1 });
	});

	test("--baseline seeds a missing section from today's counts, once; later runs only shrink", async () => {
		const root = await makeRepo();
		put(root, SVELTE, "<p>One</p>\n<p>Two</p>\n");
		put(root, "crates/atlas-cli/src/main.rs", 'fn main() {\n    println!("Hello");\n}\n');
		rmSync(join(root, "i18n/hardcoded-baseline.json"));
		fix(loadCtx(root));
		expect(existsSync(join(root, "i18n/hardcoded-baseline.json"))).toBe(false);
		fix(loadCtx(root), { baseline: true });
		expect(baselineOf(root)).toEqual({ platform: {}, rust: { "crates/atlas-cli/src/main.rs": 1 }, shortcut: {}, svelte: { [SVELTE]: 2 }, writing: {} });
		expect(rulesOf(await check(root), "error")).toEqual([]);
		put(root, SVELTE, "<p>One</p>\n<p>Two</p>\n<p>Three</p>\n<p>Four</p>\n");
		fix(loadCtx(root), { baseline: true });
		expect(baselineOf(root).svelte?.[SVELTE]).toBe(2);
		expect(hardcoded(await check(root)).length).toBeGreaterThan(0);
	});

	test("the Rust ratchet works the same way, and skips tests", async () => {
		const root = await makeRepo();
		put(root, "crates/atlas-tui/src/lib.rs", 'pub fn a() { eprintln!("something failed"); }\n#[cfg(test)]\nmod tests {\n    fn t() { println!("test only"); }\n}\n');
		put(root, "crates/atlas-tui/src/tests.rs", 'fn t() { println!("also test only"); }\n');
		put(root, "crates/atlas-tui/src/tests/more.rs", 'fn t() { println!("also test only"); }\n');
		const f = hardcoded(await check(root));
		expect(f.map((x) => x.file)).toEqual(["crates/atlas-tui/src/lib.rs"]);
		expect(f[0].line).toBe(1);
		put(root, "i18n/hardcoded-baseline.json", JSON.stringify({ rust: { "crates/atlas-tui/src/lib.rs": 1 } }));
		expect(rulesOf(await check(root), "error")).toEqual([]);
		put(root, "crates/atlas-tui/src/lib.rs", "pub fn a() {}\n");
		expect(finding(await check(root), "hardcoded-baseline")?.key).toBe("crates/atlas-tui/src/lib.rs");
	});

	test("--surface keeps the other surface's ratchet out of the run", async () => {
		const root = await makeRepo();
		put(root, SVELTE, "<p>Hello</p>\n");
		expect(hardcoded(await check(root, "site"))).toEqual([]);
		expect(hardcoded(await check(root, "desktop"))).toHaveLength(1);
	});

	test("a file the scanner cannot parse is an error, never silently clean", async () => {
		const root = await makeRepo();
		put(root, SVELTE, "<div><p>unclosed\n");
		expect(finding(await check(root), "hardcoded")?.message).toContain("cannot parse");
	});
});
