// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import atlas from "../themes/atlas.json";
import dracula from "../themes/dracula.json";
import one from "../themes/one.json";
import { ansi16, cssVars, syntaxTokens, terminalColors } from "../theme.ts";
import { contrast, ensureContrast, fromOklch, over, readableOn, terminalLift, toOklch } from "./color.ts";
import { CODE_PAINTS, codeSurfaces, complete } from "./derive.ts";
import {
	DEFAULT_SELECTION,
	migrateSelection,
	parseFamily,
	resolveTerminalTheme,
	resolveTheme,
	validateStyle,
} from "./schema.ts";
import {
	ANSI_KEYS,
	EDITOR_TOKENS,
	PLAYER_TOKENS,
	SYNTAX_ROLES,
	TERMINAL_KEYS,
	UI_TOKENS,
	allVars,
	synVar,
} from "./tokens.ts";

const families = { atlas, one, dracula };
const bundled = Object.values(families).flatMap((f) => parseFamily(f).themes);
const by = (name) => bundled.find((t) => t.name === name);
const parseAlpha = (h) => (h.length === 9 ? parseInt(h.slice(7, 9), 16) / 255 : 1);

describe("color math", () => {
	test("OKLCH round-trips sRGB hex", () => {
		for (const hex of ["#282a36", "#f8f8f2", "#bd93f9", "#ff5555", "#fffbeb", "#000000", "#ffffff"])
			expect(fromOklch(toOklch(hex))).toBe(hex);
	});

	test("WCAG contrast matches known pairs", () => {
		expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 1);
		expect(contrast("#f8f8f2", "#282a36")).toBeCloseTo(13.36, 1);
		expect(contrast("#6272a4", "#282a36")).toBeCloseTo(3.03, 1);
	});

	test("ensureContrast lifts toward the readable side and keeps passing colors", () => {
		expect(ensureContrast("#f8f8f2", "#282a36", 4.5)).toBe("#f8f8f2");
		expect(contrast(ensureContrast("#6272a4", "#282a36", 4.5), "#282a36")).toBeGreaterThanOrEqual(4.5);
		expect(contrast(ensureContrast("#a0a0a0", "#fffbeb", 4.5), "#fffbeb")).toBeGreaterThanOrEqual(4.5);
	});
});

describe("validation", () => {
	test("bundled families parse without a single issue", () => {
		for (const [name, f] of Object.entries(families))
			expect([name, parseFamily(f).issues]).toEqual([name, []]);
	});

	test("errors name the JSON path", () => {
		const r = parseFamily({
			themes: [
				{ name: "A", appearance: "sepia", style: {} },
				{ name: "B", appearance: "dark" },
			],
		});
		expect(r.themes).toEqual([]);
		expect(r.issues.map((i) => i.path)).toEqual(["$.themes[0].appearance", "$.themes[1].style"]);
	});

	test("bad colors, roles and unknown keys are warnings, not failures", () => {
		const issues = validateStyle(
			{ text: "red", "made.up": "#ffffff", syntax: { nope: { color: "#ffffff" }, keyword: { color: "x" } } },
			"s",
		);
		expect(issues.every((i) => i.level === "warn")).toBe(true);
		expect(issues.map((i) => i.path).sort()).toEqual([
			"s.made.up",
			"s.syntax.keyword.color",
			"s.syntax.nope",
			"s.text",
		]);
	});

	test("a newer file version warns, a font block is validated", () => {
		const r = parseFamily({
			themes: [
				{
					name: "A",
					appearance: "dark",
					style: {},
					atlas: { version: 9, font: { family: "Fira Code", size: 99 } },
				},
			],
		});
		expect(r.themes[0].font).toEqual({ family: "Fira Code" });
		expect(r.issues.map((i) => i.path)).toEqual(["$.themes[0].atlas.version", "$.themes[0].atlas.font.size"]);
	});
});

describe("completeness: every token the app reads exists in every theme", () => {
	const required = [...Object.keys(UI_TOKENS), ...Object.keys(EDITOR_TOKENS), ...TERMINAL_KEYS, ...ANSI_KEYS];

	for (const t of bundled)
		test(`${t.name}: all registry tokens resolve after complete()`, () => {
			const s = complete(t.style, t.appearance);
			const missing = required.filter((k) => !/^#[0-9a-f]{6}([0-9a-f]{2})?$/.test(s[k] ?? ""));
			expect(missing).toEqual([]);
			for (const k of Object.keys(PLAYER_TOKENS)) expect(s.players[0][k]).toMatch(/^#/);
			for (const r of SYNTAX_ROLES) expect([r, s.syntax[r]?.color]).toEqual([r, expect.stringMatching(/^#/)]);
			const vars = cssVars(s);
			expect(
				allVars().filter((v) => !v.endsWith("-italic") && !v.endsWith("-weight") && !(v in vars)),
			).toEqual([]);
		});

	test("a theme with only three colors becomes complete", () => {
		const s = complete(
			{ "terminal.background": "#101820", "terminal.foreground": "#dddddd", "text.accent": "#ff8800" },
			"dark",
		);
		for (const k of required) expect([k, typeof s[k]]).toEqual([k, "string"]);
		expect(s["editor.background"]).toBe("#101820");
		expect(s["text.accent"]).toBe("#ff8800");
	});

	test("an empty light theme is derived light, with AA text", () => {
		const s = complete({}, "light");
		expect(s["editor.background"]).toBe("#ffffff");
		for (const k of ["text", "text.muted", "text.accent", "created", "warning", "info", "error"])
			expect([k, contrast(s[k].slice(0, 7), s["elevated_surface.background"])]).toEqual([
				k,
				expect.any(Number),
			]);
		expect(contrast(s.text, s["editor.background"])).toBeGreaterThanOrEqual(7);
		expect(contrast(s["text.muted"], s["elevated_surface.background"])).toBeGreaterThanOrEqual(4.5);
	});

	test("explicit values that read are never overwritten; ones that do not are lifted, keeping their hue", () => {
		const s = complete(
			{ text: "#d0e0f0", syntax: { keyword: { color: "#abcdef", font_style: "italic" } } },
			"dark",
		);
		expect(s.text).toBe("#d0e0f0");
		// the syntax colour reads on the editor but not on the derived selection: lifted (lightness only, hue kept),
		// the font style is the theme's own
		expect(s.syntax.keyword.font_style).toBe("italic");
		expect(Math.abs(toOklch(s.syntax.keyword.color).h - toOklch("#abcdef").h)).toBeLessThan(6);
		for (const bg of codeSurfaces(s))
			expect(contrast(s.syntax.keyword.color, bg)).toBeGreaterThanOrEqual(4.5);
		// Incident: an explicit grey that vanished on its own surface (light theme sidebar) used to be kept as is.
		const bad = complete(
			{ "editor.background": "#fafafa", text: "#383a42", "text.muted": "#c8c8c8" },
			"light",
		);
		expect(bad["text.muted"]).not.toBe("#c8c8c8");
		expect(contrast(bad["text.muted"], bad.background)).toBeGreaterThanOrEqual(4.5);
		expect(Math.abs(toOklch(bad["text.muted"]).h - toOklch("#c8c8c8").h)).toBeLessThan(40);
	});

	test("syntax derived from an ANSI-only palette follows the classic mapping", () => {
		const s = complete(
			{
				"terminal.background": "#000000",
				"terminal.ansi.magenta": "#ff00ff",
				"terminal.ansi.green": "#00ff00",
				"terminal.ansi.yellow": "#ffff00",
				"terminal.ansi.cyan": "#00ffff",
			},
			"dark",
		);
		// Classic mapping by hue; lightness is lifted where the derived selection/search paints would not read
		// (full-chroma #ff00ff on the derived selection is 3.9:1): the hue stays.
		for (const [role, ansi] of [
			["keyword", "#ff00ff"],
			["function", "#00ff00"],
			["string", "#ffff00"],
			["type", "#00ffff"],
		]) {
			expect(Math.abs(toOklch(s.syntax[role].color).h - toOklch(ansi).h)).toBeLessThan(6);
			for (const bg of codeSurfaces(s))
				expect(contrast(s.syntax[role].color, bg)).toBeGreaterThanOrEqual(4.5);
		}
	});
});

/**
 * Incident (user, live): "the contrast in the light theme is really bad", the sidebar text nearly invisible. Every text
 * token must read on EVERY surface it can sit on, in every bundled theme. Rules (WCAG 2.x):
 *  - body text and labels (`text`, `text.muted`, `text.accent`, the semantic colours as text): >= 4.5
 *  - `text.disabled` and `editor.line_number` (placeholders, disabled controls, gutter numbers): >= 3, the documented
 *    relaxation: WCAG exempts them, we still require them to read
 *  - the keyboard focus ring (`border.selected`) and non-text state indicators: >= 3
 *  - `text.on_accent` on the accent fill: >= 4.5
 */
describe("contrast: every text token on every surface, every theme", () => {
	const SURFACES = ["background", "editor.background", "surface.background", "elevated_surface.background"];
	const AA_TEXT = [
		"text",
		"text.muted",
		"text.accent",
		"created",
		"warning",
		"info",
		"error",
		"link_text.hover",
	];
	const READABLE = ["text.disabled", "editor.line_number"];
	const INDICATORS = ["border.selected", "state.success", "state.warning", "state.info", "state.error"];
	const rgb = (hex) => hex.slice(0, 7);

	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		const fails = [];
		const check = (token, min, surfaces = SURFACES) => {
			for (const surf of surfaces) {
				const r = contrast(rgb(s[token]), rgb(s[surf]));
				if (r < min) fails.push(`${token} on ${surf}: ${r.toFixed(2)} < ${min}`);
			}
		};
		test(`${t.name} (${t.appearance}): text >= 4.5, disabled/placeholder >= 3, indicators >= 3`, () => {
			for (const k of AA_TEXT) check(k, 4.5);
			for (const k of READABLE) check(k, 3, k === "editor.line_number" ? ["editor.background"] : SURFACES);
			for (const k of INDICATORS) check(k, 3);
			check("text.on_accent", 4.5, []);
			const onAccent = contrast(rgb(s["text.on_accent"]), rgb(s["text.accent"]));
			if (onAccent < 4.5) fails.push(`text.on_accent on text.accent: ${onAccent.toFixed(2)} < 4.5`);
			expect(fails).toEqual([]);
		});
	}

	// The sidebar lets 18% of the native material through. The window material follows the theme's appearance
	// (`colorScheme` → `setTheme`), so the extreme that can show through is white under a light theme and black under
	// a dark one; the text must pass over that blend.
	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		test(`${t.name}: sidebar text survives the native material showing through (18%)`, () => {
			const fails = [];
			const side = over(rgb(s.background), t.appearance === "dark" ? "#000000" : "#ffffff", 0.82);
			for (const k of ["text", "text.muted", "text.accent"]) {
				const r = contrast(rgb(s[k]), side);
				if (r < 4.5) fails.push(`${k} over sidebar: ${r.toFixed(2)}`);
			}
			expect(fails).toEqual([]);
		});
	}
});

describe("contrast: text on tinted controls (kbd chips, hover and selected rows)", () => {
	// Incident (measured by `atlas desktop debug contrast`, One Light): the ⌘K chip is `color-mix(text 6%, transparent)`
	// over the sidebar, which dropped `text.muted` from 4.6 to 4.33. Muted text must also pass on its own tints.
	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		test(`${t.name}: text and text.muted pass on a 6% text tint and on the selected-row tint, over every surface`, () => {
			const fails = [];
			for (const surf of [
				"background",
				"editor.background",
				"surface.background",
				"elevated_surface.background",
			])
				for (const [name, a] of [
					["chip 6%", 0.06],
					["selected", t.appearance === "dark" ? 0.13 : 0.09],
				]) {
					const bg = over(s.text.slice(0, 7), s[surf].slice(0, 7), a);
					for (const k of ["text", "text.muted"]) {
						const r = contrast(s[k].slice(0, 7), bg);
						if (r < 4.5) fails.push(`${k} on ${name} over ${surf}: ${r.toFixed(2)}`);
					}
				}
			expect(fails).toEqual([]);
		});
	}
});

describe("contrast: text on accent-tinted rows (selected palette row, current project)", () => {
	// Incident (measured): the palette's selected row is `brand-main 16%`, the sidebar's current/selected project the
	// space accent at 16%. Text and muted text read 3.75-4.4:1 on them in One Dark/One Light/Dracula.
	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		test(`${t.name}: text and text.muted >= 4.5 on a 16% accent tint over every surface, for every space hue`, () => {
			const fails = [];
			const dark = t.appearance === "dark";
			const tints = [s["text.accent"].slice(0, 7)];
			for (let h = 0; h < 360; h += 15)
				tints.push(fromOklch({ l: dark ? 0.74 : 0.62, c: dark ? 0.14 : 0.17, h }));
			for (const surf of [
				"background",
				"editor.background",
				"surface.background",
				"elevated_surface.background",
			])
				for (const tint of tints) {
					const bg = over(tint, s[surf].slice(0, 7), 0.16);
					for (const k of ["text", "text.muted"]) {
						const r = contrast(s[k].slice(0, 7), bg);
						if (r < 4.5) fails.push(`${k} on 16% ${tint} over ${surf}: ${r.toFixed(2)}`);
					}
				}
			expect(fails).toEqual([]);
		});
	}
});

describe("contrast: text on the kit's hover/active fill, alone and under a kbd chip", () => {
	// Incident (measured, One Dark): `--ca-vibrancy-fill` (near-white at 10% in dark themes, near-black in light ones) is
	// the hover/active row fill; the kbd chip (text 6%) and the current-project accent (16%) stack on top of it.
	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		test(`${t.name}: text and text.muted >= 4.5 on a 12% vibrancy fill, alone and with a 6% chip on top`, () => {
			const dark = t.appearance === "dark";
			const fill = dark ? "#ffffff" : "#000000";
			const fails = [];
			const accents = [s["text.accent"].slice(0, 7)];
			for (let h = 0; h < 360; h += 15)
				accents.push(fromOklch({ l: dark ? 0.74 : 0.62, c: dark ? 0.14 : 0.17, h }));
			for (const surf of [
				"background",
				"editor.background",
				"surface.background",
				"elevated_surface.background",
			]) {
				const base = over(fill, s[surf].slice(0, 7), 0.12);
				const stacks = [
					["fill", base],
					["fill+chip", over(s.text.slice(0, 7), base, 0.06)],
				];
				for (const [name, bg] of stacks)
					for (const k of ["text", "text.muted"]) {
						const r = contrast(s[k].slice(0, 7), bg);
						if (r < 4.5) fails.push(`${k} on ${name} over ${surf}: ${r.toFixed(2)}`);
					}
			}
			expect(fails).toEqual([]);
		});
	}
});

describe("contrast: semantic colours on their own tint (status chips, tags, banners)", () => {
	// Incident (measured, `atlas desktop debug contrast`): update-channel tags and release chips set `color: var(--tone)`
	// on `color-mix(var(--tone) 14%, transparent)`; the tint of the colour itself cost 0.3-1.0 of contrast (3.7-4.5:1).
	const TINTS = [0.1, 0.14];
	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		test(`${t.name}: text.accent/created/warning/info/error >= 4.5 on their own 10% and 14% tint over every surface`, () => {
			const fails = [];
			for (const k of ["text.accent", "created", "warning", "info", "error"])
				for (const surf of [
					"background",
					"editor.background",
					"surface.background",
					"elevated_surface.background",
				])
					for (const a of TINTS) {
						const bg = over(s[k].slice(0, 7), s[surf].slice(0, 7), a);
						const r = contrast(s[k].slice(0, 7), bg);
						if (r < 4.5) fails.push(`${k} on its ${a * 100}% tint over ${surf}: ${r.toFixed(2)}`);
					}
			expect(fails).toEqual([]);
		});
	}
});

/**
 * Syntax colours are text: every role reaches 4.5 on the editor background AND on every tint the editor paints under
 * code (active line, selection focused and unfocused, selection match, bracket match, search matches). Measured
 * before the rule: 21 roles under 4.5 across the six bundled themes (comments 1.5-2.5, One Light string 2.3).
 * The rule moves lightness only (hue and chroma stay), like the UI text tokens; `terminal.ansi.*` is never touched.
 */
describe("contrast: syntax roles are text (>= 4.5 on the editor and every tint under code)", () => {
	/**
	 * Everything `cm.ts` paints under a token: each base (editor, active line, selection...) alone, and with each
	 * transient paint (selection match, bracket match, search match, active search match) over it, and with every PAIR of
	 * paints (a search match inside a highlighted symbol: both are painted, the stack measured 3.1:1 in One Dark).
	 */
	const under = (s) => {
		const bases = codeSurfaces(s);
		const paint =
			([k, scale]) =>
			(c) =>
				over(s[k], c, parseAlpha(s[k]) * scale);
		const tints = Object.fromEntries(bases.map((c, i) => [`base ${i}`, c]));
		for (const [bi, base] of bases.entries())
			for (const [i, p] of CODE_PAINTS.entries()) {
				tints[`base ${bi} + ${CODE_PAINTS[i][0]}`] = paint(p)(base);
				for (const [j, q] of CODE_PAINTS.entries())
					if (j > i)
						tints[`base ${bi} + ${CODE_PAINTS[i][0]} + ${CODE_PAINTS[j][0]}`] = paint(q)(paint(p)(base));
			}
		return tints;
	};
	for (const t of bundled)
		test(`${t.name}: every syntax role >= 4.5 on every surface under code, hue kept`, () => {
			const s = complete(t.style, t.appearance);
			const fails = [];
			for (const [role, v] of Object.entries(s.syntax))
				for (const [surf, bg] of Object.entries(under(s))) {
					const r = contrast(v.color.slice(0, 7), bg);
					if (r < 4.5) fails.push(`${role} ${v.color} on ${surf}: ${r.toFixed(2)}`);
				}
			for (const [surf, bg] of Object.entries(under(s)))
				if (contrast(s.text.slice(0, 7), bg) < 4.5) fails.push(`text on ${surf}`);
			expect(fails).toEqual([]);
		});

	test("lifting moves lightness only: hue and chroma of a role that needed it are kept", () => {
		for (const t of bundled) {
			const raw = t.style.syntax ?? {};
			const s = complete(t.style, t.appearance);
			for (const [role, v] of Object.entries(raw)) {
				if (!v?.color || !/^#[0-9a-f]{6}$/i.test(v.color)) continue;
				const before = toOklch(v.color);
				const after = toOklch(s.syntax[role].color);
				if (before.c < 0.03) continue; // grey: hue is meaningless
				const dh = Math.abs(((after.h - before.h + 540) % 360) - 180);
				expect([t.name, role, dh < 6]).toEqual([t.name, role, true]);
			}
		}
	});

	test("a role that already reads is returned byte for byte", () => {
		const s = complete(by("Dracula").style, "dark");
		expect(s.syntax.function.color).toBe("#50fa7b");
		expect(s.syntax.string.color).toBe("#f1fa8c");
		expect(s.syntax.variable.color).toBe("#f8f8f2");
	});

	test("the terminal ANSI table is never touched by the syntax rule (spec palette stays)", () => {
		for (const t of bundled) {
			const s = complete(t.style, t.appearance);
			for (const k of Object.keys(t.style).filter((k) => k.startsWith("terminal.ansi.")))
				expect([t.name, k, s[k]]).toEqual([t.name, k, t.style[k]]);
		}
	});
});

/**
 * WCAG 2.2 1.4.11 (non-text contrast): the focus ring, state indicators and icons need 3:1 against the colours NEXT
 * to them, not only against the base surface: the hover/selected fill (`element.*`, the kit's vibrancy fill), a
 * selected row (accent 16%) and the editor's active line / selection. Disabled controls are exempt from AA by WCAG
 * but must be visible: `text.disabled` >= 3 on every chrome surface (asserted above).
 */
describe("contrast: non-text UI (focus ring, indicators, icons) against the fills next to them", () => {
	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		test(`${t.name}: focus ring and state indicators >= 3 on hover/selected fills, accent tint and code surfaces`, () => {
			const surfaces = [
				"background",
				"editor.background",
				"surface.background",
				"elevated_surface.background",
			];
			const fill = t.appearance === "dark" ? "#ffffff" : "#000000";
			const fails = [];
			for (const k of ["border.selected", "state.success", "state.warning", "state.info", "state.error"])
				for (const surf of surfaces) {
					const base = s[surf].slice(0, 7);
					const next = {
						"element.hover": over(s["element.hover"].slice(0, 7), base, parseAlpha(s["element.hover"])),
						"element.selected": over(
							s["element.selected"].slice(0, 7),
							base,
							parseAlpha(s["element.selected"]),
						),
						"vibrancy fill 12%": over(fill, base, 0.12),
						"accent 16%": over(s["text.accent"].slice(0, 7), base, 0.16),
					};
					for (const [name, bg] of Object.entries(next)) {
						const r = contrast(s[k].slice(0, 7), bg);
						if (r < 3) fails.push(`${k} on ${name} over ${surf}: ${r.toFixed(2)}`);
					}
				}
			for (const bg of codeSurfaces(s))
				if (contrast(s["border.selected"].slice(0, 7), bg) < 3)
					fails.push(`focus ring on code surface ${bg}`);
			expect(fails).toEqual([]);
		});
	}
});

/**
 * WCAG 2.2 1.4.11: a text field/select must be told apart from its surface at 3:1, by its border or its fill. The app's
 * fields wore `border.variant` (8% of the text colour, ~1.2:1). `border.control` is the field boundary: derived (a theme
 * need not ship it), >= 3:1 on every surface a field sits on, the sidebar over the native material included.
 */
describe("border.control: the boundary of a field/control", () => {
	for (const t of bundled)
		test(`${t.name}: >= 3:1 on every chrome surface and on the 5% field tint over each; hue of the text colour`, () => {
			const s = complete(t.style, t.appearance);
			expect(s["border.control"]).toMatch(/^#[0-9a-f]{6}$/);
			const fails = [];
			for (const k of [
				"background",
				"editor.background",
				"surface.background",
				"elevated_surface.background",
			]) {
				const bg = s[k].slice(0, 7);
				for (const [name, under] of [
					["surface", bg],
					["field tint 5%", over(s.text.slice(0, 7), bg, 0.05)],
				]) {
					const r = contrast(s["border.control"], under);
					if (r < 3) fails.push(`${name} over ${k}: ${r.toFixed(2)}`);
				}
			}
			expect(fails).toEqual([]);
		});
	test("a theme that ships border.control keeps it when it already reads, lifts it when it does not", () => {
		const ok = complete(
			{ "editor.background": "#ffffff", text: "#111111", "border.control": "#555555" },
			"light",
		);
		expect(ok["border.control"]).toBe("#555555");
		const bad = complete(
			{ "editor.background": "#ffffff", text: "#111111", "border.control": "#eeeeee" },
			"light",
		);
		expect(bad["border.control"]).not.toBe("#eeeeee");
		expect(contrast(bad["border.control"], "#ffffff")).toBeGreaterThanOrEqual(3);
	});
});

/**
 * Kit-owned surfaces take their colours from tokens the theme must provide (measured by the runtime audit): the white
 * label of a destructive button on the (lifted, light) error colour was 1.8:1 in Dracula; the glass tooltip was 2.5:1
 * on a light theme; the glass menu over One Light composited to 4.08:1.
 */
describe("kit surfaces: destructive button, tooltip, overlay", () => {
	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		test(`${t.name}: error.on_fill reads 4.5 on the error fill; tooltip and overlay pairs read 4.5`, () => {
			expect(contrast(s["error.on_fill"].slice(0, 7), s.error.slice(0, 7))).toBeGreaterThanOrEqual(4.5);
			expect(
				contrast(s["tooltip.foreground"].slice(0, 7), s["tooltip.background"].slice(0, 7)),
			).toBeGreaterThanOrEqual(7);
			// the overlay is OPAQUE: nothing behind it changes what text on it reads
			expect(s["elevated_surface.background"]).toMatch(/^#[0-9a-f]{6}$/);
			expect(
				contrast(s.text.slice(0, 7), s["elevated_surface.background"].slice(0, 7)),
			).toBeGreaterThanOrEqual(7);
		});
	}
});

/**
 * `@codeatlasdev/ui` 2.2+ draws every state from named tokens and puts component CSS in `@layer ca.components`, so an
 * unlayered app token wins with no doubled class. Incident: app.css doubled the kit's selectors (`.ca-btn.ca-btn:disabled`,
 * `.ca-tf__wrap.ca-tf__wrap`, `.glass-overlay.glass-overlay`, …) to patch colours the kit now reads from tokens, so the
 * theme must set the KIT's token names (the derived colours the runtime audit measured), not only `--atlas-*` copies.
 */
describe("kit tokens: the theme drives the kit's own contrast tokens", () => {
	const KIT_VARS = {
		"border.control": "--ca-outline-control",
		"error.on_fill": "--ca-semantic-error-contrast",
		"tooltip.background": "--ca-tooltip-bg",
		"overlay.background": "--ca-overlay-bg",
		"border.selected": "--ca-focus-ring",
	};
	for (const t of bundled) {
		test(`${t.name}: cssVars sets each kit token to the derived colour`, () => {
			const s = complete(t.style, t.appearance);
			const vars = cssVars(s);
			for (const [key, kitVar] of Object.entries(KIT_VARS))
				expect([kitVar, vars[kitVar]]).toEqual([kitVar, s[key]]);
			// a disabled control is filled with the chrome ground: its 3:1 label never sits on the red/brand fill
			expect(vars["--ca-action-disabled-bg"]).toBe(s.background);
			expect(vars["--ca-action-disabled-bg"]).toBe(vars["--ca-surface-sunken"]);
		});
	}
});

describe("terminalLift (the terminal's minimum-contrast rule, for previews)", () => {
	test("returns fg untouched when it reads or the rule is off; else lifts toward white on dark and black on light", () => {
		expect(terminalLift("#f8f8f2", "#282a36", 4.5)).toBe("#f8f8f2");
		expect(terminalLift("#6272a4", "#282a36", 1)).toBe("#6272a4");
		const dark = terminalLift("#6272a4", "#282a36", 4.5);
		expect(dark).not.toBe("#6272a4");
		expect(contrast(dark, "#282a36")).toBeGreaterThanOrEqual(4.5);
		const light = terminalLift("#1f8a4c", "#eef0f4", 4.5);
		expect(contrast(light, "#eef0f4")).toBeGreaterThanOrEqual(4.5);
	});
	test("every ANSI colour of every bundled theme reads 4.5 on the terminal background after the lift", () => {
		for (const t of bundled) {
			const s = complete(t.style, t.appearance);
			const bg = s["terminal.background"].slice(0, 7);
			for (const k of Object.keys(s).filter((k) => k.startsWith("terminal.ansi.")))
				expect([t.name, k, contrast(terminalLift(s[k].slice(0, 7), bg, 4.5), bg) >= 4.5]).toEqual([
					t.name,
					k,
					true,
				]);
		}
	});
});

describe("readableOn", () => {
	test("the better of white and black on any fill: >= 4.58 always (the worst fill, mid grey #767676, is 4.54:1 with white)", () => {
		for (const bg of [
			"#000000",
			"#ffffff",
			"#767676",
			"#777777",
			"#ff0000",
			"#00ff00",
			"#ffff00",
			"#0000ff",
			"#8b5cf6",
		])
			expect([bg, contrast(readableOn(bg), bg) >= 4.5]).toEqual([bg, true]);
	});
	test("a light fill gets black, a dark fill white", () => {
		expect(readableOn("#ffff00")).toBe("#000000");
		expect(readableOn("#1f2937")).toBe("#ffffff");
	});
});

describe("bundled theme files ship what the user sees", () => {
	// What the file says must be what is applied: a UI colour the legibility pass lifts at runtime would make the
	// theme preview, `export` and the JSON on disk lie about the theme.
	for (const t of bundled)
		test(`${t.name}: no UI colour in the file is changed by complete()`, () => {
			const s = complete(t.style, t.appearance);
			const changed = Object.keys(t.style).filter(
				(k) =>
					!k.startsWith("terminal") &&
					typeof t.style[k] === "string" &&
					/^#[0-9a-f]{6}$/i.test(t.style[k]) &&
					s[k].toLowerCase() !== t.style[k].toLowerCase(),
			);
			expect(changed).toEqual([]);
		});
});

/** The official spec: https://draculatheme.com/spec */
describe("Dracula spec", () => {
	const d = by("Dracula").style;
	const a = by("Alucard").style;

	test("palette tokens (Dracula Classic)", () => {
		expect(d["editor.background"]).toBe("#282a36");
		expect(d["terminal.background"]).toBe("#282a36");
		expect(d["text"]).toBe("#f8f8f2");
		expect(d["terminal.selection.background"]).toBe("#44475a");
		expect(d["editor.line_number"]).toBe("#6272a4");
		expect(d["background"]).toBe("#21222c");
		expect(d["surface.background"]).toBe("#343746");
		expect(d.players[0].selection).toBe("#44475a");
	});

	test("ANSI table (Dracula Classic)", () => {
		expect(ansi16(d)).toEqual([
			"#21222c",
			"#ff5555",
			"#50fa7b",
			"#f1fa8c",
			"#bd93f9",
			"#ff79c6",
			"#8be9fd",
			"#f8f8f2",
			"#6272a4",
			"#ff6e6e",
			"#69ff94",
			"#ffffa5",
			"#d6acff",
			"#ff92df",
			"#a4ffff",
			"#ffffff",
		]);
	});

	test("syntax roles follow the spec scope mapping", () => {
		const t = syntaxTokens(d);
		expect(t.keyword.color).toBe("#ff79c6");
		expect(t.function.color).toBe("#50fa7b");
		expect(t.string.color).toBe("#f1fa8c");
		expect(t.number.color).toBe("#bd93f9");
		expect(t.boolean.color).toBe("#bd93f9");
		expect(t.comment).toMatchObject({ color: "#6272a4", italic: true });
		expect(t.type).toMatchObject({ color: "#8be9fd", italic: true });
		expect(t["variable.parameter"]).toMatchObject({ color: "#ffb86c", italic: true });
		expect(t["variable.special"]).toMatchObject({ color: "#bd93f9", italic: true });
		expect(t["type.parameter"]).toMatchObject({ color: "#ffb86c", italic: true });
		expect(t.tag.color).toBe("#ff79c6");
		expect(t.attribute.color).toBe("#50fa7b");
		expect(t.variable.color).toBe("#f8f8f2");
	});

	test("palette semantics: errors red, warnings orange, success green, info cyan", () => {
		expect(d.created).toBe("#50fa7b");
		expect(d.warning).toBe("#ffb86c");
		expect(d.info).toBe("#8be9fd");
		// UI text token is the spec red lifted to AA on the chrome surfaces; terminal/editor keep the spec red.
		expect(d["terminal.ansi.red"]).toBe("#ff5555");
		expect(contrast(d.error, d["surface.background"])).toBeGreaterThanOrEqual(4.5);
	});

	test("palette tokens (Alucard Classic)", () => {
		expect(a["editor.background"]).toBe("#fffbeb");
		expect(a.text).toBe("#1f1f1f");
		expect(a["terminal.selection.background"]).toBe("#cfcfde");
		expect(a["editor.line_number"]).toBe("#6c664b");
		expect(ansi16(a)).toEqual([
			"#fffbeb",
			"#cb3a2a",
			"#14710a",
			"#846e15",
			"#644ac9",
			"#a3144d",
			"#036a96",
			"#1f1f1f",
			"#6c664b",
			"#d74c3d",
			"#198d0c",
			"#9e841a",
			"#7862d0",
			"#bf185a",
			"#047fb4",
			"#2c2b31",
		]);
		const t = syntaxTokens(a);
		expect(t.keyword.color).toBe("#a3144d");
		expect(t.function.color).toBe("#14710a");
		expect(t.string.color).toBe("#846e15");
		expect(t.number.color).toBe("#644ac9");
		expect(t.type.color).toBe("#036a96");
		expect(t.comment.color).toBe("#6c664b");
	});

	// Documented deviation (THIRD_PARTY_NOTICES.md, docs/desktop.md): the FILES keep the spec hexes and the terminal
	// ANSI table is untouched; the editor's syntax roles are what `complete()` applies: the spec colour when it reads
	// 4.5:1 on every surface under code, else the same hue and chroma lifted in lightness until it does.
	test("applied syntax: spec colour when it reads, same hue lifted when it does not (Dracula, Alucard)", () => {
		for (const t of [by("Dracula"), by("Alucard")]) {
			const applied = syntaxTokens(complete(t.style, t.appearance));
			const surfaces = codeSurfaces(complete(t.style, t.appearance));
			for (const [role, spec] of Object.entries(t.style.syntax)) {
				const reads = surfaces.every((bg) => contrast(spec.color.slice(0, 7), bg) >= 4.5);
				if (reads) expect([t.name, role, applied[role].color]).toEqual([t.name, role, spec.color]);
				else {
					expect(applied[role].color).not.toBe(spec.color);
					expect(Math.abs(toOklch(applied[role].color).h - toOklch(spec.color).h)).toBeLessThan(6);
				}
				expect(!!applied[role].italic).toBe(spec.font_style === "italic");
			}
		}
	});

	test("Dracula keeps keyword/function/string/type/variable colours that already read", () => {
		const t = syntaxTokens(complete(d, "dark"));
		expect(t.function.color).toBe("#50fa7b");
		expect(t.string.color).toBe("#f1fa8c");
		expect(t.type.color).toBe("#8be9fd");
		expect(t.variable.color).toBe("#f8f8f2");
	});

	test("both themes carry the spec URL and MIT attribution", () => {
		expect(dracula.author).toContain("https://draculatheme.com/spec");
		expect(dracula.author).toContain("MIT");
		expect(dracula.$comment).toContain("https://draculatheme.com/spec");
	});

	test("body text and labels reach WCAG AA on the editor and chrome surfaces", () => {
		for (const t of [by("Dracula"), by("Alucard")]) {
			const s = complete(t.style, t.appearance);
			for (const surface of ["editor.background", "background", "surface.background"])
				for (const k of ["text", "text.muted", "text.accent", "created", "warning", "info", "error"])
					expect([t.name, k, surface, contrast(s[k].slice(0, 7), s[surface].slice(0, 7)) >= 4.5]).toEqual([
						t.name,
						k,
						surface,
						true,
					]);
		}
	});
});

describe("default-selection migration", () => {
	test("nothing stored → defaults, not explicit", () => {
		expect(migrateSelection(undefined)).toEqual({ selection: DEFAULT_SELECTION, explicit: false });
		expect(migrateSelection("junk").explicit).toBe(false);
	});

	test("a pre-v2 file naming exactly the Atlas pair was never a choice: it is the default", () => {
		const m = migrateSelection({ mode: "system", light: "Atlas Light", dark: "Atlas Dark" });
		expect(m.explicit).toBe(false);
		expect(m.selection).toMatchObject({ light: "Atlas Light", dark: "Atlas Dark", mode: "system" });
	});

	test("a legacy selection that differs in any field is the user's choice and is kept", () => {
		const m = migrateSelection({ mode: "system", light: "Atlas Light", dark: "One Dark" });
		expect(m).toMatchObject({ explicit: true });
		expect(m.selection).toMatchObject({ dark: "One Dark", light: "Atlas Light" });
		expect(migrateSelection({ mode: "dark", light: "Atlas Light", dark: "Atlas Dark" }).explicit).toBe(true);
	});

	test("a partial legacy selection keeps the old default for the missing slot", () => {
		const m = migrateSelection({ dark: "One Dark" });
		expect(m.explicit).toBe(true);
		expect(m.selection).toMatchObject({ dark: "One Dark", light: "Atlas Light", mode: "system" });
	});

	test("a v2 selection is always explicit, even when it names Atlas", () => {
		const m = migrateSelection({
			v: 2,
			mode: "system",
			light: "Atlas Light",
			dark: "Atlas Dark",
			terminal: "My Terminal Theme",
		});
		expect(m.explicit).toBe(true);
		expect(m.selection).toMatchObject({ dark: "Atlas Dark", terminal: "My Terminal Theme" });
	});

	test("synVar maps dotted roles to dashed vars", () => {
		expect(synVar("variable.parameter")).toBe("--cm-syn-variable-parameter");
	});
});

describe("a stored theme that no longer exists", () => {
	const themes = bundled;
	const sel = (o) => migrateSelection({ v: 2, ...o }).selection;

	test("a missing app theme resolves to the default of that appearance, silently", () => {
		expect(resolveTheme(themes, sel({ mode: "dark", dark: "Some Paid Theme" }), true).name).toBe(
			"Atlas Dark",
		);
		expect(resolveTheme(themes, sel({ mode: "light", light: "Some Paid Theme" }), false).name).toBe(
			"Atlas Light",
		);
		expect(resolveTheme(themes, sel({ mode: "system", dark: "Gone" }), true).name).toBe("Atlas Dark");
	});

	test("a missing terminal theme means the app theme", () => {
		const app = by("Atlas Dark");
		expect(resolveTerminalTheme(themes, "Some Paid Theme", app)).toBe(app);
		expect(resolveTerminalTheme(themes, "app", app)).toBe(app);
		expect(resolveTerminalTheme(themes, "One Dark", app).name).toBe("One Dark");
	});
});

// Incident (explorer, gitignored files): the first cut painted ignored names with `text.muted`. That token is lifted to
// AA, so in One Dark it measured rgb(204,213,228) against a clean file's rgb(205,212,225): brighter, in effect not dimmed
// at all, and in Dracula barely. A dimmed state needs its own token: readable (AA) yet strictly quieter than `text`.
describe("version_control.ignored: dimmed like VS Code / Zed, still AA, in every bundled theme", () => {
	const SURF = ["background", "editor.background", "surface.background", "elevated_surface.background"];
	const rgb = (hex) => hex.slice(0, 7);
	for (const t of bundled) {
		const s = complete(t.style, t.appearance);
		test(`${t.name} (${t.appearance}): AA on every surface and the sidebar, clearly quieter than text, not a vcs hue`, () => {
			const ign = rgb(s["version_control.ignored"]);
			const side = over(rgb(s.background), t.appearance === "dark" ? "#000000" : "#ffffff", 0.82);
			const fails = [];
			for (const k of SURF) {
				const r = contrast(ign, rgb(s[k]));
				if (r < 4.5) fails.push(`on ${k}: ${r.toFixed(2)} < 4.5`);
			}
			if (contrast(ign, side) < 4.5) fails.push(`over sidebar: ${contrast(ign, side).toFixed(2)} < 4.5`);
			// "dimmed": the contrast against the editor background drops by at least 15% from the primary text's
			const dim =
				contrast(ign, rgb(s["editor.background"])) / contrast(rgb(s.text), rgb(s["editor.background"]));
			if (dim > 0.85) fails.push(`not dimmed: ${(dim * 100).toFixed(0)}% of text contrast (want <= 85%)`);
			// and it must not pass for a vcs colour: neutral, so chroma stays well under the hues'
			if (toOklch(ign).c > 0.06) fails.push(`chroma ${toOklch(ign).c.toFixed(3)} reads as a hue`);
			expect(fails).toEqual([]);
		});
	}

	test("an explicit theme value that reads is kept; one that vanishes is lifted to AA keeping its hue", () => {
		const keep = complete({ "editor.background": "#101820", "version_control.ignored": "#b4c1d1" }, "dark");
		expect(keep["version_control.ignored"]).toBe("#b4c1d1");
		const bad = complete({ "editor.background": "#fafafa", "version_control.ignored": "#d8d8d8" }, "light");
		expect(bad["version_control.ignored"]).not.toBe("#d8d8d8");
		expect(contrast(rgb(bad["version_control.ignored"]), rgb(bad.background))).toBeGreaterThanOrEqual(4.5);
	});

	test("the token drives --atlas-vcs-ignored in the CSS vars", () => {
		const vars = cssVars(complete(bundled[0].style, bundled[0].appearance));
		expect(vars["--atlas-vcs-ignored"]).toMatch(/^#[0-9a-f]{6,8}$/i);
	});
});
