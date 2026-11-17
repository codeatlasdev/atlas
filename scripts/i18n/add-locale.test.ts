// i18n:add-locale edits exactly the files it promises, and the new locale then behaves as a community locale.
import { afterAll, describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { addLocale, addToLocalesArray, serializeLocales } from "./add-locale.ts";
import { loadLocales } from "./types.ts";
import { check, cleanup, errors, makeRepo, put, read, rulesOf } from "./testkit.ts";

afterAll(cleanup);

const CODEOWNERS = `*  @org/maintainers
/docs/  @org/maintainers

# Translations
**/pt-BR/**                  @mvilacad @luancoleto
/*/*/messages/pt-BR.json     @mvilacad @luancoleto
/site/messages/pt-BR.json    @mvilacad @luancoleto
/i18n/glossary.yaml          @org/maintainers
/i18n/style/                 @org/maintainers
`;
const LANG_RS = 'pub const SUPPORTED: &[&str] = &["en", "pt-BR"];\npub const DEFAULT: &str = "en";\n';
const PLIST = `<plist version="1.0">
<dict>
\t<key>CFBundleLocalizations</key>
\t<array>
\t\t<string>en</string>
\t\t<string>pt-BR</string>
\t</array>
\t<key>Other</key>
\t<string>x</string>
</dict>
</plist>
`;

async function repo() {
	return makeRepo({
		".github/CODEOWNERS": CODEOWNERS,
		"crates/atlas-core/src/lang.rs": LANG_RS,
		"crates/atlas-desktop/Info.plist": PLIST,
		"crates/atlas-i18n/locales/en/cli.ftl": null,
		"crates/atlas-i18n/locales/pt-BR/cli.ftl": null,
	});
}

describe("addLocale", () => {
	test("edits locales.json, CODEOWNERS, settings, lang.rs, Info.plist and scaffolds catalogs and the .ftl dir", async () => {
		const root = await repo();
		put(root, "crates/atlas-i18n/locales/en/x.ftl", "# Probe\nx-a = A\n");
		const log = addLocale(root, "es", { owner: "@someone" });

		expect(loadLocales(root).locales.es).toEqual({ tier: "community", owners: ["@someone"] });
		expect(read(root, "i18n/locales.json")).toBe(serializeLocales(loadLocales(root)));
		expect(Object.keys(loadLocales(root).locales)).toEqual(["en", "pt-BR", "es"]);

		const owners = read(root, ".github/CODEOWNERS").split("\n");
		const at = owners.findIndex((l) => l.startsWith("/site/messages/pt-BR.json"));
		expect(owners.slice(at + 1, at + 4)).toEqual([
			"**/es/**                     @someone",
			"/*/*/messages/es.json        @someone",
			"/site/messages/es.json       @someone",
		]);
		expect(owners[at + 4]).toStartWith("/i18n/glossary.yaml");

		expect(JSON.parse(read(root, "apps/desktop/project.inlang/settings.json")).locales).toEqual(["en", "pt-BR", "es"]);
		expect(read(root, "apps/desktop/messages/es.json")).toBe("{}\n");
		expect(read(root, "crates/atlas-core/src/lang.rs")).toContain('pub const SUPPORTED: &[&str] = &["en", "pt-BR", "es"];');
		expect(read(root, "crates/atlas-desktop/Info.plist")).toContain("\t\t<string>pt-BR</string>\n\t\t<string>es</string>\n\t</array>");
		expect(existsSync(join(root, "crates/atlas-i18n/locales/es"))).toBe(true);
		expect(log.join("\n")).toContain("bun run i18n:translate --locale es");
	});

	test("the new community locale warns about missing keys and never fails the gate", async () => {
		const root = await repo();
		addLocale(root, "es", { owner: "@someone" });
		const fs = await check(root);
		expect(errors(fs).filter((f) => f.file.includes("/es.json"))).toEqual([]);
		expect(fs.filter((f) => f.rule === "missing" && f.file.endsWith("es.json")).every((f) => f.severity === "warn")).toBe(true);
		expect(fs.filter((f) => f.rule === "missing" && f.file.endsWith("es.json")).length).toBeGreaterThan(0);
		expect(rulesOf(errors(fs))).toEqual([]);
	});

	test("refuses a bad tag, a bad owner and an existing locale, touching nothing", async () => {
		const root = await repo();
		const before = read(root, "i18n/locales.json");
		expect(() => addLocale(root, "pt_br")).toThrow("BCP-47");
		expect(() => addLocale(root, "es", { owner: "someone" })).toThrow("--owner");
		expect(() => addLocale(root, "pt-BR")).toThrow("already");
		expect(read(root, "i18n/locales.json")).toBe(before);
		expect(read(root, "crates/atlas-core/src/lang.rs")).toBe(LANG_RS);
	});

	test("tolerates a tree without lang.rs, CODEOWNERS or Info.plist and says so", async () => {
		const root = await makeRepo();
		const log = addLocale(root, "fr").join("\n");
		expect(log).toContain("crates/atlas-core/src/lang.rs: not found");
		expect(log).toContain(".github/CODEOWNERS: not found");
		expect(log).toContain("Info.plist: not found");
		expect(JSON.parse(read(root, "apps/desktop/project.inlang/settings.json")).locales).toEqual(["en", "pt-BR", "fr"]);
	});

	test("says so when the SUPPORTED line has another shape, and does not guess", async () => {
		const root = await makeRepo({ "crates/atlas-core/src/lang.rs": "pub const SUPPORTED: [&str; 2] = [\"en\", \"pt-BR\"];\n" });
		expect(addLocale(root, "fr").join("\n")).toContain("no `pub const SUPPORTED: &[&str] = &[...]` line found");
		expect(read(root, "crates/atlas-core/src/lang.rs")).toBe("pub const SUPPORTED: [&str; 2] = [\"en\", \"pt-BR\"];\n");
	});

	test("a second community locale appends after the first one's CODEOWNERS lines", async () => {
		const root = await repo();
		addLocale(root, "es", { owner: "@a" });
		addLocale(root, "fr", { owner: "@b" });
		const lines = read(root, ".github/CODEOWNERS").split("\n").filter((l) => /\/(es|fr)[/.]/.test(l));
		expect(lines).toHaveLength(6);
		expect(read(root, "crates/atlas-core/src/lang.rs")).toContain('["en", "pt-BR", "es", "fr"]');
	});
});

describe("addToLocalesArray keeps the layout", () => {
	test.each([
		['{\n  "locales": ["en", "pt-BR"]\n}', '{\n  "locales": ["en", "pt-BR", "es"]\n}'],
		['{ "locales": [] }', '{ "locales": ["es"] }'],
		['{\n\t"locales": [\n\t\t"en",\n\t\t"pt-BR"\n\t]\n}', '{\n\t"locales": [\n\t\t"en",\n\t\t"pt-BR",\n\t\t"es"\n\t]\n}'],
	])("%#", (input, output) => {
		expect(addToLocalesArray(input, "es")).toBe(output);
	});

	test("already present: unchanged", () => {
		expect(addToLocalesArray('{ "locales": ["en", "es"] }', "es")).toBe('{ "locales": ["en", "es"] }');
	});
});
