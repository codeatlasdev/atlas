// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { DEFAULT_KEYMAP, merge, strokeOf } from "./keys.ts";
import { MENU_COMMANDS, menuAccelerators } from "./menu.ts";
import { DEFAULT_PREFS, resolvePrefs } from "./prefs.ts";
import {
	ZOOM_LEVELS,
	applyZoom,
	appliedZoom,
	nativeRect,
	toNativeRect,
	zoomAction,
	zoomStep,
} from "./zoom.ts";

const key = (k, m = {}) => ({
	key: k,
	code: "",
	metaKey: false,
	ctrlKey: false,
	altKey: false,
	shiftKey: false,
	...m,
});

test("zoom steps through a fixed ladder, in and out are exact inverses, and 1 is on it", () => {
	expect(ZOOM_LEVELS).toContain(1);
	expect(ZOOM_LEVELS).toEqual([...ZOOM_LEVELS].sort((a, b) => a - b));
	for (const z of ZOOM_LEVELS.slice(0, -1)) expect(zoomStep(zoomStep(z, "in"), "out")).toBe(z);
	expect(zoomStep(1, "in")).toBe(1.1);
	expect(zoomStep(1, "out")).toBe(0.9);
	expect(zoomStep(2, "reset")).toBe(1);
});

test("zoom is bounded: stepping past either end stays on the end", () => {
	const lo = ZOOM_LEVELS[0];
	const hi = ZOOM_LEVELS[ZOOM_LEVELS.length - 1];
	expect(zoomStep(lo, "out")).toBe(lo);
	expect(zoomStep(hi, "in")).toBe(hi);
	let z = 1;
	for (let i = 0; i < 100; i++) z = zoomStep(z, "in");
	expect(z).toBe(hi);
	for (let i = 0; i < 100; i++) z = zoomStep(z, "out");
	expect(z).toBe(lo);
});

test("a hand-edited off-ladder value snaps to the next step in the direction pressed", () => {
	// 1.03 is between 1 and 1.1: in goes up, out goes down, so a key press always moves.
	expect(zoomStep(1.03, "in")).toBe(1.1);
	expect(zoomStep(1.03, "out")).toBe(1);
	expect(zoomStep(0.2, "in")).toBe(ZOOM_LEVELS[0]);
	expect(zoomStep(9, "out")).toBe(ZOOM_LEVELS[ZOOM_LEVELS.length - 1]);
	// past either end the key lands back on the ladder (never returns the out-of-range value itself)
	expect(zoomStep(9, "in")).toBe(ZOOM_LEVELS[ZOOM_LEVELS.length - 1]);
	expect(zoomStep(0.2, "out")).toBe(ZOOM_LEVELS[0]);
});

test("⌘ = / + / - / 0 on macOS, Ctrl on other systems, never the other modifier or extra ones", () => {
	const mac = (k, m) => zoomAction(key(k, m), true);
	const other = (k, m) => zoomAction(key(k, m), false);
	for (const k of ["=", "+"]) {
		expect(mac(k, { metaKey: true })).toBe("in");
		expect(mac(k, { metaKey: true, shiftKey: true })).toBe("in");
		expect(other(k, { ctrlKey: true })).toBe("in");
	}
	expect(mac("-", { metaKey: true })).toBe("out");
	expect(other("-", { ctrlKey: true })).toBe("out");
	expect(mac("0", { metaKey: true })).toBe("reset");
	expect(other("0", { ctrlKey: true })).toBe("reset");
	// the wrong modifier for the platform, a bare key and ⌥/⌃ chords are not zoom
	expect(mac("=", { ctrlKey: true })).toBeNull();
	expect(other("=", { metaKey: true })).toBeNull();
	expect(mac("=")).toBeNull();
	expect(mac("=", { metaKey: true, altKey: true })).toBeNull();
	expect(mac("-", { metaKey: true, ctrlKey: true })).toBeNull();
	expect(other("-", { ctrlKey: true, altKey: true })).toBeNull();
	// ⇧ with 0 / - is another chord
	expect(mac("0", { metaKey: true, shiftKey: true })).toBeNull();
	expect(mac("-", { metaKey: true, shiftKey: true })).toBeNull();
	expect(mac("a", { metaKey: true })).toBeNull();
});

test("the numpad plus / minus / zero zoom too (Ctrl+NumpadAdd), keyed by physical code", () => {
	const e = (code, k, m) => zoomAction({ ...key(k, m), code }, false);
	expect(e("NumpadAdd", "+", { ctrlKey: true })).toBe("in");
	expect(e("NumpadSubtract", "-", { ctrlKey: true })).toBe("out");
	expect(e("Numpad0", "0", { ctrlKey: true })).toBe("reset");
});

test("a layout where = needs another key still zooms by physical key (code Equal / Minus / Digit0)", () => {
	// German layout: "+" is its own key; the physical Equal key types "´". Ctrl+Equal must still zoom in.
	const e = (code, k) => zoomAction({ ...key(k, { ctrlKey: true }), code }, false);
	expect(e("Equal", "´")).toBe("in");
	expect(e("Minus", "ß")).toBe("out");
	expect(e("Digit0", "0")).toBe("reset");
});

test("native views take window points: CSS px × zoom, exact for every ladder step, identity at 100%", () => {
	for (const z of ZOOM_LEVELS) {
		const n = toNativeRect({ x: 100, y: 200, w: 300, h: 400 }, z);
		expect(n.x).toBeCloseTo(100 * z, 9);
		expect(n.y).toBeCloseTo(200 * z, 9);
		expect(n.w).toBeCloseTo(300 * z, 9);
		expect(n.h).toBeCloseTo(400 * z, 9);
	}
	expect(toNativeRect({ x: 10, y: 20, w: 300, h: 200 }, 1.5)).toEqual({ x: 15, y: 30, w: 450, h: 300 });
	// at 100% nothing changes (the default path must be byte-identical)
	const r = { x: 10.25, y: 20.5, w: 300.75, h: 200.125 };
	expect(toNativeRect(r, 1)).toEqual(r);
});

test("native rects of a degenerate or non-finite zoom are never produced", () => {
	const r = { x: 1, y: 2, w: 3, h: 4 };
	for (const bad of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) expect(toNativeRect(r, bad)).toEqual(r);
});

// The factor the webview really has is shared by every native placement (terminal, browser, pager, swipe,
// captures), so a rect can never be placed with a stale factor after a change.
test("every native rect uses the factor Rust last applied, and a bad factor leaves it unchanged", () => {
	expect(appliedZoom()).toBe(1);
	expect(nativeRect({ x: 10, y: 20, w: 30, h: 40 })).toEqual({ x: 10, y: 20, w: 30, h: 40 });
	applyZoom(1.5);
	expect(appliedZoom()).toBe(1.5);
	expect(nativeRect({ x: 10, y: 20, w: 30, h: 40 })).toEqual({ x: 15, y: 30, w: 45, h: 60 });
	for (const bad of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
		applyZoom(bad);
		expect(appliedZoom()).toBe(1.5);
	}
	applyZoom(1);
	expect(appliedZoom()).toBe(1);
});

// Persistence: the zoom lives in settings.json under `zoom`, read through the same sanitizer as every pref.
test("the saved zoom is read back, bounded, and a bad value falls back to 100%", () => {
	expect(DEFAULT_PREFS.zoom).toBe(1);
	expect(resolvePrefs({}).zoom).toBe(1);
	expect(resolvePrefs({ zoom: 1.25 }).zoom).toBe(1.25);
	expect(resolvePrefs({ zoom: ZOOM_LEVELS[0] }).zoom).toBe(ZOOM_LEVELS[0]);
	expect(resolvePrefs({ zoom: ZOOM_LEVELS[ZOOM_LEVELS.length - 1] }).zoom).toBe(
		ZOOM_LEVELS[ZOOM_LEVELS.length - 1],
	);
	for (const bad of [0, -2, 0.1, 50, "big", null, Number.NaN, [], {}])
		expect(resolvePrefs({ zoom: bad }).zoom).toBe(1);
});

test("zoom is a command with a default key and a menu item, and the keys survive a user remap", () => {
	const acc = menuAccelerators(merge(DEFAULT_KEYMAP, {}));
	expect(MENU_COMMANDS).toEqual(
		expect.arrayContaining(["shell::ZoomIn", "shell::ZoomOut", "shell::ZoomReset"]),
	);
	expect(acc["shell::ZoomIn"]).toBe("Cmd+=");
	expect(acc["shell::ZoomOut"]).toBe("Cmd+-");
	expect(acc["shell::ZoomReset"]).toBe("Cmd+0");
	const remapped = menuAccelerators(merge(DEFAULT_KEYMAP, { "cmd-=": null, "cmd-shift-=": "shell::ZoomIn" }));
	expect(remapped["shell::ZoomIn"]).toBe("Shift+Cmd+=");
});

// A focused GPU terminal keeps ⌘= ⌘- ⌘0 for its own font size (Ghostty); the menu must release them there.
test("a focused terminal keeps ⌘+ ⌘- ⌘0 for its font size; the menu gives them back elsewhere", () => {
	const keys = merge(DEFAULT_KEYMAP, {});
	const inTerminal = menuAccelerators(keys, true);
	expect(inTerminal["shell::ZoomIn"]).toBeNull();
	expect(inTerminal["shell::ZoomOut"]).toBeNull();
	expect(inTerminal["shell::ZoomReset"]).toBeNull();
	expect(menuAccelerators(keys)["shell::ZoomIn"]).toBe("Cmd+=");
});

// ⌃- is "Go Back" in the default keymap, and Ctrl+- is also the zoom-out key outside macOS (issue #25). The window
// handler asks zoom first, so on Linux the key zooms; on macOS zoom needs ⌘, so ⌃- keeps its editor meaning.
test("Ctrl+- zooms out outside macOS and stays Go Back on macOS", () => {
	const ctrlMinus = { ...key("-", { ctrlKey: true }), code: "Minus" };
	expect(merge(DEFAULT_KEYMAP, {}).get(strokeOf(ctrlMinus))).toBe("editor::GoBack");
	expect(zoomAction(ctrlMinus, false)).toBe("out");
	expect(zoomAction(ctrlMinus, true)).toBeNull();
});
