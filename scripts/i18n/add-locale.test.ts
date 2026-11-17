// i18n:add-locale edits exactly the files it promises, and the new locale then behaves as a community locale.
import { afterAll, describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { addLocale, addToLocalesArray, serializeLocales } from "./add-locale.ts";
import { loadLocales } from "./types.ts";
import { check, cleanup, errors, finding, makeRepo, put, read, rulesOf } from "./testkit.ts";

afterAll(cleanup);

const CODEOWNERS = `*  @org/maintainers
/docs/  @org/maintainers

# Translations
**/pt-BR/**                  @mvilacad @luancoleto
/i18n/messages/*/pt-BR.json  @mvilacad @luancoleto
/i18n/glossary.yaml          @org/maintainers
/i18n/style/                 @org/maintainers
`;

const repo = () => makeRepo({ ".github/CODEOWNERS": CODEOWNERS });

describe("addLocale", () => {
	test("edits locales.json, CODEOWNERS, the project settings, lang.rs, Info.plist and creates an empty file in every area", async () => {
		const root = await repo();
		const log = addLocale(root, "es", { owner: "@someone" });

		expect(loadLocales(root).locales.es).toEqual({ tier: "community", owners: ["@someone"] });
		expect(read(root, "i18n/locales.json")).toBe(serializeLocales(loadLocales(root)));
		expect(Object.keys(loadLocales(root).locales)).toEqual(["en", "pt-BR", "es"]);

		const owners = read(root, ".github/CODEOWNERS").split("\n");
		const at = owners.findIndex((l) => l.startsWith("/i18n/messages/*/pt-BR.json"));
		expect(owners.slice(at + 1, at + 3)).toEqual([`${"**/es/**".padEnd(28)} @someone`, `${"/i18n/messages/*/es.json".padEnd(28)} @someone`]);
		expect(owners[at + 3]).toStartWith("/i18n/glossary.yaml");

		expect(JSON.parse(read(root, "apps/desktop/project.inlang/settings.json")).locales).toEqual(["en", "pt-BR", "es"]);
		for (const a of ["app", "cli", "common", "common_os"]) expect(read(root, `i18n/messages/${a}/es.json`)).toBe("{}\n");
		expect(read(root, "crates/atlas-core/src/lang.rs")).toContain('pub const SUPPORTED: &[&str] = &["en", "pt-BR", "es"];');
		expect(read(root, "crates/atlas-desktop/Info.plist")).toContain("\t\t<string>pt-BR</string>\n\t\t<string>es</string>\n\t</array>");
		expect(read(root, "apps/desktop/.i18n/es.json")).toBe("{}\n");
		expect(existsSync(join(root, "crates/atlas-i18n/locales"))).toBe(false);
		expect(log.join("\n")).toContain("bun run i18n:translate --locale es");
	});

	test("the new community locale warns about missing keys and never fails the gate; every list agrees (supported-sync)", async () => {
		const root = await repo();
		addLocale(root, "es", { owner: "@someone" });
		const fs = await check(root);
		expect(errors(fs).filter((f) => f.file.includes("/es.json"))).toEqual([]);
		const missing = fs.filter((f) => f.rule === "missing" && f.file.endsWith("es.json"));
		expect(missing.length).toBeGreaterThan(0);
		expect(missing.every((f) => f.severity === "warn")).toBe(true);
		expect(rulesOf(errors(fs))).toEqual([]);
		expect(finding(fs, "supported-sync")).toBeUndefined();
	});

	test("refuses a bad tag, a bad owner and an existing locale, touching nothing", async () => {
		const root = await repo();
		const before = read(root, "i18n/locales.json");
		expect(() => addLocale(root, "pt_br")).toThrow("BCP-47");
		expect(() => addLocale(root, "es", { owner: "someone" })).toThrow("--owner");
		expect(() => addLocale(root, "pt-BR")).toThrow("already");
		expect(read(root, "i18n/locales.json")).toBe(before);
		expect(read(root, "crates/atlas-core/src/lang.rs")).toContain('"en", "pt-BR"]');
	});

	test("tolerates a tree without lang.rs, CODEOWNERS or Info.plist and says so", async () => {
		const root = await makeRepo({ "crates/atlas-core/src/lang.rs": null, "crates/atlas-desktop/Info.plist": null });
		const log = addLocale(root, "fr").join("\n");
		expect(log).toContain("crates/atlas-core/src/lang.rs: not found");
		expect(log).toContain(".github/CODEOWNERS: not found");
		expect(log).toContain("Info.plist: not found");
		expect(JSON.parse(read(root, "apps/desktop/project.inlang/settings.json")).locales).toEqual(["en", "pt-BR", "fr"]);
	});

	test("says so when the SUPPORTED line has another shape, and does not guess", async () => {
		const root = await makeRepo({ "crates/atlas-core/src/lang.rs": 'pub const SUPPORTED: [&str; 2] = ["en", "pt-BR"];\n' });
		expect(addLocale(root, "fr").join("\n")).toContain("no `pub const SUPPORTED: &[&str] = &[...]` line found");
		expect(read(root, "crates/atlas-core/src/lang.rs")).toBe('pub const SUPPORTED: [&str; 2] = ["en", "pt-BR"];\n');
	});

	test("a second community locale appends after the first one's CODEOWNERS lines", async () => {
		const root = await repo();
		addLocale(root, "es", { owner: "@a" });
		addLocale(root, "fr", { owner: "@b" });
		const lines = read(root, ".github/CODEOWNERS").split("\n").filter((l) => /\/(es|fr)[/.]/.test(l));
		expect(lines).toHaveLength(4);
		expect(read(root, "crates/atlas-core/src/lang.rs")).toContain('["en", "pt-BR", "es", "fr"]');
	});

	test("the lock file and the other languages of an area are untouched", async () => {
		const root = await repo();
		const lock = read(root, "i18n/messages/app/lock.json");
		const pt = read(root, "i18n/messages/app/pt-BR.json");
		addLocale(root, "es");
		expect(read(root, "i18n/messages/app/lock.json")).toBe(lock);
		expect(read(root, "i18n/messages/app/pt-BR.json")).toBe(pt);
		put(root, "i18n/messages/app/es.json", '{ "app_save_button": "Guardar" }\n');
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
