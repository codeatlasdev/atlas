// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { firstMove, liftAt, restAt } from "./gestureRun.ts";
import type { ScrollSample } from "../shell/trace.ts";

const ev = (t: number, phase: ScrollSample["phase"], dx = 0): ScrollSample => ({
	t,
	phase,
	momentum: "none",
	dx,
	dy: 0,
	precise: true,
	x: 0,
	y: 0,
});

test("liftAt is the last finger-driven end of the trace (momentum and later gestures aside)", () => {
	expect(liftAt([ev(0, "began"), ev(8, "changed", -5), ev(40, "ended")])).toBe(40);
	expect(liftAt([ev(0, "began"), ev(40, "ended"), ev(60, "began"), ev(120, "ended")])).toBe(120);
	expect(liftAt([ev(0, "began")])).toBe(0);
});

test("restAt is the first sample after which the layer never leaves its final position by more than 0.001", () => {
	const s = [0, 0.3, 0.8, 0.97, 0.9995, 1, 1, 1].map((x, i) => ({ t: i * 50, x: -x }));
	expect(restAt(s)).toBe(200);
	expect(restAt([{ t: 0, x: 0 }])).toBe(0);
});

test("firstMove is the time from the event that claimed the gesture to the first frame where a layer moved", () => {
	const s = [0, 0, 0.0001, 0.02, 0.05].map((x, i) => ({ t: i * 16, x: -x }));
	expect(firstMove(s, 0)).toBe(48);
	// the trace's own lead-in (mayBegin/began, the 15 pt gate) is not the app's latency: measure from the claim
	expect(firstMove(s, 32)).toBe(16);
	expect(
		firstMove(
			[
				{ t: 0, x: 0 },
				{ t: 16, x: 0 },
			],
			0,
		),
	).toBe(Infinity);
	// a frame that painted before the claim cannot give a negative latency
	expect(firstMove(s, 60)).toBe(0);
});

import { frameSamples } from "./gestureRun.ts";

test("frame samples come from the engine's frames: continuous across the commit, never the DOM layer that changes role", () => {
	const frames = [
		{ t: 10, seq: 1, amount: 0.1, stage: "track" },
		{ t: 30, seq: 1, amount: 0.6, stage: "track" },
		{ t: 50, seq: 1, amount: 0.9, stage: "settle" },
		{ t: 70, seq: 1, amount: 1, stage: "rest" },
	];
	// x is the origin layer's position in pages: -amount, monotone toward -1 through the commit
	expect(frameSamples(frames).map((s) => s.x)).toEqual([-0.1, -0.6, -0.9, -1]);
	// a new generation rebases on the landed page: its frames start from 0 again, offset by what is already done
	const next = [...frames, { t: 200, seq: 2, amount: 0.2, stage: "track" }];
	expect(frameSamples(next).at(-1)!.x).toBeCloseTo(-1.2, 9);
	// a cancelled generation lands back on 0: the next one is not offset
	const cancelled = [
		{ t: 0, seq: 1, amount: 0.3, stage: "track" },
		{ t: 20, seq: 1, amount: 0, stage: "rest" },
		{ t: 90, seq: 2, amount: 0.1, stage: "track" },
	];
	expect(frameSamples(cancelled).at(-1)!.x).toBeCloseTo(-0.1, 9);
});

import { NativeCount, endStateProblems } from "./gestureRun.ts";

test("native park/restore counts follow the visible flag of the bounds calls, per view", () => {
	const n = new NativeCount();
	n.see("term_bounds", { tab: "t1", visible: true });
	n.see("term_bounds", { tab: "t1", visible: false }); // parked for the gesture
	n.see("term_bounds", { tab: "t1", visible: false }); // a repeat is not a second park
	n.see("browser_bounds", { tab: "b1", visible: true });
	n.see("term_bounds", { tab: "t1", visible: true }); // restored
	n.see("other_cmd", { visible: false });
	expect(n.result()).toEqual({ parked: 1, restored: 1 });
	// A view left parked at the end is the violation `assess` reports.
	n.see("browser_bounds", { tab: "b1", visible: false });
	expect(n.result()).toEqual({ parked: 2, restored: 1 });
});

test("end state: exactly one current layer, it is the selected space, and nothing keeps a stale transform", () => {
	const ok = {
		selected: "b",
		layers: [
			{ id: "a", role: "near", transform: "" },
			{ id: "b", role: "current", transform: "" },
		],
		overlay: false,
	};
	expect(endStateProblems(ok)).toEqual([]);
	expect(endStateProblems({ ...ok, selected: "a" })).toContain("current layer is b but the selection is a");
	expect(
		endStateProblems({ ...ok, layers: [{ id: "b", role: "current", transform: "translate3d(-4px,0,0)" }] }),
	).toContain("layer b keeps a transform at rest");
	expect(endStateProblems({ ...ok, overlay: true })).toContain("the native overlay is still up");
	expect(
		endStateProblems({
			...ok,
			layers: [
				{ id: "a", role: "current", transform: "" },
				{ id: "b", role: "current", transform: "" },
			],
		}),
	).toContain("2 current layers");
});

import { prepareTabs } from "./gestureRun.ts";

function fakeWs(initial: string[]) {
	const g = { id: "g1", order: [...initial], activeTab: initial[0] ?? null };
	return {
		g,
		ws: {
			selected: { id: "w" },
			focusedGroup: () => g,
			openTab: (_w, p, gid) => {
				const id = `n${g.order.length}`;
				g.order.push(id);
				g.activeTab = id;
				void p;
				void gid;
				return { id };
			},
			activate: (id) => (g.activeTab = id),
		},
	};
}

test("prepareTabs fills the group up to the count and activates the middle tab, so both directions exist", () => {
	const { g, ws } = fakeWs(["t0"]);
	const r = prepareTabs(ws, 5);
	expect(r.order.length).toBe(5);
	expect(g.activeTab).toBe(r.order[2]);
	expect(r.group).toBe("g1");
});

test("prepareTabs keeps existing tabs and never adds past the count", () => {
	const { g, ws } = fakeWs(["a", "b", "c", "d", "e", "f"]);
	const r = prepareTabs(ws, 4);
	expect(r.order).toEqual(["a", "b", "c", "d", "e", "f"]);
	expect(g.activeTab).toBe("d");
});

test("prepareTabs refuses to guess a worktree", () => {
	const { ws } = fakeWs(["a"]);
	expect(() => prepareTabs({ ...ws, selected: undefined }, 3)).toThrow("no worktree selected");
});

import { claimAt } from "./gestureRun.ts";

test("claimAt is the time of the sample that opens the gate: 15 pt of travel, horizontal at least 2x vertical", () => {
	const t = (dx: number, dy = 0, at = 0): ScrollSample => ({ ...ev(at, "changed", dx), dy });
	expect(claimAt([ev(0, "began"), t(-8, 0, 8), t(-8, 0, 16), t(-8, 0, 24)])).toBe(16);
	// vertical dominates: never claimed
	expect(claimAt([ev(0, "began"), t(-8, -30, 8), t(-8, -30, 16), t(-30, -30, 24)])).toBeNull();
	// a wheel mouse is not a swipe
	expect(claimAt([{ ...t(-40, 0, 8), precise: false }])).toBeNull();
	// momentum events never count
	expect(claimAt([{ ...t(-40, 0, 8), momentum: "changed" }])).toBeNull();
});
