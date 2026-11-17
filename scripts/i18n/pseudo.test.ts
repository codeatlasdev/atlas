// The en-XA pseudo-locale: every variant of every message is transformed, nothing structural is touched.
import { afterAll, describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { FluentParser } from "@fluent/syntax";
import { printFluentArea, pseudoCatalog, pseudoFluent, pseudoText, pseudoValue, writePseudoCatalogs } from "./pseudo.ts";
import type { ParaglideValue, Variant } from "./shape.ts";
import { check, cleanup, errors, makeRepo, read } from "./testkit.ts";

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

describe("Fluent", () => {
	const FTL = `# Reveal action
cli-reveal = { $os ->
    [mac] Reveal { $name } in Finder
    [windows] Reveal { $name } in File Explorer
   *[linux] Reveal { $name } in the file manager
}

# Count
cli-files =
    { $count ->
        [one] { $count } file
       *[other] { $count } files
    }
    .title = Files in { $dir }

# Plain
cli-save = Save { term-trash }
`;

	test("every select variant and attribute is transformed; ids, variables, selectors and references untouched", () => {
		const out = pseudoFluent(FTL);
		const parsed = new FluentParser().parse(out);
		expect(parsed.body.some((e) => e.type === "Junk")).toBe(false);
		expect(parsed.body.filter((e) => e.type === "Message").map((e) => e.id.name)).toEqual(["cli-reveal", "cli-files", "cli-save"]);
		expect(out).toContain("$os ->");
		expect(out).toContain("[mac]");
		expect(out).toContain("[windows]");
		expect(out).toContain("*[linux]");
		expect(out).toContain("$count ->");
		expect(out).toContain("{ term-trash }");
		for (const line of out.split("\n").filter((l) => /^\s*\*?\[(mac|windows|linux|one|other)\]/.test(l))) {
			expect(line).toMatch(/⟦.*⟧$/);
		}
		expect(out).toContain(".title = ⟦");
		expect(out).toContain("cli-save = ⟦");
		expect(out).not.toContain("# Reveal action");
	});

	test("stable: transforming twice, or the output again, gives the same text", () => {
		const once = pseudoFluent(FTL);
		expect(pseudoFluent(FTL)).toBe(once);
		expect(pseudoFluent(once)).toBe(once);
	});

	test("--print area prints the English messages of one area, and refuses an unknown area", async () => {
		const root = await makeRepo();
		const out = printFluentArea(root, "cli");
		expect(out).toContain("cli-lang-current = ⟦");
		expect(out).toContain("{ $lang }");
		expect(() => printFluentArea(root, "nope")).toThrow("areas: cli");
	});
});

describe("en-XA files", () => {
	test("written next to the English catalog, transformed key by key, and invisible to every checker", async () => {
		const root = await makeRepo();
		const before = JSON.stringify(await check(root));
		const [file] = writePseudoCatalogs(root);
		expect(file).toBe(join(root, "apps/desktop/messages/en-XA.json"));
		const en = JSON.parse(read(root, "apps/desktop/messages/en.json"));
		const xa = JSON.parse(read(root, "apps/desktop/messages/en-XA.json"));
		expect(Object.keys(xa)).toEqual(Object.keys(en));
		expect(xa.app_save_button).toBe("⟦Sávé ~~⟧");
		expect(JSON.stringify(await check(root))).toBe(before);
		expect(errors(await check(root))).toEqual([]);
		expect(existsSync(join(root, "crates/atlas-i18n/locales/en-XA"))).toBe(false);
		const again = read(root, "apps/desktop/messages/en-XA.json");
		writePseudoCatalogs(root);
		expect(read(root, "apps/desktop/messages/en-XA.json")).toBe(again);
	});
});
