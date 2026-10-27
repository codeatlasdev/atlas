// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { dockKeyFrom } from "./dockKey.ts";

const ev = (code, mods = {}, key = "") => ({
	code,
	key,
	ctrlKey: false,
	altKey: false,
	shiftKey: false,
	metaKey: false,
	...mods,
});

describe("dockKeyFrom: a key press recorded in Settings becomes the summon key", () => {
	test("⌃⌥Space is the default stroke", () => {
		expect(dockKeyFrom(ev("Space", { ctrlKey: true, altKey: true }), true)).toEqual({
			kind: "stroke",
			stroke: "ctrl-alt-space",
		});
	});

	test("Esc without modifiers cancels the recording; with a modifier it is a key like any other", () => {
		expect(dockKeyFrom(ev("Escape", {}, "Escape"), true)).toEqual({ kind: "cancel" });
		expect(dockKeyFrom(ev("Escape", { ctrlKey: true }, "Escape"), true)).toEqual({
			kind: "stroke",
			stroke: "ctrl-escape",
		});
	});

	test("a modifier alone keeps waiting", () => {
		expect(dockKeyFrom(ev("AltLeft", { altKey: true }), true)).toEqual({ kind: "wait" });
	});

	/** A global hotkey without ⌘/⌃/⌥ would swallow that key in every app; Rust refuses it too (`dock::accelerator`). */
	test("a bare or shift-only key is refused with the reason", () => {
		for (const e of [ev("KeyK"), ev("KeyK", { shiftKey: true }), ev("Space")]) {
			const r = dockKeyFrom(e, true);
			expect(r.kind).toBe("refuse");
			expect(r.reason).toMatch(/⌘, ⌃ or ⌥/);
		}
		expect(dockKeyFrom(ev("KeyK"), false).reason).toMatch(/Ctrl or Alt/);
	});

	/** A bare F5 would take F5 away from every app, so it is refused like any bare key (and by Rust). */
	test("an F-key alone is refused, with a modifier it is accepted", () => {
		expect(dockKeyFrom(ev("F5"), true).kind).toBe("refuse");
		expect(dockKeyFrom(ev("F5", { altKey: true }), true)).toEqual({ kind: "stroke", stroke: "alt-f5" });
	});

	/** Rust accepts exactly the strokes the keymap grammar writes: the Windows key chord is the system's. */
	test("a key the grammar cannot name keeps waiting", () => {
		expect(dockKeyFrom(ev("IntlBackslash", { ctrlKey: true }), true)).toEqual({ kind: "wait" });
	});
});
