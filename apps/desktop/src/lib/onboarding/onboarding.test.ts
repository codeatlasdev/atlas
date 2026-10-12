// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { TOUR_STEPS, nextIndex, placeCard, prevIndex, reanchor, visibleSteps } from "./tour.ts";
import {
	CHECKLIST,
	EMPTY_STATE,
	complete,
	isFirstRun,
	itemsFromRecent,
	nextItem,
	parseState,
	progress,
} from "./checklist.ts";
import { buildSheet } from "./shortcuts.ts";

describe("tour sequencing", () => {
	test("steps with a missing anchor are skipped, order kept", () => {
		const present = new Set(["sidebar", "palette", "panel-kpis"]);
		expect(visibleSteps(TOUR_STEPS, (a) => present.has(a)).map((s) => s.anchor)).toEqual([
			"sidebar",
			"panel-kpis",
			"palette",
		]);
		expect(visibleSteps(TOUR_STEPS, () => false)).toEqual([]);
	});

	test("next ends the tour after the last step, back stops at the first", () => {
		expect(nextIndex(0, 3)).toBe(1);
		expect(nextIndex(2, 3)).toBeNull();
		expect(prevIndex(0)).toBe(0);
		expect(prevIndex(2)).toBe(1);
	});

	test("reanchor keeps the current step when the list shrinks around it", () => {
		const before = TOUR_STEPS.slice(0, 4);
		const after = before.filter((s) => s.id !== "spaces");
		expect(reanchor(after, "breadcrumb", 2)).toBe(1);
		// current step vanished: clamp to the closest valid index
		expect(reanchor(after, "spaces", 9)).toBe(after.length - 1);
		expect(reanchor([], undefined, 0)).toBe(0);
	});

	test("card flips when the preferred side has no room and stays inside the view", () => {
		const view = { w: 1000, h: 700 };
		const card = { w: 300, h: 140 };
		const nearRight = { x: 900, y: 300, w: 80, h: 30 };
		const flipped = placeCard(nearRight, card, view, "right");
		expect(flipped.side).toBe("left");
		expect(flipped.x + card.w).toBeLessThanOrEqual(view.w);
		const roomy = placeCard({ x: 100, y: 300, w: 80, h: 30 }, card, view, "right");
		expect(roomy.side).toBe("right");
		expect(roomy.x).toBe(100 + 80 + 14);
		const corner = placeCard({ x: 0, y: 0, w: 20, h: 20 }, card, view, "top");
		expect(corner.y).toBeGreaterThanOrEqual(12);
		expect(corner.x).toBeGreaterThanOrEqual(12);
	});
});

describe("checklist", () => {
	test("parseState drops unknown ids and bad shapes", () => {
		expect(parseState(undefined)).toEqual(EMPTY_STATE);
		expect(parseState("x")).toEqual(EMPTY_STATE);
		const s = parseState({
			done: true,
			checklist: { done: ["project", "bogus", "project", 4], dismissed: true },
		});
		expect(s.done).toBe(true);
		expect(s.toured).toBe(false);
		expect(s.checklist).toEqual({ done: ["project"], dismissed: true });
	});

	test("complete is idempotent and returns the same object when nothing changes", () => {
		const a = complete(EMPTY_STATE, "stack");
		expect(a.checklist.done).toEqual(["stack"]);
		expect(complete(a, "stack")).toBe(a);
		expect(EMPTY_STATE.checklist.done).toEqual([]);
	});

	test("progress and next item follow completion", () => {
		let s = EMPTY_STATE;
		expect(progress(s)).toEqual({ done: 0, total: CHECKLIST.length, all: false });
		expect(nextItem(s)?.id).toBe("project");
		for (const c of CHECKLIST) s = complete(s, c.id);
		expect(progress(s).all).toBe(true);
		expect(nextItem(s)).toBeUndefined();
	});

	test("first run needs a loaded empty list and no finished welcome", () => {
		expect(isFirstRun(EMPTY_STATE, false, 0)).toBe(false);
		expect(isFirstRun(EMPTY_STATE, true, 1)).toBe(false);
		expect(isFirstRun({ ...EMPTY_STATE, done: true }, true, 0)).toBe(false);
		expect(isFirstRun(EMPTY_STATE, true, 0)).toBe(true);
	});

	test("palette and file search complete from a newly run command only", () => {
		expect(itemsFromRecent(["atlas::Jump"], ["atlas::Jump"])).toEqual([]);
		expect(itemsFromRecent([], ["file_finder::Toggle"])).toEqual(["file"]);
		expect(
			itemsFromRecent(["file_finder::Toggle"], ["command_palette::Toggle", "file_finder::Toggle"]),
		).toEqual(["palette"]);
		expect(itemsFromRecent([], ["workspace::NewTerminal"])).toEqual([]);
		expect(itemsFromRecent(["a"], [])).toEqual([]);
	});
});

describe("shortcut sheet", () => {
	const cmds = [
		{ id: "atlas::Jump", label: "Jump to worktree or terminal" },
		{ id: "workspace::ToggleLeftDock", label: "Toggle left dock" },
		{ id: "pane::SplitRight", label: "Split right" },
		{ id: "atlas::Logs", label: "Logs" },
		{ id: "atlas::RecentWorktree1", label: "Recent worktree 1" },
		{ id: "atlas::RecentWorktree2", label: "Recent worktree 2" },
		{ id: "atlas::Unbound", label: "Unbound thing" },
		{ id: "x::Unknown", label: "Unknown area" },
	];
	const keys = {
		"atlas::Jump": ["⌘K"],
		"workspace::ToggleLeftDock": ["⌘B"],
		"pane::SplitRight": ["⌘\\"],
		"atlas::Logs": ["⌘L"],
		"atlas::RecentWorktree1": ["⌘1"],
		"atlas::RecentWorktree2": ["⌘2"],
		"x::Unknown": ["⌃X"],
	};
	const keysOf = (id) => keys[id] ?? [];

	test("groups bound commands in a fixed order and drops unbound ones", () => {
		const g = buildSheet(cmds, keysOf);
		expect(g.map((x) => x.title)).toEqual(["Go anywhere", "Panels", "Tabs and panes", "Project", "More"]);
		expect(g.flatMap((x) => x.rows.map((r) => r.id))).not.toContain("atlas::Unbound");
	});

	test("recent worktree rows collapse into one", () => {
		const rows = buildSheet(cmds, keysOf)[0].rows;
		expect(rows.filter((r) => r.label === "Recent worktree 1–6")).toHaveLength(1);
		expect(rows.some((r) => r.id === "atlas::RecentWorktree2")).toBe(false);
	});

	test("tab-N rows collapse into one in Tabs and panes", () => {
		const tabs = [1, 2, 8].map((n) => ({ id: `pane::ActivateItem${n}`, label: `Go to tab ${n}` }));
		const g = buildSheet(tabs, (id) => [`⌘${id.slice(-1)}`]);
		expect(g).toHaveLength(1);
		expect(g[0].title).toBe("Tabs and panes");
		expect(g[0].rows.map((r) => r.label)).toEqual(["Tab 1–8 (⌘9 last)"]);
	});

	test("search matches label or key and removes empty groups", () => {
		expect(buildSheet(cmds, keysOf, "split").map((g) => g.title)).toEqual(["Tabs and panes"]);
		expect(buildSheet(cmds, keysOf, "⌘b")[0].rows[0].id).toBe("workspace::ToggleLeftDock");
		expect(buildSheet(cmds, keysOf, "zzz")).toEqual([]);
	});
});
