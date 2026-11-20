// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { clock, endsDictationHold, isDictationKey } from "./key.ts";

const down = (over = {}) => ({ code: "KeyM", shiftKey: true, metaKey: false, ctrlKey: false, altKey: false, ...over });

describe("the microphone chord", () => {
	test("⌘⇧M on a Mac, Ctrl+Shift+M elsewhere, each only with its own modifier", () => {
		expect(isDictationKey(down({ metaKey: true }), true)).toBe(true);
		expect(isDictationKey(down({ ctrlKey: true }), true)).toBe(false);
		expect(isDictationKey(down({ ctrlKey: true }), false)).toBe(true);
		expect(isDictationKey(down({ metaKey: true }), false)).toBe(false);
	});

	test("it needs the shift, the right key and no Alt (Alt types symbols on a Mac)", () => {
		expect(isDictationKey(down({ ctrlKey: true, shiftKey: false }), false)).toBe(false);
		expect(isDictationKey(down({ ctrlKey: true, code: "KeyN" }), false)).toBe(false);
		expect(isDictationKey(down({ ctrlKey: true, altKey: true }), false)).toBe(false);
	});

	test("ctrl+cmd together is not the chord on a Mac (⌃⌘M is the system's)", () => {
		expect(isDictationKey(down({ metaKey: true, ctrlKey: true }), true)).toBe(false);
	});

	test("any key of the chord coming up ends a hold; other keys do not", () => {
		for (const [code, key] of [["KeyM", "m"], ["ShiftLeft", "Shift"], ["MetaLeft", "Meta"], ["ControlLeft", "Control"]]) {
			expect(endsDictationHold({ code, key })).toBe(true);
		}
		expect(endsDictationHold({ code: "KeyA", key: "a" })).toBe(false);
		expect(endsDictationHold({ code: "AltLeft", key: "Alt" })).toBe(false);
	});
});

describe("the elapsed clock", () => {
	test("m:ss, seconds padded, minutes unbounded below the 5 minute cap", () => {
		expect(clock(0)).toBe("0:00");
		expect(clock(999)).toBe("0:00");
		expect(clock(65_000)).toBe("1:05");
		expect(clock(300_000)).toBe("5:00");
	});
});
