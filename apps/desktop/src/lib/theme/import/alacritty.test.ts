// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { parseAlacritty } from "./alacritty.ts";
import { ThemeImportError } from "./errors.ts";

const TOML = `
[colors.primary]
background = "#101820"
foreground = '0xe0f0ff'

[colors.cursor]
text = "#101820"
cursor = "#ffcc00"

[colors.selection]
background = "#203040"
text = "#ffffff"

[colors.normal]
black = "#000011"
red = "#ff6b6b"
green = "#6bff95"
yellow = "#ffe66b"
blue = "#6b9bff"
magenta = "#c46bff"
cyan = "#6bf0ff"
white = "#d0d8e0"

[colors.bright]
black = "#334455"
red = "#ff8f8f"
white = "#ffffff"
`;

const YAML = `
colors:
  primary:
    background: '0xfaf4e8'
    foreground: '#2a2418'
  normal:
    black: 0x2a2418
    red: "#b03030"
  bright:
    red: '#d04848'
`;

describe("parseAlacritty", () => {
	test("toml: maps primary, cursor, selection, normal and bright", () => {
		const r = parseAlacritty(TOML, "night-test.toml");
		expect(r.name).toBe("night-test");
		expect(r.appearance).toBe("dark");
		expect(r.style["terminal.background"]).toBe("#101820");
		expect(r.style["terminal.foreground"]).toBe("#e0f0ff");
		expect(r.style["terminal.cursor"]).toBe("#ffcc00");
		expect(r.style["terminal.cursor_text"]).toBe("#101820");
		expect(r.style["terminal.selection.background"]).toBe("#203040");
		expect(r.style["terminal.selection.foreground"]).toBe("#ffffff");
		expect(r.style["terminal.ansi.red"]).toBe("#ff6b6b");
		expect(r.style["terminal.ansi.white"]).toBe("#d0d8e0");
		expect(r.style["terminal.ansi.bright_red"]).toBe("#ff8f8f");
		expect(r.style["terminal.ansi.bright_white"]).toBe("#ffffff");
		expect(r.warnings).toEqual([]);
	});

	test("legacy yaml: 0x colors, light appearance, nested path name", () => {
		const r = parseAlacritty(YAML, "themes/dawn-test.yml");
		expect(r.name).toBe("dawn-test");
		expect(r.appearance).toBe("light");
		expect(r.style["terminal.background"]).toBe("#faf4e8");
		expect(r.style["terminal.foreground"]).toBe("#2a2418");
		expect(r.style["terminal.ansi.black"]).toBe("#2a2418");
		expect(r.style["terminal.ansi.bright_red"]).toBe("#d04848");
	});

	test("invalid colors warn instead of throwing", () => {
		const r = parseAlacritty(`[colors.primary]\nbackground = "#101820"\nforeground = "nope"\n`, "x.toml");
		expect(r.style["terminal.foreground"]).toBeUndefined();
		expect(r.warnings).toEqual([
			m.settings_theme_diag_invalid_color({ key: "colors.primary.foreground", value: "nope" }),
		]);
	});

	test("garbage throws ThemeImportError", () => {
		expect(() => parseAlacritty("[[[", "x.toml")).toThrow(ThemeImportError);
		expect(() => parseAlacritty("a: [", "x.yml")).toThrow(ThemeImportError);
		expect(() => parseAlacritty("[font]\nsize = 12\n", "x.toml")).toThrow(ThemeImportError);
		expect(() => parseAlacritty("[colors.primary]\n", "x.toml")).toThrow(ThemeImportError);
	});
});
