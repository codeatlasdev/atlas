// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { fontStep, optionActsAsAlt, termAction } from "./termKeys.ts";

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
