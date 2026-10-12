// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { byAttention, gridStep, shellIn, shq } from "./panel.ts";

describe("shell quoting", () => {
	test("quotes single quotes safely", () => {
		expect(shq("a'b")).toBe(`'a'\\''b'`);
	});
	test("no dir means a plain terminal", () => {
		expect(shellIn(null)).toEqual([]);
	});
	test("dir is quoted into the cd", () => {
		expect(shellIn("apps/my web")[2]).toContain(`cd 'apps/my web' &&`);
	});
});

describe("byAttention", () => {
	test("failed first, then starting, running, others; ties by name", () => {
		const rows = [
			{ name: "z", state: "running" },
			{ name: "a", state: "stopped" },
			{ name: "b", state: "failed" },
			{ name: "c", state: "starting" },
			{ name: "a2", state: "running" },
		];
		expect(byAttention(rows).map((r) => r.name)).toEqual(["b", "c", "a2", "z", "a"]);
	});
	test("does not mutate its input", () => {
		const rows = [
			{ name: "b", state: "running" },
			{ name: "a", state: "failed" },
		];
		byAttention(rows);
		expect(rows[0].name).toBe("b");
	});
});

describe("gridStep", () => {
	test("moves by one and by row, clamped", () => {
		expect(gridStep("ArrowRight", 0, 5, 3)).toBe(1);
		expect(gridStep("ArrowRight", 4, 5, 3)).toBe(4);
		expect(gridStep("ArrowLeft", 0, 5, 3)).toBe(0);
		expect(gridStep("ArrowDown", 1, 5, 3)).toBe(4);
		expect(gridStep("ArrowDown", 3, 5, 3)).toBe(4);
		expect(gridStep("ArrowUp", 4, 5, 3)).toBe(1);
		expect(gridStep("ArrowUp", 1, 5, 3)).toBe(0);
	});
	test("Home/End and unknown keys", () => {
		expect(gridStep("Home", 3, 5, 3)).toBe(0);
		expect(gridStep("End", 0, 5, 3)).toBe(4);
		expect(gridStep("x", 2, 5, 3)).toBe(2);
	});
	test("empty grid", () => {
		expect(gridStep("ArrowRight", 0, 0, 3)).toBe(-1);
	});
});
