// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { diff } from "./diff.ts";

test("identical text yields no output", () => {
	expect(diff("a\nb", "a\nb")).toBe("");
});

test("insertion", () => {
	expect(diff("a\nc", "a\nb\nc")).toBe("  a\n+ b\n  c");
});

test("removal", () => {
	expect(diff("a\nb\nc", "a\nc")).toBe("  a\n- b\n  c");
});
