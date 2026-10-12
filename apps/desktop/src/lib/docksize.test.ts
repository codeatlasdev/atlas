// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { clampSize } from "./docksize.ts";

test("sizes stay within the dock's limits", () => {
	expect(clampSize("left", 10)).toBe(180);
	expect(clampSize("left", 9999)).toBe(520);
	expect(clampSize("right", 300)).toBe(300);
});

test("a ceiling (window room) wins over the max but never goes below the min", () => {
	expect(clampSize("right", 500, 400)).toBe(400);
	expect(clampSize("right", 500, 50)).toBe(220);
	expect(clampSize("bottom", 900, 400)).toBe(400);
});

test("corrupt persisted values fall back to the minimum", () => {
	expect(clampSize("left", "wide")).toBe(180);
	expect(clampSize("right", NaN)).toBe(220);
	expect(clampSize("bottom", undefined)).toBe(80);
});
