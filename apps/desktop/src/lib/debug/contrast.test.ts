// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { beforeEach, describe, expect, test } from "bun:test";
import {
	composite,
	contrastAudit,
	effectiveBackground,
	nonTextAudit,
	parseCssColor,
	ratio,
	requiredRatio,
	summarize,
} from "./contrast.ts";

describe("parseCssColor", () => {
	test("rgb()/rgba()/hex in the forms getComputedStyle and var() produce", () => {
		expect(parseCssColor("rgb(255, 0, 0)")).toEqual({ r: 255, g: 0, b: 0, a: 1 });
		expect(parseCssColor("rgba(0, 0, 0, 0.5)")).toEqual({ r: 0, g: 0, b: 0, a: 0.5 });
		expect(parseCssColor("rgb(10 20 30 / 25%)")).toEqual({ r: 10, g: 20, b: 30, a: 0.25 });
		expect(parseCssColor("#fff")).toEqual({ r: 255, g: 255, b: 255, a: 1 });
		expect(parseCssColor("#00000080")?.a).toBeCloseTo(0.5, 2);
		expect(parseCssColor("transparent")).toEqual({ r: 0, g: 0, b: 0, a: 0 });
	});
	// Incident: WebKit reports `color-mix(in oklab, …)` as `oklab(L a b / alpha)`, which made the whole sidebar "unknown".
	test("oklab()/oklch()/color(srgb) as WebKit reports color-mix() and kit tokens", () => {
		const w = parseCssColor("oklab(1 0 0 / 0.5)");
		expect(w).toMatchObject({ r: 255, g: 255, b: 255 });
		expect(w?.a).toBeCloseTo(0.5, 5);
		expect(parseCssColor("oklab(0 0 0)")).toMatchObject({ r: 0, g: 0, b: 0, a: 1 });
		const l = parseCssColor("oklch(97% 0.003 260 / 0.15)");
		expect(l?.a).toBeCloseTo(0.15, 5);
		expect(l?.r).toBeGreaterThan(240);
		expect(parseCssColor("color(srgb 1 0 0 / 0.5)")).toMatchObject({ r: 255, g: 0, b: 0, a: 0.5 });
	});
	test("what it cannot read is undefined, never a guess (color-mix(), named, garbage)", () => {
		expect(parseCssColor("color-mix(in srgb, red, blue)")).toBeUndefined();
		expect(parseCssColor("")).toBeUndefined();
		expect(parseCssColor("nonsense")).toBeUndefined();
	});
});

describe("composite + ratio", () => {
	test("alpha composites over an opaque backdrop; opaque top wins; transparent top vanishes", () => {
		expect(composite({ r: 0, g: 0, b: 0, a: 0.5 }, { r: 255, g: 255, b: 255, a: 1 })).toEqual({
			r: 128,
			g: 128,
			b: 128,
			a: 1,
		});
		expect(composite({ r: 9, g: 9, b: 9, a: 1 }, { r: 255, g: 255, b: 255, a: 1 }).r).toBe(9);
		expect(composite({ r: 9, g: 9, b: 9, a: 0 }, { r: 200, g: 100, b: 50, a: 1 })).toEqual({
			r: 200,
			g: 100,
			b: 50,
			a: 1,
		});
	});
	test("WCAG ratio: black/white is 21, equal is 1, symmetric", () => {
		const k = { r: 0, g: 0, b: 0, a: 1 };
		const w = { r: 255, g: 255, b: 255, a: 1 };
		expect(ratio(k, w)).toBeCloseTo(21, 5);
		expect(ratio(w, k)).toBeCloseTo(21, 5);
		expect(ratio(w, w)).toBeCloseTo(1, 5);
	});
	test("the incident: light gray text on a light surface is ~1.6, nowhere near AA", () => {
		const r = ratio({ r: 200, g: 200, b: 200, a: 1 }, { r: 240, g: 240, b: 240, a: 1 });
		expect(r).toBeLessThan(1.7);
	});
});

describe("requiredRatio", () => {
	test("4.5 for normal text, 3 for large (>=24px) or bold large (>=18.66px bold)", () => {
		expect(requiredRatio({ sizePx: 13, weight: 400 })).toBe(4.5);
		expect(requiredRatio({ sizePx: 18, weight: 700 })).toBe(4.5);
		expect(requiredRatio({ sizePx: 18.67, weight: 700 })).toBe(3);
		expect(requiredRatio({ sizePx: 24, weight: 400 })).toBe(3);
		expect(requiredRatio({ sizePx: 23.9, weight: 400 })).toBe(4.5);
		expect(requiredRatio({ sizePx: 20, weight: 600 })).toBe(3);
	});
	test("UI icons need 3", () => {
		expect(requiredRatio({ sizePx: 14, weight: 400, icon: true })).toBe(3);
	});
});

/** happy-dom does no layout: mark an element as laid out and visible. */
function box(el: Element, w = 100, h = 20) {
	el.getBoundingClientRect = () => ({
		x: 0,
		y: 0,
		left: 0,
		top: 0,
		right: w,
		bottom: h,
		width: w,
		height: h,
		toJSON() {},
	});
	el.checkVisibility = () => true;
}
const mount = (html: string) => {
	document.body.innerHTML = html;
	document.querySelectorAll("*").forEach((e) => box(e));
};
beforeEach(() => {
	document.body.removeAttribute("style");
	document.body.innerHTML = "";
});

describe("effectiveBackground", () => {
	test("walks ancestors compositing translucent layers over the first opaque one", () => {
		mount(
			`<div style="background:#ffffff"><div style="background:rgba(0,0,0,0.5)"><span id=t>x</span></div></div>`,
		);
		const bg = effectiveBackground(document.getElementById("t"));
		expect(bg.kind).toBe("color");
		expect(bg.color).toMatchObject({ r: 128, g: 128, b: 128 });
	});
	test("a gradient or image in the chain makes the background unknown, not wrong", () => {
		mount(
			`<div style="background:#fff"><div style="background-image:linear-gradient(red,blue)"><span id=t>x</span></div></div>`,
		);
		expect(effectiveBackground(document.getElementById("t")).kind).toBe("unknown");
	});
	test("falls back to the page canvas (white) when nothing paints", () => {
		mount(`<div><span id=t>x</span></div>`);
		expect(effectiveBackground(document.getElementById("t"))).toMatchObject({
			kind: "color",
			color: { r: 255, g: 255, b: 255 },
		});
	});
	// Regression (2026-10-13): the welcome copy sits over a canvas night sky. Canvas pixels are not a CSS background,
	// so the audit judged dark-grey text against the white page and passed it while it was unreadable on screen.
	test("an ancestor's declared data-contrast-bg (canvas/WebGL stage) is the background, not the page behind it", () => {
		mount(`<div data-contrast-bg="#04050a"><canvas></canvas><p><span id=t>x</span></p></div>`);
		expect(effectiveBackground(document.getElementById("t"))).toMatchObject({
			kind: "color",
			color: { r: 4, g: 5, b: 10 },
		});
	});
	test("translucent layers between the text and a declared stage still composite over it", () => {
		mount(
			`<div data-contrast-bg="#000000"><div style="background:rgba(255,255,255,0.5)"><span id=t>x</span></div></div>`,
		);
		expect(effectiveBackground(document.getElementById("t")).color).toMatchObject({ r: 128, g: 128, b: 128 });
	});
	test("an unreadable declared colour is unknown, never a guess", () => {
		mount(`<div data-contrast-bg="night"><span id=t>x</span></div>`);
		expect(effectiveBackground(document.getElementById("t")).kind).toBe("unknown");
	});
});

describe("contrastAudit", () => {
	test("flags light gray on light gray (the sidebar incident) with ratio, required, selector and var tokens", () => {
		mount(
			`<aside class="sidebar" style="background:#f0f0f0"><span class="hint" style="color:#c8c8c8;font-size:13px;--ca-text-hint:#c8c8c8">Filter worktrees</span></aside>`,
		);
		const r = contrastAudit(document.body);
		expect(r.failures).toHaveLength(1);
		const [f] = r.failures;
		expect(f.text).toBe("Filter worktrees");
		expect(f.selector).toContain("span.hint");
		expect(f.ratio).toBeLessThan(1.7);
		expect(f.required).toBe(4.5);
		expect(f.vars.fg).toEqual(["--ca-text-hint"]);
	});
	test("vars names only the theme tokens whose value IS the failing foreground or background, not every inherited var", () => {
		document.documentElement.style.setProperty("--ca-text-secondary", "#c8c8c8");
		document.documentElement.style.setProperty("--ca-text-primary", "#111111");
		document.documentElement.style.setProperty("--ca-surface-sunken", "#f0f0f0");
		document.documentElement.style.setProperty("--atlas-ui-font-size", "13px");
		mount(
			`<aside style="background:#f0f0f0"><span style="color:#c8c8c8;font-size:13px">Filter</span></aside>`,
		);
		const [f] = contrastAudit(document.body).failures;
		expect(f.vars).toEqual({ fg: ["--ca-text-secondary"], bg: ["--ca-surface-sunken"] });
		document.documentElement.removeAttribute("style");
	});
	test("a subtree marked data-contrast-ignore (a preview that paints ANOTHER theme on purpose) is skipped and counted", () => {
		mount(
			`<div style="background:#fff"><p style="color:#111;font-size:14px">ui</p><div data-contrast-ignore><p style="color:#eee;font-size:14px">preview</p></div></div>`,
		);
		const r = contrastAudit(document.body);
		expect(r.failures).toEqual([]);
		expect(r.ignored).toBe(1);
	});
	test("editor syntax colours are text: a failing token is a failure like any label (no `code` carve-out)", () => {
		mount(
			`<div style="background:#fff"><div class="cm-content"><div class="cm-line"><span style="color:#cccccc;font-size:13px">number</span></div></div><p style="color:#cccccc;font-size:13px">label</p></div>`,
		);
		const r = contrastAudit(document.body);
		expect(r.failures.map((f) => f.text).sort()).toEqual(["label", "number"]);
		expect("code" in r).toBe(false);
	});
	test("passes AA text and does not report it", () => {
		mount(`<div style="background:#fff"><p style="color:#111;font-size:14px">Readable</p></div>`);
		expect(contrastAudit(document.body).failures).toEqual([]);
	});
	test("opacity chain multiplies into the effective foreground", () => {
		mount(
			`<div style="background:#fff"><div style="opacity:0.5"><p style="color:#000;opacity:0.5;font-size:14px">faded</p></div></div>`,
		);
		const r = contrastAudit(document.body);
		// black at 0.25 over white = #bfbfbf: ratio ~1.8
		expect(r.failures).toHaveLength(1);
		expect(r.failures[0].ratio).toBeLessThan(2);
	});
	test("text whose opacity chain is ~0 (mid fade-in, collapsed panel) is invisible, so it is not a failure", () => {
		mount(
			`<div style="background:#fff"><div style="opacity:0"><p style="color:#fafafa;font-size:14px">fading in</p></div></div>`,
		);
		const r = contrastAudit(document.body);
		expect(r.failures).toEqual([]);
		expect(r.checked).toBe(0);
	});
	test("large text only needs 3", () => {
		// #8a8a8a on white is ~3.4: fails body, passes large
		mount(
			`<div style="background:#fff"><h1 style="color:#8a8a8a;font-size:28px">Big</h1><p style="color:#8a8a8a;font-size:13px">small</p></div>`,
		);
		const r = contrastAudit(document.body);
		expect(r.failures.map((f) => f.text)).toEqual(["small"]);
	});
	test("an unknown background is reported as unknown, never as pass or fail", () => {
		mount(`<div style="background-image:url(x.png)"><p style="color:#000;font-size:14px">on image</p></div>`);
		const r = contrastAudit(document.body);
		expect(r.failures).toEqual([]);
		expect(r.unknown.map((u) => u.text)).toEqual(["on image"]);
	});
	test("hidden elements and empty text are skipped", () => {
		mount(
			`<div style="background:#fff"><p style="color:#fefefe;display:none">hidden</p><p style="color:#fefefe;visibility:hidden">invisible</p><p style="color:#fefefe">  </p></div>`,
		);
		// display:none: WebKit reports no box.
		document.querySelectorAll("p").forEach((p, i) => {
			if (i < 1) p.checkVisibility = () => false;
		});
		expect(contrastAudit(document.body).failures).toEqual([]);
	});
	test("line-number gutters and placeholders need 3, like disabled controls (documented relaxation)", () => {
		// #8a8a8a on white is ~3.4: passes 3, fails 4.5
		mount(
			`<div style="background:#fff"><div class="cm-gutterElement" style="color:#8a8a8a;font-size:13px">12</div><p style="color:#8a8a8a;font-size:13px">body</p></div>`,
		);
		const r = contrastAudit(document.body);
		expect(r.failures.map((f) => f.text)).toEqual(["body"]);
		mount(
			`<div style="background:#fff"><div class="cm-gutterElement" style="color:#bbbbbb;font-size:13px">12</div></div>`,
		);
		expect(contrastAudit(document.body).failures[0].required).toBe(3);
	});
	test("disabled controls need 3, not 4.5 (documented relaxation)", () => {
		// ~3.4 on white
		mount(
			`<div style="background:#fff"><button disabled style="color:#8a8a8a;font-size:13px">Save</button></div>`,
		);
		expect(contrastAudit(document.body).failures).toEqual([]);
		mount(
			`<div style="background:#fff"><button disabled style="color:#bbbbbb;font-size:13px">Save</button></div>`,
		);
		expect(contrastAudit(document.body).failures[0].required).toBe(3);
	});
});

describe("summarize", () => {
	test("top offenders by count, grouped by selector", () => {
		const f = (selector: string) => ({ selector, text: "x", ratio: 1, required: 4.5, vars: {} });
		const s = summarize([f("a.x"), f("a.x"), f("b.y")]);
		expect(s[0]).toEqual({ selector: "a.x", count: 2 });
		expect(s).toHaveLength(2);
	});
});

/**
 * WCAG 2.2 1.4.11: icons, control borders and the focus ring are graphical objects and need 3:1 against what is next
 * to them. `nonTextAudit` measures the real computed colours: an inline `svg`'s stroke/fill (the lucide icons use
 * `currentColor`), the border of a field/checkbox/switch and the outline a focused control paints.
 */
describe("nonTextAudit", () => {
	test("an icon whose stroke is light grey on white fails 3:1; a dark one passes", () => {
		mount(
			`<div style="background:#ffffff"><svg id=a style="stroke:#cccccc;fill:none"></svg><svg id=b style="stroke:#444444;fill:none"></svg></div>`,
		);
		const r = nonTextAudit(document.body);
		expect(r.failures.map((f) => f.selector.split(" > ").pop())).toEqual(["svg#a"]);
		expect(r.failures[0].required).toBe(3);
		expect(r.failures[0].kind).toBe("icon");
		expect(r.checked).toBe(2);
	});
	test("an icon painted with currentColor reads the text colour (the lucide default)", () => {
		mount(
			`<div style="background:#ffffff;color:#d0d0d0"><svg id=a style="stroke:currentColor;fill:none"></svg></div>`,
		);
		expect(nonTextAudit(document.body).failures).toHaveLength(1);
	});
	test("a decorative icon with no paint (stroke none, fill none) and an aria-hidden twin of a labelled control are not graphical objects", () => {
		mount(`<div style="background:#fff"><svg style="stroke:none;fill:none"></svg></div>`);
		expect(nonTextAudit(document.body)).toMatchObject({ checked: 0, failures: [] });
	});
	test("a text field whose 1px border is 1.2:1 against its surface fails; a 3:1 border passes", () => {
		mount(
			`<div style="background:#ffffff"><input id=a style="border:1px solid #eeeeee;background:#ffffff"><input id=b style="border:1px solid #767676;background:#ffffff"></div>`,
		);
		const r = nonTextAudit(document.body);
		expect(r.failures.map((f) => f.selector.split(" > ").pop())).toEqual(["input#a"]);
		expect(r.failures[0].kind).toBe("control-border");
	});
	test("a field with no border but a fill that separates it from the surface (3:1) passes", () => {
		mount(
			`<div style="background:#ffffff"><input id=a style="border:0 solid transparent;background:#767676"></div>`,
		);
		expect(nonTextAudit(document.body).failures).toEqual([]);
	});
	test("a disabled control is held to 3 as well (exempt from AA, but it must be visible)", () => {
		mount(
			`<div style="background:#ffffff"><svg id=a disabled aria-disabled="true" style="stroke:#dddddd;fill:none"></svg></div>`,
		);
		expect(nonTextAudit(document.body).failures).toHaveLength(1);
	});
	test("a hidden icon and a data-contrast-ignore subtree are skipped", () => {
		mount(
			`<div style="background:#fff"><svg style="stroke:#eee;display:none"></svg><div data-contrast-ignore><svg style="stroke:#eee"></svg></div></div>`,
		);
		expect(nonTextAudit(document.body)).toMatchObject({ checked: 0, failures: [], ignored: 1 });
	});
});

describe("nonTextAudit: what the user sees", () => {
	test("an input with no border/fill inside a bordered label is measured by the label (the visible field box)", () => {
		mount(
			`<div style="background:#ffffff"><label id=l style="display:block;border:1px solid #767676;background:#ffffff"><input id=i style="border:0 none;background:transparent"></label></div>`,
		);
		expect(nonTextAudit(document.body).failures).toEqual([]);
	});
	test("the same input in a label whose border is 1.2:1 fails, reported on the field box", () => {
		mount(
			`<div style="background:#ffffff"><label id=l style="display:block;border:1px solid #eeeeee;background:#ffffff"><input id=i style="border:0 none;background:transparent"></label></div>`,
		);
		const r = nonTextAudit(document.body);
		expect(r.failures).toHaveLength(1);
		expect(r.failures[0].kind).toBe("control-border");
	});
	test("an svg with no paint of its own is read through its shapes (stroke on the path, like lucide's children)", () => {
		mount(
			`<div style="background:#ffffff"><svg id=a><path style="stroke:#444444;fill:none"/></svg><svg id=b><circle style="stroke:#dddddd;fill:none"/></svg></div>`,
		);
		const r = nonTextAudit(document.body);
		expect(r.checked).toBe(2);
		expect(r.failures.map((f) => f.selector.split(" > ").pop())).toEqual(["svg#b"]);
	});
	test("a CSS-default svg (fill black, stroke none, no shapes painted) is not a measured icon", () => {
		mount(`<div style="background:#101010"><svg id=a></svg></div>`);
		expect(nonTextAudit(document.body).checked).toBe(0);
	});
});

describe("effectiveBackground: portaled overlays", () => {
	// Incident (contrast audit): a glass menu (translucent bg, blur) portaled to <body> was measured over the white page
	// canvas, so light text on a dark theme "failed" at 2.0:1 while the user sees it over the dark app behind it.
	test("a translucent fixed layer child of <body> is composited over what is under it, not over white", () => {
		mount(
			`<div id=app style="background:#282a36"></div><div id=menu style="position:fixed;background:rgba(40,42,54,0.6)"><span id=t style="color:#f8f8f2">New terminal</span></div>`,
		);
		const app = document.getElementById("app");
		document.elementsFromPoint = () => [
			document.getElementById("menu"),
			app,
			document.body,
			document.documentElement,
		];
		const bg = effectiveBackground(document.getElementById("t"));
		delete document.elementsFromPoint;
		expect(bg.kind).toBe("color");
		// 60% #282a36 over #282a36 is #282a36, not a grey from white
		expect([bg.color.r, bg.color.g, bg.color.b]).toEqual([40, 42, 54]);
	});
	test("an opaque overlay needs no hit-test; with no hit-test at all the canvas is the fallback", () => {
		mount(`<div id=menu style="position:fixed;background:#101010"><span id=t>x</span></div>`);
		expect(effectiveBackground(document.getElementById("t")).color.r).toBe(16);
		mount(`<div id=menu style="position:fixed;background:rgba(0,0,0,0.5)"><span id=t>x</span></div>`);
		expect(effectiveBackground(document.getElementById("t")).color.r).toBe(128);
	});
	test("an overlay that declares what it floats over (data-contrast-bg) uses it instead of the white page canvas", () => {
		mount(
			`<div data-contrast-bg="#282a36" style="background:rgba(255,255,255,0.1)"><span id=t style="color:#f8f8f2">x</span></div>`,
		);
		const bg = effectiveBackground(document.getElementById("t"));
		expect(bg.kind).toBe("color");
		expect(bg.color.r).toBeLessThan(80);
	});
});

describe("nonTextAudit: decorative glyphs", () => {
	// Decision (docs/release.md): a file-type icon repeats what the adjacent filename already says (WCAG 1.4.11 only
	// holds graphics that carry information WITHOUT a text alternative); brand-coloured, so it is marked
	// data-contrast-decorative and skipped (counted), never recoloured.
	test("an svg inside data-contrast-decorative is skipped and counted as ignored; its siblings are still measured", () => {
		mount(
			`<div style="background:#fff"><span data-contrast-decorative><svg id=a style="stroke:#eeeeee"></svg></span><svg id=b style="stroke:#eeeeee"></svg></div>`,
		);
		const r = nonTextAudit(document.body);
		expect(r.ignored).toBe(1);
		expect(r.failures.map((f) => f.selector.split(" > ").pop())).toEqual(["svg#b"]);
	});
});

describe("nonTextAudit: the field box", () => {
	// Incident (audit run): a chrome-less input that still carried a faint own fill (kit global `input` style, 1.2:1)
	// stopped `fieldBox` at the input, so the bordered label around it (the 3:1 box the user sees) was never measured and
	// every search field "failed".
	test("a bordered label wins over a faint fill on the input inside it", () => {
		mount(
			`<div style="background:#ffffff"><label style="display:block;border:1px solid #767676"><input style="border:0 none;background:#f2f2f2"></label></div>`,
		);
		expect(nonTextAudit(document.body).failures).toEqual([]);
	});
	test("with no border anywhere, the faint own fill is what is measured (and fails)", () => {
		mount(
			`<div style="background:#ffffff"><label style="display:block"><input style="border:0 none;background:#f2f2f2"></label></div>`,
		);
		expect(nonTextAudit(document.body).failures).toHaveLength(1);
	});
});

describe("nonTextAudit: progress graphics (track + indicator)", () => {
	// A ring/spinner is a faint TRACK plus the INDICATOR arc: the information is in the arc (WCAG 1.4.11: the part
	// needed to understand the content), the track is context. A faint track must not fail the graphic when the
	// indicator reads; an all-faint graphic still fails.
	test("a svg whose indicator shape reads 3:1 passes although its track is faint", () => {
		mount(
			`<div style="background:#ffffff"><svg id=a><circle style="stroke:#eeeeee;fill:none"/><circle style="stroke:#444444;fill:none"/></svg></div>`,
		);
		expect(nonTextAudit(document.body).failures).toEqual([]);
	});
	test("an svg whose every shape is faint still fails", () => {
		mount(
			`<div style="background:#ffffff"><svg id=a><circle style="stroke:#eeeeee;fill:none"/><circle style="stroke:#e0e0e0;fill:none"/></svg></div>`,
		);
		expect(nonTextAudit(document.body).failures).toHaveLength(1);
	});
});

describe("effectiveBackground: the native window material", () => {
	// Incident (audit run): the sidebar is 82% opaque over the native window material, which follows the appearance
	// (black under a dark theme, white under a light one: `SIDEBAR_SEE_THROUGH` in theme/derive.ts guards the same model).
	// The audit composited it over a white canvas in dark themes and reported fields "failing" on a grey that is not on screen.
	test("a translucent layer over nothing opaque is composited over black when the page is dark, white when light", () => {
		mount(`<div style="background:rgba(33,34,44,0.82)"><span id=t>x</span></div>`);
		document.documentElement.classList.add("dark");
		const dark = effectiveBackground(document.getElementById("t")).color;
		document.documentElement.classList.remove("dark");
		const light = effectiveBackground(document.getElementById("t")).color;
		expect(dark.r).toBeLessThan(light.r);
		expect(Math.round(dark.r)).toBe(Math.round(33 * 0.82));
		expect(Math.round(light.r)).toBe(Math.round(33 * 0.82 + 255 * 0.18));
	});
});

describe("nonTextAudit: a bare input in a panel is not the panel's border", () => {
	// Decision (docs/release.md): a field's 3:1 boundary is the box that wraps THE FIELD (a label, the input itself). A
	// container that holds other controls too (the command palette: input + a list of buttons) is a surface, not the
	// field; an input with no box of its own there is identified by its placeholder and caret, which WCAG 1.4.11 does
	// not hold to a boundary. It is counted as checked and not failed.
	test("an input whose only bordered ancestor also holds other controls is judged by itself (no box), not by the panel", () => {
		mount(
			`<div style="background:#ffffff"><div style="border:1px solid #eeeeee;background:#ffffff"><input style="border:0 none;background:transparent"><button>One</button><button>Two</button></div></div>`,
		);
		expect(nonTextAudit(document.body).failures).toEqual([]);
	});
	test("a label that wraps only the input (+ its icon/clear button) is still the field box", () => {
		mount(
			`<div style="background:#ffffff"><label style="display:flex;border:1px solid #eeeeee"><svg></svg><input style="border:0 none;background:transparent"><button>x</button></label></div>`,
		);
		expect(nonTextAudit(document.body).failures).toHaveLength(1);
	});
});
