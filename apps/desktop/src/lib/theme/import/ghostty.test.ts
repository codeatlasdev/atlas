// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { ThemeImportError } from "./errors.ts";
import { exportGhostty } from "./export.ts";
import { parseGhosttyConfig } from "./ghostty.ts";

const PALETTE = [
	"#101820",
	"#ff6b6b",
	"#6bff95",
	"#f0e060",
	"#6b9bff",
	"#ff6bd5",
	"#6be5ff",
	"#F8F8F2",
	"#405060",
	"#ff8a8a",
	"#8affb0",
	"#fff080",
	"#8ab4ff",
	"#ff8ae0",
	"#8aefff",
	"#FFFFFF",
];

const REAL_CONFIG = `# my ghostty
${PALETTE.map((c, i) => `palette = ${i}=${c}`).join("\n")}
background = #101820
foreground = #F8F8F2
cursor-color = #F8F8F2
cursor-text = #101820
selection-background = #304050
selection-foreground = #F8F8F2

font-family = Fira Code
font-size = 14
font-thicken = true
window-padding-x = 8
window-padding-y = 4
cursor-style = block
cursor-style-blink = false
mouse-shift-capture = true
keybind = cmd+t=new_tab
`;

const ANSI = [
	"black",
	"red",
	"green",
	"yellow",
	"blue",
	"magenta",
	"cyan",
	"white",
	"bright_black",
	"bright_red",
	"bright_green",
	"bright_yellow",
	"bright_blue",
	"bright_magenta",
	"bright_cyan",
	"bright_white",
];

/** In-memory filesystem for the injected readFile. */
const fsCtx = (files: Record<string, string>, extra = {}) => ({
	home: "/home/u",
	themeDirs: ["/home/u/.config/ghostty/themes", "/app/themes"],
	readFile: async (p: string) => files[p] ?? null,
	...extra,
});

describe("parseGhosttyConfig: real config", () => {
	test("maps every key exactly", async () => {
		const r = await parseGhosttyConfig(REAL_CONFIG, fsCtx({}));
		expect(r.name).toBe("Ghostty: my config");
		expect(r.appearance).toBe("dark");
		expect(r.warnings).toEqual([]);
		ANSI.forEach((n, i) => expect(r.style[`terminal.ansi.${n}`]).toBe(PALETTE[i].toLowerCase()));
		expect(r.style["terminal.background"]).toBe("#101820");
		expect(r.style["terminal.foreground"]).toBe("#f8f8f2");
		expect(r.style["terminal.cursor"]).toBe("#f8f8f2");
		expect(r.style["terminal.cursor_text"]).toBe("#101820");
		expect(r.style["terminal.selection.background"]).toBe("#304050");
		expect(r.style["terminal.selection.foreground"]).toBe("#f8f8f2");
		expect(r.font).toEqual({ family: "Fira Code", size: 14, thicken: true });
		expect(r.terminal).toEqual({ cursor_shape: "block", blinking: false, padding_x: 8, padding_y: 4 });
	});

	test("export -> import round trip is equal", async () => {
		const first = await parseGhosttyConfig(REAL_CONFIG, fsCtx({}));
		const text = exportGhostty(first, first.terminal);
		const second = await parseGhosttyConfig(text, fsCtx({}));
		expect(second.style).toEqual(first.style);
		expect(second.font).toEqual(first.font);
		expect(second.terminal).toEqual(first.terminal);
		expect(second.appearance).toBe(first.appearance);
		expect(second.warnings).toEqual([]);
	});
});

describe("syntax", () => {
	test("whitespace, comments, quotes, duplicates", async () => {
		const r = await parseGhosttyConfig(
			`# c\nbackground=#000000\n  foreground   =   "ffffff"  \nbackground = #111111\nfont-family = "Fira Code"\nfont-family = Menlo\n`,
			fsCtx({}),
		);
		expect(r.style["terminal.background"]).toBe("#111111");
		expect(r.style["terminal.foreground"]).toBe("#ffffff");
		expect(r.font.family).toBe("Fira Code");
	});

	test("palette beyond 15 warns once, 0-15 kept", async () => {
		const r = await parseGhosttyConfig(
			"palette = 16=#112233\npalette = 17=#112233\npalette = 1=#ff0000\n",
			fsCtx({}),
		);
		expect(r.style["terminal.ansi.red"]).toBe("#ff0000");
		expect(r.warnings.filter((w) => w.includes("out of range"))).toHaveLength(1);
		expect(Object.keys(r.style)).toEqual(["terminal.ansi.red"]);
	});

	test("X11 names and bad colors", async () => {
		const r = await parseGhosttyConfig(
			"background = black\nforeground = nonsense\ncursor-color = nonsense\nselection-background = #12\n",
			fsCtx({}),
		);
		expect(r.style["terminal.background"]).toBe("#000000");
		expect(r.style["terminal.foreground"]).toBeUndefined();
		expect(r.warnings).toEqual([
			m.settings_theme_diag_invalid_color({ key: "foreground", value: "nonsense" }),
			m.settings_theme_diag_invalid_color({ key: "cursor-color", value: "nonsense" }),
			m.settings_theme_diag_invalid_color({ key: "selection-background", value: "#12" }),
		]);
	});

	test("terminal prefs", async () => {
		const r = await parseGhosttyConfig(
			"cursor-style = block_hollow\nwindow-padding-x = 6,2\ncopy-on-select = clipboard\nmacos-option-as-alt = left\nbackground = #ffffff\n",
			fsCtx({}),
		);
		expect(r.terminal).toEqual({
			cursor_shape: "hollow",
			padding_x: 6,
			copy_on_select: true,
			option_as_alt: "left",
		});
		expect(r.appearance).toBe("light");
		const t = await parseGhosttyConfig("macos-option-as-alt = true\ncopy-on-select = false\n", fsCtx({}));
		expect(t.terminal).toEqual({ option_as_alt: "both", copy_on_select: false });
	});

	test("nothing usable throws", async () => {
		await expect(parseGhosttyConfig("mouse-shift-capture = true\n", fsCtx({}))).rejects.toBeInstanceOf(
			ThemeImportError,
		);
	});
});

describe("themes", () => {
	const files = {
		"/home/u/.config/ghostty/themes/Dracula":
			"background = #282a36\nforeground = #f8f8f2\npalette = 1=#ff5555\n",
		"/app/themes/Dracula": "background = #000000\n",
		"/app/themes/Solar": "background = #fdf6e3\n",
		"/abs/MyTheme": "background = #101010\npalette = 2=#00ff00\n",
	};

	test("resolves in themeDirs order; config overrides theme", async () => {
		const r = await parseGhosttyConfig("theme = Dracula\nforeground = #ffffff\n", fsCtx(files));
		expect(r.name).toBe("Ghostty: Dracula");
		expect(r.style["terminal.background"]).toBe("#282a36");
		expect(r.style["terminal.foreground"]).toBe("#ffffff");
		expect(r.style["terminal.ansi.red"]).toBe("#ff5555");
	});

	test("config keys listed before `theme =` still override the theme", async () => {
		const r = await parseGhosttyConfig("background = #abcdef\ntheme = Dracula\n", fsCtx(files));
		expect(r.style["terminal.background"]).toBe("#abcdef");
	});

	test("falls through to later dir and absolute path", async () => {
		const solar = await parseGhosttyConfig("theme = Solar\n", fsCtx(files));
		expect(solar.style["terminal.background"]).toBe("#fdf6e3");
		expect(solar.appearance).toBe("light");
		const abs = await parseGhosttyConfig("theme = /abs/MyTheme\n", fsCtx(files));
		expect(abs.name).toBe("Ghostty: MyTheme");
		expect(abs.style["terminal.ansi.green"]).toBe("#00ff00");
	});

	test("light:/dark: picks by appearance, default dark", async () => {
		const cfg = "theme = light:Solar,dark:Dracula\n";
		const dark = await parseGhosttyConfig(cfg, fsCtx(files));
		expect(dark.name).toBe("Ghostty: Dracula");
		const light = await parseGhosttyConfig(cfg, fsCtx(files, { appearance: "light" }));
		expect(light.name).toBe("Ghostty: Solar");
	});

	test("missing theme warns once but still imports config keys", async () => {
		const r = await parseGhosttyConfig("theme = Nope\nbackground = #222222\n", fsCtx(files));
		expect(r.warnings).toEqual(["theme not found: Nope"]);
		expect(r.name).toBe("Ghostty: my config");
		expect(r.style["terminal.background"]).toBe("#222222");
	});
});

describe("config-file", () => {
	test("includes relative to the including file, optional prefix, cycle guard", async () => {
		const files = {
			"/home/u/.config/ghostty/extra":
				"foreground = #aaaaaa\nconfig-file = nested/more\nconfig-file = extra\n",
			"/home/u/.config/ghostty/nested/more": "palette = 3=#010203\nconfig-file = ../extra\n",
		};
		const text = "config-file = extra\nconfig-file = ?missing\nbackground = #000000\n";
		const r = await parseGhosttyConfig(text, fsCtx(files));
		expect(r.style["terminal.foreground"]).toBe("#aaaaaa");
		expect(r.style["terminal.ansi.yellow"]).toBe("#010203");
		expect(r.warnings.some((w) => w.includes("cycle"))).toBe(true);
		expect(r.warnings.some((w) => w.includes("missing"))).toBe(false);
	});

	test("required missing include warns; including file's own keys keep order", async () => {
		const files = { "/home/u/.config/ghostty/a": "background = #111111\n" };
		const r = await parseGhosttyConfig(
			"config-file = a\nbackground = #222222\nconfig-file = b\n",
			fsCtx(files),
		);
		expect(r.style["terminal.background"]).toBe("#222222");
		expect(r.warnings).toEqual(["config-file not found: /home/u/.config/ghostty/b"]);
	});
});
