// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, describe, expect, test } from "bun:test";
import { corner } from "./corner.svelte.ts";
import { MORE_ID } from "./cornerPlan.ts";
let off = [];
const claim = (id, kind) => {
	const release = corner.claim(id, kind);
	off.push(release);
	return release;
};
afterEach(() => {
	off.forEach((r) => r());
	off = [];
	corner.reset();
});

describe("the corner store: claims in, plan out", () => {
	test("a claim is on screen until it is released", () => {
		const release = claim("t1", "toast");
		expect(corner.shown("t1")).toBe(true);
		release();
		expect(corner.shown("t1")).toBe(false);
	});

	test("claiming the same id twice is one card, and releasing the first copy does not remove the second", () => {
		const first = claim("t1", "toast");
		claim("t1", "toast");
		first();
		expect(corner.shown("t1")).toBe(true);
	});

	test("a card is placed by its position in the plan: nearest the corner is 0", () => {
		claim("old", "toast");
		claim("new", "toast");
		expect(corner.slot("new")).toBe(0);
		expect(corner.slot("old")).toBe(1);
	});

	test("a card that waits is not shown and has no slot", () => {
		claim("tip", "suggestion");
		claim("t", "toast");
		expect(corner.shown("tip")).toBe(false);
		expect(corner.slot("tip")).toBe(-1);
	});

	test("an id that is claimed again after release arrives as a new card (nearest the corner)", () => {
		const a = claim("a", "toast");
		claim("b", "toast");
		a();
		claim("a", "toast");
		expect(corner.slot("a")).toBe(0);
	});
});

describe("checklist folding follows the toasts and the user's choice", () => {
	test("the toast folds it, and when the toast leaves it is open again", () => {
		claim("list", "checklist");
		expect(corner.plan.checklist.collapsed).toBe(false);
		const release = claim("t", "toast");
		expect(corner.plan.checklist.collapsed).toBe(true);
		release();
		expect(corner.plan.checklist.collapsed).toBe(false);
	});

	test("clicking the folded header during a toast opens it (pinned) and the pin clears when the corner is free", () => {
		claim("list", "checklist");
		const release = claim("t", "toast");
		corner.toggleChecklist();
		expect(corner.plan.checklist.collapsed).toBe(false);
		expect(corner.checklistOpen).toBe(true);
		release();
		expect(corner.checklistPinned).toBe(false);
		expect(corner.plan.checklist.collapsed).toBe(false);
	});

	test("closing it by hand is remembered through toasts", () => {
		claim("list", "checklist");
		corner.toggleChecklist();
		expect(corner.checklistOpen).toBe(false);
		const release = claim("t", "toast");
		release();
		expect(corner.plan.checklist.collapsed).toBe(true);
	});
});

describe("the +N more row", () => {
	test("opening the row shows every toast, and it folds back by itself when only three are left", () => {
		const releases = [1, 2, 3, 4, 5].map((i) => claim(`t${i}`, "toast"));
		expect(corner.plan.order).toContain(MORE_ID);
		expect(corner.shown("t1")).toBe(false);
		corner.toggleMore();
		expect(corner.shown("t1")).toBe(true);
		releases[0]();
		releases[1]();
		expect(corner.toastsExpanded).toBe(false);
	});
});
