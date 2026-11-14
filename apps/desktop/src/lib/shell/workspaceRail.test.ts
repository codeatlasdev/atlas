// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, test, expect } from "bun:test";
import { inHotZone, leftRail, railToggleKey, SHOW_DELAY_MS, HIDE_DELAY_MS } from "./workspaceRail.ts";

describe("workspaceRail", () => {
	test("inHotZone: 0–6px triggers, 7px does not", () => {
		expect(inHotZone(0)).toBe(true);
		expect(inHotZone(6)).toBe(true);
		expect(inHotZone(7)).toBe(false);
		expect(inHotZone(-1)).toBe(false);
	});
	test("leftRail: beyond 56px = left", () => {
		expect(leftRail(56)).toBe(false);
		expect(leftRail(57)).toBe(true);
		expect(leftRail(0)).toBe(false);
	});
	test("railToggleKey: cmd-shift-b triggers, not in input", () => {
		const base = { key: "B", shiftKey: true, metaKey: true, ctrlKey: false, target: null };
		expect(railToggleKey(base)).toBe(true);
		expect(railToggleKey({ ...base, metaKey: false, ctrlKey: true })).toBe(true);
		expect(railToggleKey({ ...base, shiftKey: false })).toBe(false);
		const input = document.createElement("input");
		expect(railToggleKey({ ...base, target: input })).toBe(false);
	});
	test("SHOW_DELAY_MS is 120", () => expect(SHOW_DELAY_MS).toBe(120));
	test("HIDE_DELAY_MS is 300", () => expect(HIDE_DELAY_MS).toBe(300));
});
