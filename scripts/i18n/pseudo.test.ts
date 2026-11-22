// The en-XA pseudo-locale: every variant of every message is transformed, nothing structural is touched.
import { afterAll, describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { printArea, pseudoCatalog, pseudoText, pseudoValue, writePseudoCatalogs } from "./pseudo.ts";
import type { ParaglideValue, Variant } from "./shape.ts";
import { generateAll } from "./generate.ts";
import { loadCtx } from "./types.ts";
import { EN, META, check, cleanup, editJson, errors, makeRepo, read } from "./testkit.ts";

afterAll(cleanup);

const OS_MESSAGE: Variant[] = [
	{
		declarations: ["input os", "input name"],
		selectors: ["os"],
		match: { "os=mac": "Reveal {name} in Finder", "os=windows": "Reveal {name} in File Explorer", "os=linux": "Reveal {name} in the file manager", "os=*": "Reveal {name}" },
	},
];

const PLURAL_OS: Variant[] = [
	{
		declarations: ["input count", "input os", "local countPlural = count: plural"],
		selectors: ["os", "countPlural"],
		match: { "os=mac countPlural=one": "{count} file in Trash", "os=mac countPlural=*": "{count} files in Trash", "os=* countPlural=one": "{count} file in the Recycle Bin", "os=* countPlural=*": "{count} files in the Recycle Bin" },
	},
];

const bracketed = (t: string) => t.startsWith("⟦") && t.endsWith("⟧");
const vars = (t: string) => [...t.matchAll(/\{[^{}]*\}/g)].map((m) => m[0]);

describe("pseudoText", () => {
	test("accents, pads by 35% of the letters, wraps; the same text always gives the same result", () => {
		expect(pseudoText("Save")).toBe("⟦Sávé ~~⟧");
		// 23 letters: ceil(23 * 0.35) = 9 pad characters
		expect(pseudoText("Open the project folder now")).toBe("⟦Ópéñ thé prójéçt fóldér ñów ~~~~~~~~~⟧");
		expect(pseudoText("Save")).toBe(pseudoText("Save"));
	});

	test("variables, markup, code spans and URLs are kept byte for byte", () => {
		const src = "Run `atlas dev` in {name} or {#link to=|/docs|}read https://atlas.codeatlas.com.br/docs{/link}";
		const out = pseudoText(src);
		expect(vars(out)).toEqual(vars(src));
		expect(out).toContain("`atlas dev`");
		expect(out).toContain("https://atlas.codeatlas.com.br/docs");
		expect(out).toContain("{#link to=|/docs|}");
		expect(pseudoText("a \\{b\\}")).toContain("\\{b\\}");
		expect(out).toContain("{/link}");
		expect(bracketed(out)).toBe(true);
	});

	test("a text with no letters is still bracketed, and bracketing twice changes nothing", () => {
		expect(pseudoText("{count}")).toBe("⟦{count}⟧");
		for (const s of ["Save", "{count} files", "42", "Run `x`"]) expect(pseudoText(pseudoText(s))).toBe(pseudoText(s));
	});
});

describe("Paraglide variants (os select x plural)", () => {
	test.each([["os select", OS_MESSAGE], ["os x plural", PLURAL_OS]])("%s: every variant is transformed, keys and structure untouched, stable", (_n, variants) => {
		const out = pseudoValue(variants) as Variant[];
		expect(out).toHaveLength(variants.length);
		for (const [i, v] of variants.entries()) {
			expect(out[i].declarations).toEqual(v.declarations);
			expect(out[i].selectors).toEqual(v.selectors);
			expect(Object.keys(out[i].match)).toEqual(Object.keys(v.match));
			for (const [k, text] of Object.entries(v.match)) {
				expect(bracketed(out[i].match[k])).toBe(true);
				expect(vars(out[i].match[k])).toEqual(vars(text));
				expect(out[i].match[k]).not.toBe(text);
			}
		}
		expect(pseudoValue(out)).toEqual(out);
		expect(pseudoValue(variants)).toEqual(out);
	});

	test.each(["mac", "windows", "linux"])("the %s arm of the os select keeps its selector key and placeholder", (os) => {
		const [out] = pseudoValue(OS_MESSAGE) as Variant[];
		const key = `os=${os}`;
		expect(out.match[key]).toBeDefined();
		expect(out.match[key]).toContain("{name}");
		expect(out.match[key]).toMatch(/^⟦.*⟧$/);
	});

	test("a catalog: keys and $schema untouched, every value transformed", () => {
		const en: Record<string, ParaglideValue> = { $schema: "https://inlang.com/schema/inlang-message-format", app_a_one: "Save", app_b_two: OS_MESSAGE };
		const out = pseudoCatalog(en);
		expect(Object.keys(out)).toEqual(Object.keys(en));
		expect(out.$schema).toBe(en.$schema);
		expect(bracketed(out.app_a_one as string)).toBe(true);
	});
});

describe("an area as JSON (--print)", () => {
	test("every value of an area is transformed, keys and references kept; an unknown area is refused", async () => {
		const root = await makeRepo();
		const out = JSON.parse(printArea(root, "cli"));
		expect(Object.keys(out)).toEqual(["cli_files_count", "cli_lang_current"]);
		expect(out.cli_lang_current).toBe("⟦Láñgúágé: {lang} ~~~⟧");
		for (const m of Object.values((out.cli_files_count as Variant[])[0].match)) expect(bracketed(m)).toBe(true);
		expect(() => printArea(root, "nope")).toThrow("areas:");
	});

	test("references are kept as written so Rust still evaluates them", async () => {
		const root = await makeRepo();
		const out = JSON.parse(printArea(root, "common_os"));
		expect(out.common_os_trash).toBeDefined();
		const app = await makeRepo();
		editJson(app, EN, (o) => void (o.app_gone = "Moved to {@common_os_trash}"));
		editJson(app, META, (o) => void (o.app_gone = "Toast"));
		expect(JSON.parse(printArea(app, "app")).app_gone).toContain("{@common_os_trash}");
	});
});

describe("en-XA files", () => {
	test("written next to the generated English input, transformed key by key, and invisible to every checker", async () => {
		const root = await makeRepo();
		generateAll(loadCtx(root));
		const before = JSON.stringify(await check(root));
		const [file] = writePseudoCatalogs(root);
		expect(file).toBe(join(root, "apps/desktop/.i18n/en-XA.json"));
		const en = JSON.parse(read(root, "apps/desktop/.i18n/en.json"));
		const xa = JSON.parse(read(root, "apps/desktop/.i18n/en-XA.json"));
		expect(Object.keys(xa)).toEqual(Object.keys(en));
		expect(xa.app_save_button).toBe("⟦Sávé ~~⟧");
		expect(JSON.stringify(await check(root))).toBe(before);
		expect(errors(await check(root))).toEqual([]);
		const again = read(root, "apps/desktop/.i18n/en-XA.json");
		writePseudoCatalogs(root);
		expect(read(root, "apps/desktop/.i18n/en-XA.json")).toBe(again);
	});

	test("an inlined reference is pseudo-localized in every variant of the generated input", async () => {
		const root = await makeRepo();
		editJson(root, EN, (o) => void (o.app_gone = "Moved to {@common_os_trash}"));
		editJson(root, META, (o) => void (o.app_gone = "Toast"));
		writePseudoCatalogs(root);
		const xa = JSON.parse(read(root, "apps/desktop/.i18n/en-XA.json"));
		const match = (xa.app_gone as Variant[])[0].match;
		expect(Object.keys(match).sort()).toEqual(["os=*", "os=windows"]);
		for (const t of Object.values(match)) expect(bracketed(t)).toBe(true);
	});
});

describe("the cases shared with the Rust implementation", () => {
	// crates/atlas-i18n/src/pseudo.rs derives en-XA in a debug build of the CLI/TUI/hostd/desktop shell and is held to the
	// same file: the two transforms must agree, or a layout pass in the terminal would not match the one in the app.
	const cases = JSON.parse(readFileSync(join(import.meta.dir, "../../i18n/pseudo-cases.json"), "utf8")) as { en: string; pseudo: string }[];
	test.each(cases.map((c) => [c.en, c.pseudo]))("%s", (en, pseudo) => expect(pseudoText(en)).toBe(pseudo));
});
