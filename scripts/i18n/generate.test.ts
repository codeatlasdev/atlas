// The message algebra (references, cross product, nested selects) and the generators built on it.
import { afterAll, describe, expect, test } from "bun:test";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { arbFile, arbMessage } from "./emit/arb.ts";
import { generateAll, generateSurface, outputsFor } from "./generate.ts";
import { type MsgValue, MessageError, type Node, buildTree, inlineRefs, normalize, pick, refsOf, simplify } from "./resolve.ts";
import { loadSource } from "./source.ts";
import { loadCtx } from "./types.ts";
import { EN, OS, PLURAL, addKey, area, areaFile, check, cleanup, editJson, finding, makeRepo, put, read } from "./testkit.ts";

afterAll(cleanup);

const lib: Record<string, MsgValue> = {
	common_os_trash: OS("Trash", "Recycle Bin", "Trash"),
	common_os_file_manager: OS("Finder", "File Explorer", "the file manager"),
	common_os_alt_key: OS("Option", "Alt", "Alt"),
};
const lookup = (k: string) => lib[k];

/** The text a value gives for one assignment of selector values (`*` = none of the literals). */
const textFor = (v: MsgValue, assign: Record<string, string>) => pick(normalize(v), assign)?.text;

describe("inlineRefs", () => {
	test("a message without references is returned as it is", () => {
		const v = PLURAL("{count} file", "{count} files");
		expect(inlineRefs(v, lookup)).toBe(v);
		expect(inlineRefs("Plain", lookup)).toBe("Plain");
		expect(refsOf("A {@x} and {@y} and {@x}")).toEqual(["x", "y"]);
	});

	test("a string with a reference becomes a select on os: every platform composes its own noun", () => {
		const r = inlineRefs("Moved to {@common_os_trash}", lookup);
		for (const [os, want] of [["mac", "Moved to Trash"], ["windows", "Moved to Recycle Bin"], ["linux", "Moved to Trash"], ["freebsd", "Moved to Trash"]]) {
			expect(textFor(r, { os })).toBe(want);
		}
		const m = normalize(r);
		expect(m.selectors).toEqual(["os"]);
		expect(m.declarations).toEqual(["input os"]);
	});

	test("an arm equal to the catch-all is dropped, and a selector that never matters disappears", () => {
		const m = normalize(inlineRefs("Moved to {@common_os_trash}", lookup));
		expect(m.arms.map((a) => a.cond.os)).toEqual(["windows", "*"]);
		expect(inlineRefs("Look: {@same}", (k) => (k === "same" ? OS("x", "x", "x") : undefined))).toBe("Look: x");
	});

	test("cross product: a reference inside a plural message selects on both, every combination right", () => {
		const plural: MsgValue = [{ declarations: ["input count", "local countPlural = count: plural"], selectors: ["countPlural"], match: { "countPlural=one": "{count} file moved to {@common_os_trash}", "countPlural=*": "{count} files moved to {@common_os_trash}" } }];
		const r = inlineRefs(plural, lookup);
		const m = normalize(r);
		expect(m.selectors).toEqual(["countPlural", "os"]);
		expect(m.declarations).toEqual(["input count", "local countPlural = count: plural", "input os"]);
		for (const c of ["one", "other"]) {
			for (const [os, noun] of [["mac", "Trash"], ["windows", "Recycle Bin"], ["linux", "Trash"]]) {
				const plural = c === "one" ? "file" : "files";
				expect(textFor(r, { countPlural: c === "one" ? "one" : "*", os })).toBe(`{count} ${plural} moved to ${noun}`);
			}
		}
	});

	test("two references with the same selector share it; different selectors are both kept", () => {
		const r = inlineRefs("Open {@common_os_file_manager} or {@common_os_trash}", lookup);
		expect(normalize(r).selectors).toEqual(["os"]);
		expect(textFor(r, { os: "mac" })).toBe("Open Finder or Trash");
		expect(textFor(r, { os: "windows" })).toBe("Open File Explorer or Recycle Bin");
		expect(textFor(r, { os: "linux" })).toBe("Open the file manager or Trash");
		const withCount: Record<string, MsgValue> = { ...lib, n: PLURAL("one thing", "many things") };
		const both = inlineRefs("Done: {@n}, in {@common_os_trash}", (k) => withCount[k]);
		expect(normalize(both).selectors.sort()).toEqual(["countPlural", "os"]);
		expect(textFor(both, { countPlural: "one", os: "windows" })).toBe("Done: one thing, in Recycle Bin");
	});

	test("the message's own selector beats a referenced one: the earliest selector decides", () => {
		const v: MsgValue = [{ declarations: ["input os"], selectors: ["os"], match: { "os=mac": "On a Mac: {@common_os_trash}", "os=*": "Elsewhere: {@common_os_trash}" } }];
		const r = inlineRefs(v, lookup);
		expect(textFor(r, { os: "mac" })).toBe("On a Mac: Trash");
		expect(textFor(r, { os: "windows" })).toBe("Elsewhere: Recycle Bin");
	});

	test("references chain: a reaches b reaches c", () => {
		const chain: Record<string, MsgValue> = { b: "[{@c}]", c: OS("C-mac", "C-win", "C-other") };
		const r = inlineRefs("a {@b}", (k) => chain[k]);
		expect(textFor(r, { os: "mac" })).toBe("a [C-mac]");
		expect(textFor(r, { os: "linux" })).toBe("a [C-other]");
	});

	test("a missing target and a cycle throw a dangling-ref MessageError naming them", () => {
		expect(() => inlineRefs("x {@nope}", lookup)).toThrow("{@nope} is not a message");
		const loop: Record<string, MsgValue> = { a: "A {@b}", b: "B {@a}" };
		try {
			inlineRefs("{@a}", (k) => loop[k], ["start"]);
			throw new Error("no throw");
		} catch (e) {
			expect(e).toBeInstanceOf(MessageError);
			expect((e as MessageError).rule).toBe("dangling-ref");
			expect((e as Error).message).toContain("cycle: start -> a -> b -> a");
		}
	});

	test("one name declared as a plural by one message and a plain select by another is refused", () => {
		const clash: MsgValue = [{ declarations: ["input os", "local os = count: plural"], selectors: ["os"], match: { "os=one": "x", "os=*": "y" } }];
		expect(() => inlineRefs("{@x}", () => clash, [])).not.toThrow();
		expect(() => inlineRefs([{ declarations: ["input os"], selectors: ["os"], match: { "os=mac": "{@x}", "os=*": "z" } }], () => clash)).toThrow("declared differently");
	});

	test("shrink: false keeps a selector the text never depends on (what a translation's shape is compared by)", () => {
		const flat: Record<string, MsgValue> = { t: OS("Lixeira", "Lixeira", "Lixeira") };
		expect(normalize(inlineRefs("Para {@t}", (k) => flat[k])).selectors).toEqual([]);
		expect(normalize(inlineRefs("Para {@t}", (k) => flat[k], [], false)).selectors).toEqual(["os"]);
	});
});

describe("simplify", () => {
	test("drops a selector whose value never changes the text, and arms equal to their catch-all", () => {
		const m = simplify({ selectors: ["a", "b"], declarations: ["input a", "input b"], arms: [{ cond: { a: "1", b: "x" }, text: "T" }, { cond: { a: "1", b: "*" }, text: "T" }, { cond: { a: "*", b: "*" }, text: "T" }] });
		expect(m.selectors).toEqual([]);
		expect(m.arms).toEqual([{ cond: {}, text: "T" }]);
	});
});

/** Evaluate a tree for an assignment of selector values (what a select on those would pick). */
function evalTree(n: Node, assign: Record<string, string>): string {
	if ("text" in n) return n.text;
	const v = assign[n.sel.name];
	return evalTree(n.branches.find((b) => b.literal === v)?.node ?? n.fallback, assign);
}

/** Evaluate a tree for the VARIABLES it selects on, as a Rust, Fluent-like or ICU select does: an exact number first, then the plural category. */
function evalVars(n: Node, vars: Record<string, string | number>): string {
	if ("text" in n) return n.text;
	const v = vars[n.sel.var];
	const category = typeof v === "number" ? new Intl.PluralRules("en").select(v) : "";
	const hit = n.branches.find((b) => (/^\d+$/.test(b.literal) ? Number(b.literal) === Number(v) : n.sel.kind === "plural" ? b.literal === category : b.literal === String(v)));
	return evalVars(hit?.node ?? n.fallback, vars);
}

describe("buildTree: nested selects", () => {
	const settingsHost: MsgValue = [
		{
			declarations: ["input count", "local countExact = count", "local countPlural = count: plural"],
			selectors: ["countExact", "countPlural"],
			match: { "countExact=0, countPlural=*": "none", "countExact=*, countPlural=one": "one", "countExact=*, countPlural=*": "many" },
		},
	];

	test("exact zero and plural categories on the same input are ONE select: [0], [one], other", () => {
		const t = buildTree(normalize(settingsHost));
		if (!("sel" in t)) throw new Error("expected a select");
		expect(t.sel.var).toBe("count");
		expect(t.branches.map((b) => b.literal)).toEqual(["0", "one"]);
		expect(t.branches.every((b) => "text" in b.node)).toBe(true);
		expect(evalVars(t, { count: 0 })).toBe("none");
		expect(evalVars(t, { count: 1 })).toBe("one");
		expect(evalVars(t, { count: 5 })).toBe("many");
	});

	test("the tree gives the same text as MF2 best-match for every count", () => {
		const m = normalize(settingsHost);
		const t = buildTree(m);
		for (let count = 0; count <= 25; count++) {
			const category = new Intl.PluralRules("en").select(count);
			const want = pick(m, { countExact: count === 0 ? "0" : "*", countPlural: category === "one" ? "one" : "*" })?.text;
			expect(evalVars(t, { count }), `count ${count}`).toBe(want);
		}
	});

	test("two different variables nest; an arm equal to the fallback is not repeated", () => {
		const v: MsgValue = [
			{
				declarations: ["input os", "input kind"],
				selectors: ["os", "kind"],
				match: { "os=mac, kind=file": "mac file", "os=mac, kind=*": "mac thing", "os=*, kind=file": "file", "os=*, kind=*": "thing" },
			},
		];
		const t = buildTree(normalize(v));
		expect(evalTree(t, { os: "mac", kind: "file" })).toBe("mac file");
		expect(evalTree(t, { os: "mac", kind: "dir" })).toBe("mac thing");
		expect(evalTree(t, { os: "linux", kind: "file" })).toBe("file");
		expect(evalTree(t, { os: "linux", kind: "dir" })).toBe("thing");
		const redundant = buildTree(normalize(OS("same", "same", "same")));
		expect(redundant).toEqual({ text: "same" });
	});

	test("an earlier selector's literal wins even when a later selector would match a catch-all", () => {
		const v: MsgValue = [{ declarations: ["input a", "input b"], selectors: ["a", "b"], match: { "a=1, b=*": "A", "a=*, b=2": "B", "a=*, b=*": "C" } }];
		const t = buildTree(normalize(v));
		expect(evalTree(t, { a: "1", b: "2" })).toBe("A");
		expect(evalTree(t, { a: "9", b: "2" })).toBe("B");
		expect(evalTree(t, { a: "9", b: "9" })).toBe("C");
	});

	test("a message with no catch-all variant cannot be expressed and throws with the reason", () => {
		const v: MsgValue = [{ declarations: ["input os"], selectors: ["os"], match: { "os=mac": "m", "os=windows": "w" } }];
		expect(() => buildTree(normalize(v))).toThrow("no catch-all");
	});
});

describe("ARB (the future mobile surface)", () => {
	test("plural, exact zero and select become ICU with nested arms", () => {
		const plural = arbMessage("files_count", [{ declarations: ["input count", "local countExact = count", "local countPlural = count: plural"], selectors: ["countExact", "countPlural"], match: { "countExact=0, countPlural=*": "No files", "countExact=*, countPlural=one": "{count} file", "countExact=*, countPlural=*": "{count} files" } }], "Count of files");
		expect(plural.files_count).toBe("{count, plural, =0{No files} one{{count} file} other{{count} files}}");
		expect(plural["@files_count"]).toEqual({ description: "Count of files", placeholders: { count: { type: "num" } } });
		const os = arbMessage("trash", OS("Trash", "Recycle Bin", "Trash"));
		expect(os.trash).toBe("{os, select, windows{Recycle Bin} other{Trash}}");
		expect(os["@trash"]).toEqual({ placeholders: { os: { type: "String" } } });
	});

	test("apostrophes and literal braces are escaped the ICU way, plain variables keep their name", () => {
		expect(arbMessage("a", "Don't {name} \\{x\\}").a).toBe("Don''t {name} '{'x'}'");
	});

	test("a file has @@locale in Flutter's underscore form, then the messages by key", () => {
		const f = JSON.parse(arbFile("pt-BR", [{ key: "b_two", value: "Dois" }, { key: "a_one", value: "Um", description: "First" }]));
		expect(Object.keys(f)).toEqual(["@@locale", "a_one", "@a_one", "b_two"]);
		expect(f["@@locale"]).toBe("pt_BR");
	});
});

describe("the Paraglide generator", () => {
	test("writes one file per locale with only the consumed areas, sorted, references inlined", async () => {
		const root = await makeRepo();
		addKey(root, "app_gone", "Moved to {@common_os_trash}", "Movido para {@common_os_trash}", "Toast after a delete");
		const source = loadSource(loadCtx(root));
		const outs = outputsFor(source, "desktop");
		expect(outs.map((o) => o.path)).toEqual(["apps/desktop/.i18n/en.json", "apps/desktop/.i18n/pt-BR.json"]);
		const en = JSON.parse(outs[0].content);
		expect(Object.keys(en)).toEqual([...Object.keys(en)].sort());
		expect(Object.keys(en)).toContain("common_action_cancel");
		expect(Object.keys(en)).not.toContain("cli_lang_current");
		expect(en.app_gone[0].match["os=windows"]).toBe("Moved to Recycle Bin");
		expect(en.app_gone[0].match["os=*"]).toBe("Moved to Trash");
		const pt = JSON.parse(outs[1].content);
		// pt-BR's term reads the same on every OS, so its generated message needs no select
		expect(pt.app_gone).toBe("Movido para Lixeira");
	});

	test("a key a locale lacks is left out of that file (the runtime falls back to English)", async () => {
		const root = await makeRepo();
		editJson(root, area("app", "pt-BR"), (o) => void delete o.app_welcome);
		const outs = outputsFor(loadSource(loadCtx(root)), "desktop");
		expect(Object.keys(JSON.parse(outs[1].content))).not.toContain("app_welcome");
		expect(Object.keys(JSON.parse(outs[0].content))).toContain("app_welcome");
	});

	test("a message that has no variants stays byte for byte what the source says", async () => {
		const root = await makeRepo();
		const en = JSON.parse(outputsFor(loadSource(loadCtx(root)), "desktop")[0].content);
		expect(en.app_files_count).toEqual(JSON.parse(read(root, EN)).app_files_count);
		expect(en.app_welcome).toBe("Welcome to Atlas, {name}");
	});

	test("generateAll writes the files; running it again writes nothing and leaves the mtimes", async () => {
		const root = await makeRepo();
		const first = generateAll(loadCtx(root));
		expect(first).toEqual(["apps/desktop/.i18n/en.json", "apps/desktop/.i18n/pt-BR.json"]);
		const path = join(root, "apps/desktop/.i18n/en.json");
		const m = statSync(path).mtimeMs;
		expect(generateAll(loadCtx(root))).toEqual([]);
		expect(statSync(path).mtimeMs).toBe(m);
		editJson(root, EN, (o) => void (o.app_welcome = "Welcome"));
		expect(generateAll(loadCtx(root))).toEqual(["apps/desktop/.i18n/en.json"]);
	});

	test("the rust surface writes nothing (Rust embeds the JSON itself)", async () => {
		const root = await makeRepo();
		expect(outputsFor(loadSource(loadCtx(root)), "rust")).toEqual([]);
		expect(generateSurface(loadSource(loadCtx(root)), "rust")).toEqual([]);
		expect(existsSync(join(root, "crates/atlas-i18n/locales"))).toBe(false);
	});

	test("an arb surface writes app_<locale>.arb with ICU messages; nothing declares one in the repo yet", async () => {
		const root = await makeRepo();
		put(root, "i18n/surfaces.json", read(root, "i18n/surfaces.json").replace('"rust": {', '"mobile": { "arb": "apps/mobile/lib/l10n" },\n\t\t"rust": {'));
		put(root, area("cli", "area"), areaFile("mobile"));
		const outs = outputsFor(loadSource(loadCtx(root)), "mobile");
		expect(outs.map((o) => o.path)).toEqual(["apps/mobile/lib/l10n/app_en.arb", "apps/mobile/lib/l10n/app_pt_BR.arb"]);
		const en = JSON.parse(outs[0].content);
		expect(en["@@locale"]).toBe("en");
		expect(en.cli_files_count).toBe("{count, plural, one{{count} file} other{{count} files}}");
		expect(en["@cli_files_count"].description).toContain("atlas files");
		expect(JSON.parse(readFileSync(join(import.meta.dir, "..", "..", "i18n/surfaces.json"), "utf8")).surfaces.mobile).toBeUndefined();
	});

	test("a shape a target cannot express is a `generate` finding naming the key, not a crash", async () => {
		const root = await makeRepo();
		put(root, "i18n/surfaces.json", read(root, "i18n/surfaces.json").replace('"rust": {', '"mobile": { "arb": "apps/mobile/l10n" },\n\t\t"rust": {'));
		put(root, area("app", "area"), areaFile("mobile"));
		const noCatchAll = [{ declarations: ["input os"], selectors: ["os"], match: { "os=mac": "Mac", "os=windows": "Win" } }];
		addKey(root, "app_no_default", noCatchAll, noCatchAll, "A message without a default variant");
		const f = finding(await check(root), "generate");
		expect(f?.message).toContain("no catch-all");
	});

	test("the generator runs with bun alone: nothing it imports needs svelte or any package", () => {
		const seen = new Set<string>();
		const visit = (file: string) => {
			if (seen.has(file)) return;
			seen.add(file);
			const text = readFileSync(file, "utf8");
			for (const m of text.matchAll(/^\s*import\s+(type\s+)?[^"']*?from\s+["']([^"']+)["']/gm)) {
				if (m[1]) continue; // a type-only import is erased
				const spec = m[2];
				if (spec.startsWith(".")) visit(join(dirname(file), spec));
				else expect(spec.startsWith("node:") || spec === "bun", `${file} imports "${spec}"`).toBe(true);
			}
		};
		visit(join(import.meta.dir, "generate.ts"));
		expect(seen.size).toBeGreaterThan(5);
		expect([...seen].some((f) => /svelte|hardcoded/.test(f))).toBe(false);
	});
});
