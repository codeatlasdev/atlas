// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { fontStep, keyToSend, optionActsAsAlt, termAction, xtermKey } from "./termKeys.ts";

const ev = (
	key: string,
	m: Partial<Record<"metaKey" | "altKey" | "ctrlKey" | "shiftKey", boolean>> = {},
) => ({
	key,
	metaKey: false,
	altKey: false,
	ctrlKey: false,
	shiftKey: false,
	...m,
});
const cmd = { metaKey: true };
const cmdShift = { metaKey: true, shiftKey: true };

// Ghostty macOS defaults (`ghostty +list-keybinds --default`) that the GPU terminal follows.
test("⌘ chords map to the Ghostty actions", () => {
	const table: [string, object, object | null][] = [
		["f", cmd, { kind: "search" }],
		["c", cmd, { kind: "copy" }],
		["a", cmd, { kind: "selectAll" }],
		["k", cmd, { kind: "clear" }],
		["=", cmd, { kind: "font", change: "increase" }],
		["+", cmd, { kind: "font", change: "increase" }],
		["+", cmdShift, { kind: "font", change: "increase" }],
		["-", cmd, { kind: "font", change: "decrease" }],
		["0", cmd, { kind: "font", change: "reset" }],
		["d", cmd, { kind: "split", dir: "right" }],
		["d", cmdShift, { kind: "split", dir: "down" }],
		["ArrowUp", cmd, { kind: "jumpPrompt", up: true }],
		["ArrowDown", cmd, { kind: "jumpPrompt", up: false }],
		["ArrowUp", cmdShift, { kind: "jumpPrompt", up: true }],
		["ArrowDown", cmdShift, { kind: "jumpPrompt", up: false }],
		["Home", cmd, { kind: "scroll", to: "top" }],
		["End", cmd, { kind: "scroll", to: "bottom" }],
		["PageUp", cmd, { kind: "scroll", to: "pageUp" }],
		["PageDown", cmd, { kind: "scroll", to: "pageDown" }],
		// macOS line editing: sent to the program, encoded in Rust (^U ^A ^E).
		["Backspace", cmd, { kind: "key", name: "Backspace", meta: true }],
		["ArrowLeft", cmd, { kind: "key", name: "ArrowLeft", meta: true }],
		["ArrowRight", cmd, { kind: "key", name: "ArrowRight", meta: true }],
		// Ghostty `navigate_search`: next / previous match of the open find bar (⌘G / ⇧⌘G).
		["g", cmd, { kind: "searchNext", forward: true }],
		["g", cmdShift, { kind: "searchNext", forward: false }],
		["G", cmdShift, { kind: "searchNext", forward: false }],
		// Select the last command's output (OSC 133): no Ghostty default, ⇧⌘Y is free in the app keymap.
		["y", cmdShift, { kind: "selectOutput" }],
	];
	for (const [key, mods, want] of table) {
		expect(termAction(ev(key, mods)), `${key} ${JSON.stringify(mods)}`).toEqual(want);
	}
});

test("caps lock and shifted letters still match (⌘K arrives as 'K' with caps)", () => {
	expect(termAction(ev("K", cmd))).toEqual({ kind: "clear" });
});

test("chords the terminal does not own stay with the app or the program", () => {
	// No ⌘: plain keys go to the program.
	expect(termAction(ev("k"))).toBeNull();
	expect(termAction(ev("Backspace"))).toBeNull();
	// ⌃⌘ and ⌥⌘ are app shortcuts (recent worktrees, spaces).
	expect(termAction(ev("1", { metaKey: true, ctrlKey: true }))).toBeNull();
	expect(termAction(ev("ArrowLeft", { metaKey: true, altKey: true }))).toBeNull();
	// ⇧⌘P (palette), ⇧⌘[ are not terminal actions.
	expect(termAction(ev("p", cmdShift))).toBeNull();
	// ⌘V is a paste event, ⌘W/⌘T/⌘N are menu items: never claimed here.
	for (const k of ["v", "w", "t", "n"]) expect(termAction(ev(k, cmd))).toBeNull();
});

test("font steps by one point inside 6–48 and resets to the configured size", () => {
	expect(fontStep(12.5, 12.5, "increase")).toBe(13.5);
	expect(fontStep(12.5, 12.5, "decrease")).toBe(11.5);
	expect(fontStep(48, 12.5, "increase")).toBe(48);
	expect(fontStep(6, 12.5, "decrease")).toBe(6);
	expect(fontStep(30, 12.5, "reset")).toBe(12.5);
});

test("option-as-alt: setting × side × key", () => {
	const table: [string, "left" | "right" | null, string, boolean][] = [
		["both", "left", "b", true],
		["both", "right", "b", true],
		["none", "left", "b", false],
		["left", "left", "b", true],
		["left", "right", "b", false],
		["right", "right", "b", true],
		["right", "left", "b", false],
		// A side that is not known (no ⌥ keydown seen) never counts as a match.
		["left", null, "b", false],
		// Keys with no macOS option character are Alt whatever the setting.
		["none", null, "ArrowLeft", true],
		["none", null, "Backspace", true],
		["left", "right", "ArrowRight", true],
	];
	for (const [setting, side, key, want] of table) {
		expect(optionActsAsAlt(setting, side, key), `${setting}/${side}/${key}`).toBe(want);
	}
});

test("keyToSend leaves ⌘, IME, modifiers and plain printable keys to their own paths", () => {
	const k = (key, over = {}) => ({ ...ev(key), code: "KeyA", isComposing: false, ...over });
	expect(keyToSend(k("a", { metaKey: true }), "none", null)).toBeNull();
	expect(keyToSend(k("a", { isComposing: true }), "none", null)).toBeNull();
	expect(keyToSend(k("Shift"), "none", null)).toBeNull();
	expect(keyToSend(k("a"), "none", null)).toBeNull();
	expect(keyToSend(k("ArrowUp", { shiftKey: true }), "none", null)).toEqual({
		key: "ArrowUp",
		shift: true,
		alt: false,
		ctrl: false,
	});
	expect(keyToSend(k("c", { ctrlKey: true }), "none", null)).toEqual({
		key: "c",
		shift: false,
		alt: false,
		ctrl: true,
	});
});

test("keyToSend turns ⌥ into Alt on the physical key when option_as_alt allows it", () => {
	const alt = (over = {}) => ({ ...ev("å", { altKey: true }), code: "KeyA", isComposing: false, ...over });
	expect(keyToSend(alt(), "both", null)).toEqual({ key: "a", shift: false, alt: true, ctrl: false });
	expect(keyToSend(alt({ shiftKey: true }), "left", "left")).toEqual({
		key: "A",
		shift: true,
		alt: true,
		ctrl: false,
	});
	// ⌥ composes a macOS character: text goes through `input`.
	expect(keyToSend(alt(), "left", "right")).toBeNull();
	expect(keyToSend(alt(), "none", "left")).toBeNull();
});

// Incident guard: a chord the terminal claims but the app keymap also binds would be taken by the
// app's capture handler before the terminal sees it (⇧⌘D was Changes). Every claimed chord must be
// unbound in the default keymap, or be on `terminalKeeps`' list.
test("terminal chords never collide with the default app keymap unless the terminal keeps them", async () => {
	const { DEFAULT_KEYMAP } = await import("./keys.ts");
	const { terminalKeeps } = await import("./termKeys.ts");
	const claimed: [string, boolean][] = [
		["f", false],
		["g", false],
		["g", true],
		["c", false],
		["a", false],
		["k", false],
		["=", false],
		["-", false],
		["0", false],
		["d", false],
		["d", true],
		["y", true],
		["ArrowUp", false],
		["ArrowDown", false],
		["Home", false],
		["End", false],
		["PageUp", false],
		["PageDown", false],
		["Backspace", false],
		["ArrowLeft", false],
		["ArrowRight", false],
	];
	const bound = new Set(Object.keys(DEFAULT_KEYMAP));
	for (const [key, shift] of claimed) {
		const stroke = `cmd-${shift ? "shift-" : ""}${key.length === 1 ? key : key.replace(/^Arrow/, "").toLowerCase()}`;
		const e = { key, metaKey: true, altKey: false, ctrlKey: false, shiftKey: shift };
		if (bound.has(stroke)) expect(terminalKeeps(e), stroke).toBe(true);
	}
});

// Incident guard: ⌘K was documented as Ghostty's clear_screen in a focused terminal, but the window's
// capture handler dispatched it to the app keymap (`atlas::Jump`) before the terminal saw it. Every chord
// the terminal claims AND the default keymap binds must be kept by the terminal.
test("⌘K and ⇧⌘D are kept by a focused terminal; app-only chords are not", async () => {
	const { terminalKeeps } = await import("./termKeys.ts");
	const e = (key, shiftKey = false) => ({ key, metaKey: true, altKey: false, ctrlKey: false, shiftKey });
	expect(terminalKeeps(e("k"))).toBe(true);
	expect(terminalKeeps(e("K"))).toBe(true);
	expect(terminalKeeps(e("d", true))).toBe(true);
	for (const k of ["t", "w", "n", ",", "l"]) expect(terminalKeeps(e(k)), k).toBe(false);
	expect(terminalKeeps(e("p", true))).toBe(false);
});

// Windows: in an xterm.js terminal Ctrl+C must stay ^C for the program unless text is selected, and copy/paste
// follow Windows Terminal (Ctrl+Shift+C/V, Ctrl+V); the app's shortcuts there are Ctrl+Shift chords.
test("xterm keys off macOS: copy with a selection, paste, ^C otherwise, Ctrl+Shift for the app", () => {
	const k = (key, m = {}) => ({ key, metaKey: false, altKey: false, ctrlKey: false, shiftKey: false, ...m });
	expect(xtermKey(k("c", { ctrlKey: true }), false, false)).toBeNull();
	expect(xtermKey(k("c", { ctrlKey: true }), true, false)).toBe("copy");
	expect(xtermKey(k("C", { ctrlKey: true, shiftKey: true }), false, false)).toBe("copy");
	expect(xtermKey(k("v", { ctrlKey: true }), false, false)).toBe("paste");
	expect(xtermKey(k("V", { ctrlKey: true, shiftKey: true }), false, false)).toBe("paste");
	expect(xtermKey(k("F", { ctrlKey: true, shiftKey: true }), false, false)).toBe("search");
	expect(xtermKey(k("T", { ctrlKey: true, shiftKey: true }), false, false)).toBe("app");
	expect(xtermKey(k("r", { ctrlKey: true }), false, false)).toBeNull();
	expect(xtermKey(k("Enter", { shiftKey: true }), false, false)).toBe("shiftEnter");
	expect(xtermKey(k("x"), false, false)).toBeNull();
});

test("xterm keys on macOS: ⌘F searches, other ⌘ chords are the app's, Ctrl stays the program's", () => {
	const k = (key, m = {}) => ({ key, metaKey: false, altKey: false, ctrlKey: false, shiftKey: false, ...m });
	expect(xtermKey(k("f", { metaKey: true }), false, true)).toBe("search");
	expect(xtermKey(k("t", { metaKey: true }), false, true)).toBe("app");
	expect(xtermKey(k("c", { ctrlKey: true }), true, true)).toBeNull();
	expect(xtermKey(k("v", { ctrlKey: true }), false, true)).toBeNull();
	expect(xtermKey(k("Enter", { shiftKey: true }), false, true)).toBe("shiftEnter");
});

// Layout zoom (issue #25) binds ⌘= ⌘- ⌘0 in the app keymap, and the window handler would take them before the
// terminal sees its font-size chords. A focused terminal must keep all four spellings (⌘+ is ⇧⌘= or ⌘ with "+").
test("a focused terminal keeps ⌘= ⌘+ ⇧⌘= ⌘- ⌘0 for its font size, not the layout zoom", async () => {
	const { terminalKeeps, termAction } = await import("./termKeys.ts");
	const e = (key, shiftKey = false) => ({ key, metaKey: true, altKey: false, ctrlKey: false, shiftKey });
	for (const [key, shift] of [
		["=", false],
		["+", false],
		["+", true],
		["-", false],
		["0", false],
	]) {
		expect(terminalKeeps(e(key, shift)), `${shift ? "⇧" : ""}⌘${key}`).toBe(true);
		expect(termAction(e(key, shift))?.kind, `${shift ? "⇧" : ""}⌘${key}`).toBe("font");
	}
	// ⇧⌘- and ⇧⌘0 are not font chords: the app keeps them
	expect(terminalKeeps(e("-", true))).toBe(false);
	expect(terminalKeeps(e("0", true))).toBe(false);
});
