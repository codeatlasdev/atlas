// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { ThemeImportError } from "./errors.ts";
import { parseItermColors } from "./iterm.ts";

const color = (r: number, g: number, b: number, extra = "") => `<dict>
	<key>Alpha Component</key><real>1</real>
	<key>Blue Component</key><real>${b}</real>
	<key>Color Space</key><string>sRGB</string>
	<key>Green Component</key><real>${g}</real>
	<key>Red Component</key><real>${r}</real>${extra}
</dict>`;

const plist = (body: string) => `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<!-- exported -->
<dict>
${body}
</dict>
</plist>`;

describe("parseItermColors", () => {
	test("maps ansi, bg/fg, cursor and selection", () => {
		const xml = plist(`
<key>Ansi 0 Color</key>${color(0, 0, 0)}
<key>Ansi 1 Color</key>${color(1, 0, 0)}
<key>Ansi 15 Color</key>${color(1, 1, 1)}
<key>Background Color</key>${color(0.0627, 0.0941, 0.1255)}
<key>Foreground Color</key>${color(0.8784, 0.9412, 1.0)}
<key>Cursor Color</key>${color(1, 0.5, 0)}
<key>Cursor Text Color</key>${color(0, 0, 1)}
<key>Selection Color</key>${color(0.1882, 0.251, 0.3137)}
<key>Selected Text Color</key>${color(1, 1, 1)}
<key>Badge Color</key>${color(1, 0, 0)}
<key>Some Flag</key><true/>
<key>Ansi 2 Color</key><dict><key>Red Component</key><real>x</real></dict>`);
		const r = parseItermColors(xml, "My Scheme");
		expect(r.name).toBe("My Scheme");
		expect(r.appearance).toBe("dark");
		expect(r.style["terminal.ansi.black"]).toBe("#000000");
		expect(r.style["terminal.ansi.red"]).toBe("#ff0000");
		expect(r.style["terminal.ansi.bright_white"]).toBe("#ffffff");
		expect(r.style["terminal.background"]).toBe("#101820");
		expect(r.style["terminal.foreground"]).toBe("#e0f0ff");
		expect(r.style["terminal.cursor"]).toBe("#ff8000");
		expect(r.style["terminal.cursor_text"]).toBe("#0000ff");
		expect(r.style["terminal.selection.background"]).toBe("#304050");
		expect(r.style["terminal.selection.foreground"]).toBe("#ffffff");
		expect(r.warnings).toEqual([m.settings_theme_diag_invalid_color_key({ key: "Ansi 2 Color" })]);
	});

	test("light background and integer components", () => {
		const xml = plist(`<key>Background Color</key><dict>
<key>Red Component</key><integer>1</integer><key>Green Component</key><integer>1</integer><key>Blue Component</key><integer>1</integer></dict>`);
		const r = parseItermColors(xml, "Light");
		expect(r.appearance).toBe("light");
		expect(r.style["terminal.background"]).toBe("#ffffff");
	});

	test("garbage throws ThemeImportError", () => {
		expect(() => parseItermColors("not xml", "x")).toThrow(ThemeImportError);
		expect(() => parseItermColors(plist("<key>Foo</key><string>bar</string>"), "x")).toThrow(
			ThemeImportError,
		);
	});
});
