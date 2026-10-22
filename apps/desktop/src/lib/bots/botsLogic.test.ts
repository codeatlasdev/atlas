// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, test, expect } from "bun:test";
import { roleTag, previewIcon, groupTasks, firstUnreadIndex, mergeActivity } from "./botsLogic.ts";

describe("roleTag", () => {
	test("extracts token before colon", () => {
		expect(roleTag("Engineer: do code review")).toBe("Engineer");
	});

	test("extracts multi-word before colon if ≤20 chars", () => {
		expect(roleTag("Head of Growth: analyze")).toBe("Head of Growth");
	});

	test("PascalCase single word no colon", () => {
		expect(roleTag("DevOps")).toBe("DevOps");
	});

	test("long role with no colon and no PascalCase first word", () => {
		expect(roleTag("You are a helpful bot")).toBe("");
	});

	test("empty string", () => {
		expect(roleTag("")).toBe("");
	});

	test("colon but token too long (>20 chars) falls back to first word check", () => {
		// "ThisIsAVeryLongRoleNameHere" is 26 chars before colon, >20; first word same, >15 → ""
		expect(roleTag("ThisIsAVeryLongRoleNameHere: something")).toBe("");
	});

	test("AllCaps single word ≤15 chars returned", () => {
		expect(roleTag("CEO")).toBe("CEO");
	});
});

describe("previewIcon", () => {
	test("needs_you state → ⚠", () => {
		expect(previewIcon(undefined, "needs_you")).toBe("⚠");
	});

	test("last done → ✓", () => {
		expect(previewIcon({ state: "done", ts: 1 }, "idle")).toBe("✓");
	});

	test("last failed → ✕", () => {
		expect(previewIcon({ state: "failed", ts: 1 }, "idle")).toBe("✕");
	});

	test("working state → empty string", () => {
		expect(previewIcon(undefined, "working")).toBe("");
	});

	test("no last, no state → empty string", () => {
		expect(previewIcon()).toBe("");
	});

	test("needs_you takes priority over last.state", () => {
		expect(previewIcon({ state: "done", ts: 1 }, "needs_you")).toBe("⚠");
	});
});

describe("groupTasks", () => {
	const base = { id: "", source: "desktop" as const, prompt: "", state: "done" as const };

	test("empty array → empty result", () => {
		expect(groupTasks([])).toEqual([]);
	});

	test("single task → groupStart true", () => {
		const tasks = [{ ...base, id: "1", ts: 1000 }];
		const result = groupTasks(tasks);
		expect(result).toHaveLength(1);
		expect(result[0].groupStart).toBe(true);
	});

	test("two tasks <7min apart → second groupStart false", () => {
		const tasks = [
			{ ...base, id: "1", ts: 0 },
			{ ...base, id: "2", ts: 5 * 60 * 1000 },
		];
		const result = groupTasks(tasks);
		expect(result[0].groupStart).toBe(true);
		expect(result[1].groupStart).toBe(false);
	});

	test("two tasks >7min apart → second groupStart true", () => {
		const tasks = [
			{ ...base, id: "1", ts: 0 },
			{ ...base, id: "2", ts: 8 * 60 * 1000 },
		];
		const result = groupTasks(tasks);
		expect(result[0].groupStart).toBe(true);
		expect(result[1].groupStart).toBe(true);
	});

	test("exactly 7min gap → second groupStart false (not strictly greater)", () => {
		const tasks = [
			{ ...base, id: "1", ts: 0 },
			{ ...base, id: "2", ts: 7 * 60 * 1000 },
		];
		const result = groupTasks(tasks);
		expect(result[1].groupStart).toBe(false);
	});

	test("preserves task references", () => {
		const t = { ...base, id: "x", ts: 100 };
		const result = groupTasks([t]);
		expect(result[0].task).toBe(t);
	});
});

describe("firstUnreadIndex", () => {
	const base = { id: "", source: "desktop" as const, prompt: "", state: "done" as const };
	const tasks = [
		{ ...base, id: "1", ts: 100 },
		{ ...base, id: "2", ts: 200 },
		{ ...base, id: "3", ts: 300 },
	];

	test("lastSeenTs=0 → -1 (never seen)", () => {
		expect(firstUnreadIndex(tasks, 0)).toBe(-1);
	});

	test("all tasks seen → -1", () => {
		expect(firstUnreadIndex(tasks, 300)).toBe(-1);
	});

	test("all tasks seen (ts > max) → -1", () => {
		expect(firstUnreadIndex(tasks, 9999)).toBe(-1);
	});

	test("first task unread → 0", () => {
		expect(firstUnreadIndex(tasks, 50)).toBe(0);
	});

	test("middle task unread → correct index", () => {
		expect(firstUnreadIndex(tasks, 150)).toBe(1);
	});

	test("last task unread → 2", () => {
		expect(firstUnreadIndex(tasks, 250)).toBe(2);
	});

	test("empty tasks → -1", () => {
		expect(firstUnreadIndex([], 100)).toBe(-1);
	});
});

describe("mergeActivity", () => {
	const base = { id: "", source: "desktop" as const, prompt: "", state: "done" as const };

	test("empty array → empty", () => {
		expect(mergeActivity([])).toEqual([]);
	});

	test("single entry → same", () => {
		const entries = [{ botId: "a", botName: "A", hostKey: "", task: { ...base, id: "1", ts: 100 } }];
		const result = mergeActivity(entries);
		expect(result).toHaveLength(1);
		expect(result[0].task.ts).toBe(100);
	});

	test("two entries from different bots sorted newest first", () => {
		const entries = [
			{ botId: "a", botName: "A", hostKey: "", task: { ...base, id: "1", ts: 100 } },
			{ botId: "b", botName: "B", hostKey: "", task: { ...base, id: "2", ts: 500 } },
		];
		const result = mergeActivity(entries);
		expect(result[0].task.ts).toBe(500);
		expect(result[1].task.ts).toBe(100);
	});

	test("does not mutate input array", () => {
		const entries = [
			{ botId: "a", botName: "A", hostKey: "", task: { ...base, id: "1", ts: 100 } },
			{ botId: "b", botName: "B", hostKey: "", task: { ...base, id: "2", ts: 500 } },
		];
		mergeActivity(entries);
		expect(entries[0].task.ts).toBe(100);
	});

	test("stable sort preserves order of equal ts", () => {
		const entries = [
			{ botId: "a", botName: "A", hostKey: "", task: { ...base, id: "1", ts: 200 } },
			{ botId: "b", botName: "B", hostKey: "", task: { ...base, id: "2", ts: 200 } },
		];
		const result = mergeActivity(entries);
		// Both have same ts; stable sort preserves original relative order
		expect(result[0].botId).toBe("a");
		expect(result[1].botId).toBe("b");
	});
});
