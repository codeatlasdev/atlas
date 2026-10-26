// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

// Contract between the page and the native side, read from the real sources (not retyped): GPU terminals sit UNDER the
// transparent webview, so nothing in the terminal path may hide, park or re-place a terminal because a menu, dialog,
// popover or the sidebar peek opened. Only embedded browser tabs (child webviews ABOVE the page) still ask `overlayOpen`.

const src = (rel: string) => readFileSync(new URL(rel, import.meta.url), "utf8");
const root = "../../../../../../crates/atlas-desktop/src/term/";

test("the terminal path never consults overlayOpen: opening an overlay cannot park a terminal", () => {
	for (const f of ["nativeView.ts", "termLayout.ts", "../GpuTerminal.svelte"])
		expect(src(f), f).not.toContain("overlayOpen");
	// The browser tab still does: a child webview cannot go under the page.
	expect(src("../BrowserTab.svelte")).toContain("overlayOpen");
});

test("a terminal's watcher does not observe overlays (no data-state / childList over the body)", () => {
	expect(src("../GpuTerminal.svelte")).toMatch(/watchSlot\(area!, view\.sync, \(\) =>/);
	expect(src("../GpuTerminal.svelte")).not.toContain("overlays: true");
	expect(src("../BrowserTab.svelte")).toContain("overlays: true");
});

test("the native view goes in BELOW the page's web view, never above it", () => {
	const mac = readFileSync(new URL(`${root}mac.rs`, import.meta.url), "utf8");
	expect(mac).toContain("NSWindowOrderingMode::Below");
	expect(mac).not.toMatch(/content\.addSubview\(&view\)/);
});

test("the backend has no overlay input: placement is a function of the tab being on screen only", () => {
	const shared = readFileSync(new URL(`${root}shared.rs`, import.meta.url), "utf8");
	const layering = readFileSync(new URL(`${root}layering.rs`, import.meta.url), "utf8");
	for (const s of [shared, layering]) expect(s).not.toMatch(/overlay_open|covered|under_dialog/);
	// `term_layout` takes tab/x/y/w/h/visible and nothing else.
	const life = readFileSync(new URL(`${root}lifecycle.rs`, import.meta.url), "utf8");
	const slot = life.slice(life.indexOf("pub struct Slot"), life.indexOf("pub struct Placed"));
	expect(slot.match(/pub (\w+):/g)).toEqual(["pub tab:", "pub x:", "pub y:", "pub w:", "pub h:", "pub visible:"]);
});

test("every native-geometry call of a terminal goes through the one batch", () => {
	const nv = src("nativeView.ts");
	expect(nv).not.toContain('invoke("term_bounds"');
	expect(nv).toContain("termLayout.request()");
});

test("the ground and card are separate masked layers, the body transparent only with a hole (macOS)", () => {
	const css = readFileSync(new URL("../../../app.css", import.meta.url), "utf8");
	expect(css).toMatch(/html\.term-hole:not\(\.opaque-window\)[^{]*\{[^}]*background: transparent !important/);
	expect(css).toMatch(/\.ground\s*\{/);
	expect(css).toMatch(/\.card\s*\{/);
	// `.content` itself paints no surface: a mask on it would also mask its children.
	expect(css.match(/\.content\s*\{[^}]*\}/)![0]).not.toContain("background:");
});

// The page is above the native view, so the mouse cursor over a terminal is the page's: the slot must set it.
test("the terminal slot sets the text I-beam, and the pointer while links are live", () => {
	const svelte = src("../GpuTerminal.svelte");
	expect(svelte.match(/\.area\s*\{[^}]*\}/)![0]).toMatch(/cursor:\s*text/);
	expect(svelte).toMatch(/\.area\.link\s*\{[^}]*cursor:\s*pointer/);
	expect(svelte).toContain("class:link={linkMode}");
	expect(src("pointer.ts")).toContain("onLinkMode?.(true)");
	expect(src("pointer.ts")).toContain("onLinkMode?.(false)");
});
