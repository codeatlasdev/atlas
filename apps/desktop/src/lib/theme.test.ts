// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import atlas from "./themes/atlas.json";
import one from "./themes/one.json";
import {
	ansi16,
	color,
	cssVars,
	parseSelection,
	parseThemeFamily,
	resolveTheme,
	syntaxTokens,
	terminalColors,
	xtermTheme,
} from "./theme.ts";

const family = (themes) => ({ name: "T", author: "a", themes });
const dark = { name: "D", appearance: "dark", style: { text: "#ffffff" } };

describe("parseThemeFamily", () => {
	test("reads bundled families", () => {
		expect(parseThemeFamily(atlas).map((t) => t.name)).toEqual(["Atlas Dark", "Atlas Light"]);
		expect(parseThemeFamily(one).map((t) => t.appearance)).toEqual(["dark", "light"]);
	});

	test("rejects a non-family", () => {
		expect(() => parseThemeFamily(null)).toThrow();
		expect(() => parseThemeFamily({ name: "x" })).toThrow();
	});

	test("skips members with a bad name, appearance or style", () => {
		const bad = [
			{ appearance: "dark", style: {} },
			{ name: "A", appearance: "sepia", style: {} },
			{ name: "B", appearance: "dark" },
			"nope",
			dark,
		];
		expect(parseThemeFamily(family(bad)).map((t) => t.name)).toEqual(["D"]);
	});
});

describe("color", () => {
	test("accepts #rrggbb and #rrggbbaa, lowercases", () => {
		expect(color("#ABCDEF")).toBe("#abcdef");
		expect(color("#abcdef80")).toBe("#abcdef80");
	});

	test("ignores anything else", () => {
		for (const v of ["red", "#abc", "#12345", "rgb(0,0,0)", "url(x)", "#gggggg", 5, null, undefined])
			expect(color(v)).toBeUndefined();
	});
});

describe("cssVars", () => {
	const style = {
		background: "#040507",
		"editor.background": "#0a0b0e",
		text: "#e9ebef",
		"text.muted": "#a6a8ab",
		"text.accent": "#2b80ff",
		border: "#ffffff24",
		"element.hover": "#ffffff13",
		created: "#49c46d",
		players: [{ cursor: "#2b80ff", selection: "#2b80ff3d" }],
		syntax: { keyword: { color: "#2eb8f8", font_style: "italic", font_weight: 700 } },
	};
	const vars = cssVars(style);

	test("maps style keys onto kit tokens", () => {
		expect(vars["--ca-surface-sunken"]).toBe("#040507");
		expect(vars["--ca-surface-base"]).toBe("#0a0b0e");
		expect(vars["--ca-text-primary"]).toBe("#e9ebef");
		expect(vars["--ca-outline-default"]).toBe("#ffffff24");
		expect(vars["--ca-action-hover"]).toBe("#ffffff13");
		expect(vars["--ca-semantic-success"]).toBe("#49c46d");
	});

	test("one key can drive several tokens", () => {
		expect(vars["--ca-text-secondary"]).toBe("#a6a8ab");
		expect(vars["--ca-text-hint"]).toBe("#a6a8ab");
		expect(vars["--ca-brand-main"]).toBe("#2b80ff");
		expect(vars["--ca-brand-text"]).toBe("#2b80ff");
	});

	test("maps editor and syntax vars", () => {
		expect(vars["--cm-caret"]).toBe("#2b80ff");
		expect(vars["--cm-selection"]).toBe("#2b80ff3d");
		expect(vars["--cm-syn-keyword"]).toBe("#2eb8f8");
		expect(vars["--cm-syn-keyword-italic"]).toBe("italic");
		expect(vars["--cm-syn-keyword-weight"]).toBe("700");
	});

	test("missing keys are left out so kit defaults apply", () => {
		expect(vars["--ca-text-disabled"]).toBeUndefined();
		expect(vars["--ca-semantic-error"]).toBeUndefined();
		expect(cssVars({})).toEqual({});
	});

	test("invalid colors are ignored", () => {
		const v = cssVars({ text: "red", background: "#12", "element.hover": "#ffffff13" });
		expect(Object.keys(v)).toEqual(["--ca-action-hover"]);
	});

	test("dotted syntax keys become dashed var names", () => {
		const v = cssVars({ syntax: { "string.special": { color: "#112233" } } });
		expect(v["--cm-syn-string-special"]).toBe("#112233");
	});
});

describe("terminal palette", () => {
	const names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
	const style = {};
	names.forEach((n, i) => {
		style[`terminal.ansi.${n}`] = `#0000${(i + 1).toString(16).padStart(2, "0")}`;
		style[`terminal.ansi.bright_${n}`] = `#0001${(i + 1).toString(16).padStart(2, "0")}`;
	});

	test("ansi16 is normal black..white then bright black..white", () => {
		const a = ansi16(style);
		expect(a).toHaveLength(16);
		expect(a[0]).toBe("#000001");
		expect(a[7]).toBe("#000008");
		expect(a[8]).toBe("#000101");
		expect(a[15]).toBe("#000108");
	});

	test("missing or invalid slots are null (terminal default) and alpha is dropped", () => {
		const a = ansi16({ "terminal.ansi.red": "#11223380", "terminal.ansi.green": "nope" });
		expect(a[1]).toBe("#112233");
		expect(a[2]).toBeNull();
		expect(a[0]).toBeNull();
	});

	test("terminalColors falls back to null fg/bg and uses the player cursor", () => {
		expect(terminalColors({})).toMatchObject({ fg: null, bg: null, cursor: null });
		const t = terminalColors({
			"terminal.foreground": "#aabbcc",
			"terminal.background": "#010203",
			players: [{ cursor: "#ff0000" }],
		});
		expect(t).toMatchObject({ fg: "#aabbcc", bg: "#010203", cursor: "#ff0000" });
	});

	test("xtermTheme uses xterm's camelCase bright names and leaves gaps undefined", () => {
		const x = xtermTheme(style);
		expect(x.black).toBe("#000001");
		expect(x.brightBlack).toBe("#000101");
		expect(x.brightWhite).toBe("#000108");
		expect(xtermTheme({}).background).toBeUndefined();
		expect(xtermTheme({ "terminal.foreground": "#aabbcc" }).cursor).toBe("#aabbcc");
	});
});

describe("syntaxTokens", () => {
	test("keeps color, italic and numeric weight; drops empty or invalid entries", () => {
		const t = syntaxTokens({
			syntax: {
				comment: { color: "#5c6370", font_style: "italic" },
				title: { color: null, font_weight: 700 },
				bad: { color: "blue", font_style: "normal" },
			},
		});
		expect(t.comment).toEqual({ color: "#5c6370", italic: true });
		expect(t.title).toEqual({ weight: "700" });
		expect(t.bad).toBeUndefined();
	});
});

describe("selection", () => {
	test("defaults to system with the Atlas themes", () => {
		expect(parseSelection(undefined)).toEqual({ mode: "system", light: "Atlas Light", dark: "Atlas Dark" });
	});

	test("keeps valid fields and defaults the rest", () => {
		expect(parseSelection({ mode: "dark", light: "One Light", dark: 5 })).toEqual({
			mode: "dark",
			light: "One Light",
			dark: "Atlas Dark",
		});
		expect(parseSelection({ mode: "sepia" }).mode).toBe("system");
	});
});

describe("resolveTheme", () => {
	const themes = [...parseThemeFamily(atlas), ...parseThemeFamily(one)];
	const sel = (o) => parseSelection(o);

	test("system mode follows the OS appearance", () => {
		expect(resolveTheme(themes, sel({}), true).name).toBe("Atlas Dark");
		expect(resolveTheme(themes, sel({}), false).name).toBe("Atlas Light");
	});

	test("fixed mode ignores the OS and uses the chosen name", () => {
		expect(resolveTheme(themes, sel({ mode: "light", light: "One Light" }), true).name).toBe("One Light");
		expect(resolveTheme(themes, sel({ mode: "dark", dark: "One Dark" }), false).name).toBe("One Dark");
	});

	test("an unknown or wrong-appearance name falls back to the Atlas default", () => {
		expect(resolveTheme(themes, sel({ mode: "dark", dark: "Gone" }), false).name).toBe("Atlas Dark");
		expect(resolveTheme(themes, sel({ mode: "dark", dark: "One Light" }), false).name).toBe("Atlas Dark");
	});

	test("without the Atlas defaults, any theme of that appearance is used", () => {
		const only = parseThemeFamily(one);
		expect(resolveTheme(only, sel({ mode: "light" }), true).name).toBe("One Light");
		expect(resolveTheme([], sel({}), true)).toBeUndefined();
	});
});

describe("bundled themes", () => {
	test("Atlas Dark reproduces the kit dark surface and text tokens", () => {
		const v = cssVars(parseThemeFamily(atlas)[0].style);
		expect(v["--ca-surface-base"]).toBe("#0a0b0e");
		expect(v["--ca-text-primary"]).toBe("#e9ebef");
		expect(v["--ca-brand-main"]).toBe("#2b80ff");
	});

	test("every bundled theme defines a full 16-color terminal palette", () => {
		for (const t of [...parseThemeFamily(atlas), ...parseThemeFamily(one)])
			expect(ansi16(t.style).every((c) => c !== null)).toBe(true);
	});
});
