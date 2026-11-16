// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Incident (maintainer screenshot, 2026-11-15): an info toast ("Enable Claude Code integration") sat on the open
 * "Getting started" card in the bottom-right corner, with another width and another shadow, and a burst of toasts
 * could pile up without limit. The corner is now one plan: who is on screen, in which order, which card folds to its
 * header, which suggestion waits. These tests pin that plan; the DOM only draws it.
 */
import { describe, expect, test } from "bun:test";
import { MORE_ID, MAX_TOASTS, flipStart, planCorner } from "./cornerPlan.ts";

const claim = (id, kind, seq) => ({ id, kind, seq });
const ui = (o = {}) => ({ checklistOpen: true, checklistPinned: false, toastsExpanded: false, ...o });

describe("order: errors and actions first, then the checklist, then suggestions", () => {
	test("from the corner outward: toasts, the checklist, the suggestion", () => {
		// Claimed in the order the user would see them arrive: suggestion first, toast last.
		const plan = planCorner(
			[claim("tip", "suggestion", 1), claim("list", "checklist", 2), claim("saved", "toast", 3)],
			ui(),
		);
		expect(plan.order).toEqual(["saved", "list"]);
		// The suggestion waits while the corner has a toast.
		expect(plan.suggestion).toBeNull();
		expect(plan.queued).toEqual(["tip"]);
	});

	test("with no toast the suggestion sits above the checklist", () => {
		const plan = planCorner([claim("list", "checklist", 1), claim("tip", "suggestion", 2)], ui());
		expect(plan.order).toEqual(["list", "tip"]);
		expect(plan.suggestion).toBe("tip");
	});

	test("the newest toast is nearest the corner", () => {
		const plan = planCorner([claim("a", "toast", 1), claim("b", "toast", 2), claim("c", "toast", 3)], ui());
		expect(plan.order).toEqual(["c", "b", "a"]);
	});

	test("an empty corner is empty", () => {
		const plan = planCorner([], ui());
		expect(plan.order).toEqual([]);
		expect(plan.checklist).toBeNull();
		expect(plan.suggestion).toBeNull();
		expect(plan.hiddenToasts).toBe(0);
	});
});

describe("at most three toasts; the rest fold into a +N more row", () => {
	const burst = (n, kind = "toast") => Array.from({ length: n }, (_, i) => claim(`t${i}`, kind, i + 1));

	test("three toasts show all three and no row", () => {
		const plan = planCorner(burst(MAX_TOASTS), ui());
		expect(plan.toasts).toHaveLength(3);
		expect(plan.hiddenToasts).toBe(0);
		expect(plan.order).not.toContain(MORE_ID);
	});

	test("five toasts show the three newest and a row counting two", () => {
		const plan = planCorner(burst(5), ui());
		expect(plan.toasts).toEqual(["t4", "t3", "t2"]);
		expect(plan.hiddenToasts).toBe(2);
		expect(plan.order).toEqual(["t4", "t3", "t2", MORE_ID]);
	});

	test("an error is never the one folded away: it outranks newer plain toasts", () => {
		const plan = planCorner(
			[
				claim("err", "alert", 1),
				claim("a", "toast", 2),
				claim("b", "toast", 3),
				claim("c", "toast", 4),
				claim("d", "toast", 5),
			],
			ui(),
		);
		expect(plan.toasts).toContain("err");
		expect(plan.toasts).toEqual(["d", "c", "err"]);
		expect(plan.hiddenToasts).toBe(2);
	});

	test("expanded, every toast shows and the row stays as the way back", () => {
		const plan = planCorner(burst(5), ui({ toastsExpanded: true }));
		expect(plan.toasts).toHaveLength(5);
		expect(plan.hiddenToasts).toBe(0);
		expect(plan.moreRow).toBe("less");
		expect(plan.order.at(-1)).toBe(MORE_ID);
	});

	test("expanded with only three toasts has nothing to collapse: no row", () => {
		const plan = planCorner(burst(3), ui({ toastsExpanded: true }));
		expect(plan.moreRow).toBeNull();
		expect(plan.order).not.toContain(MORE_ID);
	});

	test("the row counts toward the checklist collapse: hidden toasts are still toasts", () => {
		const plan = planCorner([...burst(4), claim("list", "checklist", 9)], ui());
		expect(plan.checklist).toEqual({ id: "list", collapsed: true });
	});
});

describe("the checklist folds to its header while a toast is up and comes back after", () => {
	const list = claim("list", "checklist", 1);

	test("alone and open: expanded", () => {
		expect(planCorner([list], ui()).checklist).toEqual({ id: "list", collapsed: false });
	});

	test("a toast folds it; with the toast gone it is expanded again", () => {
		const during = planCorner([list, claim("t", "toast", 2)], ui());
		expect(during.checklist.collapsed).toBe(true);
		const after = planCorner([list], ui());
		expect(after.checklist.collapsed).toBe(false);
	});

	test("an error folds it as well", () => {
		expect(planCorner([list, claim("e", "alert", 2)], ui()).checklist.collapsed).toBe(true);
	});

	test("one the user had closed stays closed when the corner frees up", () => {
		const closed = ui({ checklistOpen: false });
		expect(planCorner([list, claim("t", "toast", 2)], closed).checklist.collapsed).toBe(true);
		expect(planCorner([list], closed).checklist.collapsed).toBe(true);
	});

	test("a suggestion alone does not fold it", () => {
		expect(planCorner([list, claim("tip", "suggestion", 2)], ui()).checklist.collapsed).toBe(false);
	});

	test("the user can open it over a toast (pinned); otherwise the toast wins", () => {
		const toast = claim("t", "toast", 2);
		expect(planCorner([list, toast], ui({ checklistPinned: true })).checklist.collapsed).toBe(false);
		expect(planCorner([list, toast], ui({ checklistPinned: false })).checklist.collapsed).toBe(true);
	});

	test("pinning an item the user closed does not open it", () => {
		const plan = planCorner(
			[list, claim("t", "toast", 2)],
			ui({ checklistOpen: false, checklistPinned: true }),
		);
		expect(plan.checklist.collapsed).toBe(true);
	});
});

describe("suggestions: one at a time, in arrival order, only when the corner is calm", () => {
	test("two suggestions: the first one shows, the second queues", () => {
		const plan = planCorner([claim("claude", "suggestion", 1), claim("ghostty", "suggestion", 2)], ui());
		expect(plan.suggestion).toBe("claude");
		expect(plan.queued).toEqual(["ghostty"]);
		expect(plan.order).toEqual(["claude"]);
	});

	test("when the first is dismissed the next takes its turn", () => {
		const plan = planCorner([claim("ghostty", "suggestion", 2)], ui());
		expect(plan.suggestion).toBe("ghostty");
		expect(plan.queued).toEqual([]);
	});

	test("a toast defers the suggestion, and it returns when the toast leaves", () => {
		const tip = claim("tip", "suggestion", 1);
		expect(planCorner([tip, claim("t", "toast", 2)], ui()).suggestion).toBeNull();
		expect(planCorner([tip], ui()).suggestion).toBe("tip");
	});

	test("queue order is arrival order, not claim order in the array", () => {
		const plan = planCorner(
			[claim("late", "suggestion", 9), claim("early", "suggestion", 2), claim("mid", "suggestion", 5)],
			ui(),
		);
		expect(plan.suggestion).toBe("early");
		expect(plan.queued).toEqual(["mid", "late"]);
	});
});

describe("a second checklist claim cannot take two places", () => {
	test("only the first claimed checklist is placed", () => {
		const plan = planCorner([claim("b", "checklist", 2), claim("a", "checklist", 1)], ui());
		expect(plan.order).toEqual(["a"]);
		expect(plan.checklist.id).toBe("a");
	});
});

describe("flipStart: where a moved card begins so it glides instead of jumping", () => {
	test("a card that moved up by 40px starts 40px below its new place", () => {
		expect(flipStart(10, 50, 0)).toBe(40);
	});
	test("a card that moved down starts above", () => {
		expect(flipStart(50, 10, 0)).toBe(-40);
	});
	test("a card still gliding keeps its visual offset: no jump when moved again", () => {
		expect(flipStart(10, 50, 12)).toBe(52);
	});
	test("a move under half a pixel is not animated", () => {
		expect(flipStart(10, 10.3, 0)).toBe(0);
	});
});
