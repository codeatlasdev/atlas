// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import fixture from "./__fixtures__/browser-key.json";
import { forwardedStrokes, strokeOfBrowserKey } from "./browserKeys.ts";
import { DEFAULT_KEYMAP, merge, normalize } from "./keys.ts";

const pushed = (user = {}) => forwardedStrokes(merge(DEFAULT_KEYMAP, user));

// Guards issue #42: ⌘1…⌘9, ⌃Tab, ⌃⌘1…6 and ⇧⌘[ ] were dead while a browser page had focus.
test("every tab and worktree switch of the default keymap is pushed to the native side", () => {
	const strokes = pushed();
	const switches = Object.entries(DEFAULT_KEYMAP)
		.filter(([, id]) => /^(pane::Activate|atlas::RecentWorktree)/.test(id))
		.map(([k]) => k);
	// ⌘1…9 (8 items + last), ⌃Tab ×2, ⇧⌘[ ], ⌃⌘1…6: 9 + 2 + 2 + 6 = 19.
	expect(switches).toHaveLength(19);
	for (const k of switches) expect(strokes, k).toContain(normalize(k));
});

test("a user remap is pushed and the freed default is not", () => {
	const strokes = pushed({ "cmd-1": null, "alt-cmd-1": "pane::ActivateItem1" });
	expect(strokes).toContain("alt-cmd-1");
	expect(strokes).not.toContain("cmd-1");
});

test("a binding the user removed is not pushed, so the page keeps the key", () => {
	expect(pushed({ "ctrl-tab": null })).not.toContain("ctrl-tab");
});

test("plain and shift-only strokes are never pushed: they would swallow typing", () => {
	const strokes = pushed({ a: "x::Y", "shift-b": "x::Z", enter: "x::W", escape: "x::V" });
	for (const bad of ["a", "shift-b", "enter", "escape", "shift-escape"]) {
		expect(strokes, bad).not.toContain(bad);
	}
	for (const s of strokes) expect(/ctrl|alt|cmd|(^|-)f\d+$/.test(s), s).toBe(true);
});

test("the page keeps the Edit chords even when a user binds them", () => {
	const strokes = pushed({ "cmd-c": "x::C", "cmd-v": "x::V", "cmd-a": "x::A", "cmd-z": "x::Z" });
	for (const k of ["cmd-c", "cmd-v", "cmd-a", "cmd-z"]) expect(strokes, k).not.toContain(k);
});

test("the pushed list is sorted and duplicate-free, so an unchanged keymap re-pushes nothing", () => {
	const strokes = pushed();
	expect(strokes).toEqual([...new Set(strokes)].sort());
});

// Contract: __fixtures__/browser-key.json is the REAL serialization of a forwarded key
// (crates/atlas-desktop/src/browser_keys.rs, `UPDATE_FIXTURES=1` regenerates it).
test("a real forwarded key resolves to the keymap's stroke and command", () => {
	const stroke = strokeOfBrowserKey(fixture);
	expect(stroke).toBe("ctrl-shift-tab");
	expect(merge(DEFAULT_KEYMAP, {}).get(stroke)).toBe("pane::ActivateOlderRecentItem");
});

test("every forwarded stroke survives the Rust → strokeOf round trip with the same spelling", () => {
	const eff = merge(DEFAULT_KEYMAP, {});
	const key = (code, m = {}) =>
		strokeOfBrowserKey({ code, ctrl: false, alt: false, shift: false, meta: false, ...m });
	expect(eff.get(key("Digit1", { meta: true }))).toBe("pane::ActivateItem1");
	expect(eff.get(key("Digit9", { meta: true }))).toBe("pane::ActivateLastItem");
	expect(eff.get(key("Digit3", { ctrl: true, meta: true }))).toBe("atlas::RecentWorktree3");
	expect(eff.get(key("BracketLeft", { shift: true, meta: true }))).toBe("pane::ActivatePreviousItem");
	expect(eff.get(key("Tab", { ctrl: true }))).toBe("pane::ActivateRecentItem");
});
