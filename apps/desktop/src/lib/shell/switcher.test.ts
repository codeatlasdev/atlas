// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { openSwitcher, pickSwitcher, stepSwitcher } from "./switcher.ts";

test("the switcher needs two tabs, starts on the previous one, ⌃⇧Tab on the oldest", () => {
	expect(openSwitcher(["a"], 1)).toBeNull();
	expect(pickSwitcher(openSwitcher(["a", "b", "c"], 1))).toBe("b");
	expect(pickSwitcher(openSwitcher(["a", "b", "c"], -1))).toBe("c");
});

test("holding ⌃ and tabbing walks the list and wraps", () => {
	let s = openSwitcher(["a", "b", "c"], 1);
	s = stepSwitcher(s, 1);
	expect(pickSwitcher(s)).toBe("c");
	s = stepSwitcher(s, 1);
	expect(pickSwitcher(s)).toBe("a");
	expect(pickSwitcher(stepSwitcher(s, -1))).toBe("c");
});
