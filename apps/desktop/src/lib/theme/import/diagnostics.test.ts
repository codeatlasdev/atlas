// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../../test/locale.ts";
import { parseFamily } from "../schema.ts";
import { parseBase16 } from "./base16.ts";
import { parseGhosttyConfig } from "./ghostty.ts";
import { parseItermColors } from "./iterm.ts";

const ctx = { home: "/home/u", readFile: async () => null, themeDirs: [], appearance: "dark" };

// What an import or a theme file says is wrong is shown to the person in the dialog: it is read from the catalog when
// it is produced, so it follows the language that is set then (never an English string baked in at module load).
describe("import and theme file problems follow the display language", () => {
	test("a thrown import error", async () => {
		const english = (() => {
			try {
				parseBase16("base08: '#111111'");
			} catch (e) {
				return e.message;
			}
		})();
		expect(english).toBe(m.settings_theme_diag_base16_needs_base());
		await inLocale("pt-BR", () => {
			expect(() => parseBase16("base08: '#111111'")).toThrow(m.settings_theme_diag_base16_needs_base());
			expect(m.settings_theme_diag_base16_needs_base()).not.toBe(english);
		});
	});

	test("a warning that carries the key and the value", async () => {
		await inLocale("pt-BR", async () => {
			const r = await parseGhosttyConfig("foreground = nonsense\nbackground = #101010", ctx);
			expect(r.warnings).toEqual([
				m.settings_theme_diag_invalid_color({ key: "foreground", value: "nonsense" }),
			]);
			expect(r.warnings[0]).toContain("foreground");
			expect(r.warnings[0]).toContain("nonsense");
		});
	});

	test("an iTerm entry that cannot be read names the entry", async () => {
		const plist = `<plist><dict><key>Ansi 2 Color</key><dict><key>Red Component</key><string>x</string></dict><key>Ansi 1 Color</key><dict><key>Red Component</key><real>1</real><key>Green Component</key><real>0</real><key>Blue Component</key><real>0</real></dict></dict></plist>`;
		await inLocale("pt-BR", () => {
			const r = parseItermColors(plist, "t");
			expect(r.warnings).toEqual([m.settings_theme_diag_invalid_color_key({ key: "Ansi 2 Color" })]);
		});
	});

	test("the problems listed for a theme file", async () => {
		await inLocale("pt-BR", () => {
			expect(parseFamily({}).issues[0].message).toBe(m.settings_theme_diag_needs_themes());
			const bad = parseFamily({ themes: [{ name: "X", appearance: "mauve", style: {} }] });
			expect(bad.issues[0].message).toBe(m.settings_theme_diag_appearance({ name: "X" }));
			expect(bad.issues[0].message).toContain("X");
		});
	});
});
