// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { parseBase16 } from "./base16.ts";
import { ThemeImportError } from "./errors.ts";

const PALETTE = `base00: "282a36"
base01: "#34353e"
base02: "44475a"
base03: "6272a4"
base04: "62d6e8"
base05: "f8f8f2"
base06: "f1f2f8"
base07: "ffffff"
base08: "ff5555"
base09: "ffb86c"
base0A: "f1fa8c"
base0B: "50fa7b"
base0C: "8be9fd"
base0D: "bd93f9"
base0E: "ff79c6"
base0F: "cc6666"`;

describe("parseBase16", () => {
	test("tinted-theming nested palette", () => {
		const nested = PALETTE.split("\n")
			.map((l) => `  ${l}`)
			.join("\n");
		const r = parseBase16(
			`system: "base16"\nname: "Dracula"\nauthor: "x"\nvariant: "dark"\npalette:\n${nested}\n`,
		);
		expect(r.name).toBe("Dracula");
		expect(r.appearance).toBe("dark");
		expect(r.warnings).toEqual([]);
		expect(r.style.background).toBe("#282a36");
		expect(r.style.text).toBe("#f8f8f2");
		expect(r.style["surface.background"]).toBe("#34353e");
		expect(r.style["text.disabled"]).toBe("#6272a4");
		expect(r.style["terminal.ansi.black"]).toBe("#282a36");
		expect(r.style["terminal.ansi.red"]).toBe("#ff5555");
		expect(r.style["terminal.ansi.green"]).toBe("#50fa7b");
		expect(r.style["terminal.ansi.yellow"]).toBe("#f1fa8c");
		expect(r.style["terminal.ansi.blue"]).toBe("#bd93f9");
		expect(r.style["terminal.ansi.magenta"]).toBe("#ff79c6");
		expect(r.style["terminal.ansi.cyan"]).toBe("#8be9fd");
		expect(r.style["terminal.ansi.white"]).toBe("#f8f8f2");
		expect(r.style["terminal.ansi.bright_black"]).toBe("#6272a4");
		expect(r.style["terminal.ansi.bright_red"]).toBe("#ff5555");
		expect(r.style["terminal.ansi.bright_white"]).toBe("#ffffff");
		expect(r.style.syntax.keyword.color).toBe("#ff79c6");
		expect(r.style.syntax.function.color).toBe("#bd93f9");
		expect(r.style.syntax.string.color).toBe("#50fa7b");
		expect(r.style.syntax.number.color).toBe("#ffb86c");
		expect(r.style.syntax.type.color).toBe("#f1fa8c");
		expect(r.style.syntax.comment.color).toBe("#6272a4");
	});

	test("legacy top-level scheme/baseXX with and without #, light variant", () => {
		const flat = PALETTE.replace(/"/g, "");
		const r = parseBase16(`scheme: Legacy\nvariant: light\n${flat}\n`);
		expect(r.name).toBe("Legacy");
		expect(r.appearance).toBe("light");
		expect(r.style["surface.background"]).toBe("#34353e");
		expect(r.style.background).toBe("#282a36");
	});

	test("numeric-looking hex values keep leading zeros; appearance inferred", () => {
		const r = parseBase16("base00: 000000\nbase05: ffffff\n");
		expect(r.style.background).toBe("#000000");
		expect(r.appearance).toBe("dark");
		expect(r.name).toBe("Base16 import");
	});

	test("invalid colors warn and fall back; missing base00 throws", () => {
		const r = parseBase16("base00: 000000\nbase05: ffffff\nbase08: zzz\n");
		expect(r.warnings).toEqual([m.settings_theme_diag_invalid_color({ key: "base08", value: "zzz" })]);
		expect(r.style["terminal.ansi.red"]).toBe("#ffffff");
		expect(() => parseBase16("name: x\nbase05: ffffff\n")).toThrow(ThemeImportError);
		expect(() => parseBase16("- a\n- b\n")).toThrow(ThemeImportError);
	});
});
