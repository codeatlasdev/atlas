// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { accentOf, directionOf, hueOf, neighbour, spreadHues, tintOf } from "./space.ts";
import { MAX_HISTORY, canGo, emptyHistory, step, visit } from "./history.ts";
import { inHotZone, inPanel, peekAction } from "./peek.ts";

test("a space's hue is stable, in range, and differs between ids", () => {
	expect(hueOf("proj-a")).toBe(hueOf("proj-a"));
	for (const id of ["a", "b", "/Users/x/code/app", ""]) {
		expect(hueOf(id)).toBeGreaterThanOrEqual(0);
		expect(hueOf(id)).toBeLessThan(360);
	}
	expect(hueOf("shop")).not.toBe(hueOf("atlas"));
});

test("accent differs by theme lightness and honours a hue override", () => {
	expect(accentOf("x", false)).not.toBe(accentOf("x", true));
	expect(accentOf("x", false, 200)).toBe("oklch(0.62 0.17 200)");
	expect(tintOf("x", true, 10)).toContain(" 10 / ");
});

test("spreadHues keeps spaces visually apart and is deterministic", () => {
	const ids = Array.from({ length: 10 }, (_, i) => `repo-${i}`);
	const a = spreadHues(ids);
	expect(spreadHues(ids)).toEqual(a);
	const hues = Object.values(a);
	for (let i = 0; i < hues.length; i++)
		for (let j = i + 1; j < hues.length; j++) {
			const d = Math.abs(hues[i] - hues[j]);
			expect(Math.min(d, 360 - d)).toBeGreaterThanOrEqual(24);
		}
});

test("neighbour wraps and handles a single or unknown space", () => {
	const ids = ["a", "b", "c"];
	expect(neighbour(ids, "a", 1)).toBe("b");
	expect(neighbour(ids, "c", 1)).toBe("a");
	expect(neighbour(ids, "a", -1)).toBe("c");
	expect(neighbour(ids, null, 1)).toBe("a");
	expect(neighbour(ids, "gone", -1)).toBe("c");
	expect(neighbour(["a"], "a", 1)).toBeNull();
	expect(neighbour([], null, 1)).toBeNull();
});

test("switch direction follows the sidebar order", () => {
	const ids = ["a", "b", "c"];
	expect(directionOf(ids, "a", "c")).toBe(1);
	expect(directionOf(ids, "c", "a")).toBe(-1);
	expect(directionOf(ids, null, "b")).toBe(1);
});

test("history: visits, back/forward, dedupe, forward is dropped on a new visit", () => {
	let h = emptyHistory();
	const alive = () => true;
	h = visit(h, { worktreeId: "w1", tabId: "t1" });
	h = visit(h, { worktreeId: "w1", tabId: "t1" });
	h = visit(h, { worktreeId: "w1", tabId: "t2" });
	h = visit(h, { worktreeId: "w2", tabId: "t3" });
	expect(h.entries).toHaveLength(3);
	expect(canGo(h, 1, alive)).toBe(false);
	const back = step(h, -1, alive);
	expect(back.to).toEqual({ worktreeId: "w1", tabId: "t2" });
	expect(canGo(back.history, 1, alive)).toBe(true);
	const branched = visit(back.history, { worktreeId: "w9", tabId: null });
	expect(branched.entries.map((e) => e.worktreeId)).toEqual(["w1", "w1", "w9"]);
});

test("history skips closed targets and is bounded", () => {
	let h = emptyHistory();
	h = visit(h, { worktreeId: "a", tabId: "1" });
	h = visit(h, { worktreeId: "gone", tabId: "2" });
	h = visit(h, { worktreeId: "b", tabId: "3" });
	const r = step(h, -1, (v) => v.worktreeId !== "gone");
	expect(r.to.worktreeId).toBe("a");
	expect(step(r.history, -1, () => true)).toBeNull();
	let big = emptyHistory();
	for (let i = 0; i < MAX_HISTORY + 20; i++) big = visit(big, { worktreeId: "w", tabId: `${i}` });
	expect(big.entries).toHaveLength(MAX_HISTORY);
	expect(big.at).toBe(MAX_HISTORY - 1);
});

test("edge peek: hot zone, panel hit-test and intent transitions", () => {
	expect(inHotZone("left", 4, 1000)).toBe(true);
	expect(inHotZone("left", 20, 1000)).toBe(false);
	expect(inHotZone("right", 995, 1000)).toBe(true);
	expect(inPanel("left", 200, 1000, 260)).toBe(true);
	expect(inPanel("left", 400, 1000, 260)).toBe(false);
	expect(inPanel("right", 800, 1000, 260)).toBe(true);
	expect(peekAction({ open: false }, { t: "move", hot: true, over: false })).toEqual({ do: "schedule-open" });
	expect(peekAction({ open: false }, { t: "move", hot: false, over: false })).toEqual({ do: "cancel" });
	expect(peekAction({ open: true }, { t: "move", hot: false, over: true })).toEqual({ do: "cancel" });
	expect(peekAction({ open: true }, { t: "move", hot: false, over: false })).toEqual({
		do: "schedule-close",
	});
	expect(peekAction({ open: true }, { t: "force-close" })).toEqual({ do: "close-now" });
});
