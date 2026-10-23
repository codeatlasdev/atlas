// @ts-nocheck bun test runtime types are not part of the app tsconfig
// A wrong byte here types the wrong thing into a user's shell through the mirror (Ctrl+C must interrupt, not print c).
import { describe, expect, test } from "bun:test";
import { keyBytes } from "./keys.ts";

describe("keyBytes", () => {
	test("printable characters, unicode included, pass through", () => {
		expect(keyBytes({ key: "a" })).toBe("a");
		expect(keyBytes({ key: "A", shift: true })).toBe("A");
		expect(keyBytes({ key: "ç" })).toBe("ç");
		expect(keyBytes({ key: "😀" })).toBe("😀");
	});

	test("control keys produce the codes a shell acts on", () => {
		expect(keyBytes({ key: "c", ctrl: true })).toBe("\x03");
		expect(keyBytes({ key: "d", ctrl: true })).toBe("\x04");
		expect(keyBytes({ key: "[", ctrl: true })).toBe("\x1b");
		expect(keyBytes({ key: " ", ctrl: true })).toBe("\0");
		expect(keyBytes({ key: "1", ctrl: true })).toBeNull();
	});

	test("named keys use the terminal's sequences; Alt prefixes ESC", () => {
		expect(keyBytes({ key: "Enter" })).toBe("\r");
		expect(keyBytes({ key: "Backspace" })).toBe("\x7f");
		expect(keyBytes({ key: "ArrowUp" })).toBe("\x1b[A");
		expect(keyBytes({ key: "Tab", shift: true })).toBe("\x1b[Z");
		expect(keyBytes({ key: "b", alt: true })).toBe("\x1bb");
	});

	test("⌘ shortcuts and bare modifiers never reach the pty", () => {
		expect(keyBytes({ key: "c", meta: true })).toBeNull();
		for (const key of ["Shift", "Control", "Alt", "Meta", "F5", "CapsLock"])
			expect(keyBytes({ key })).toBeNull();
	});
});
