// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	COLLAPSE_LINES,
	collapsedText,
	expandKey,
	fmtDate,
	fmtElapsed,
	fmtTokens,
	formatApprovalInput,
	needsDateSeparator,
	shouldCollapse,
	withKey,
} from "./taskView.ts";

describe("fmtElapsed", () => {
	test("seconds, minutes, hours, days", () => {
		expect(fmtElapsed(5_000)).toBe("5s");
		expect(fmtElapsed(59_999)).toBe("59s");
		expect(fmtElapsed(60_000)).toBe("1m");
		expect(fmtElapsed(59 * 60_000)).toBe("59m");
		expect(fmtElapsed(3_600_000 + 5 * 60_000)).toBe("1h 5m");
		expect(fmtElapsed(25 * 3_600_000)).toBe("1d 1h");
	});
});

describe("fmtTokens", () => {
	test("below 1000 is exact, above is k with one decimal", () => {
		expect(fmtTokens(999)).toBe("999");
		expect(fmtTokens(1000)).toBe("1.0k");
		expect(fmtTokens(12_345)).toBe("12.3k");
	});
});

describe("fmtDate", () => {
	const today = new Date(2026, 10, 13, 15, 0);
	test("today and yesterday are named", () => {
		expect(fmtDate(new Date(2026, 10, 13, 1, 0).getTime(), today)).toBe("Today");
		expect(fmtDate(new Date(2026, 10, 12, 23, 59).getTime(), today)).toBe("Yesterday");
	});
	test("older days use the month and day", () => {
		expect(fmtDate(new Date(2026, 9, 1).getTime(), today)).not.toMatch(/Today|Yesterday/);
	});
	test("yesterday works across a month boundary", () => {
		const first = new Date(2026, 10, 1, 9, 0);
		expect(fmtDate(new Date(2026, 9, 31, 20, 0).getTime(), first)).toBe("Yesterday");
	});
});

describe("needsDateSeparator", () => {
	const day = (d: number, h = 12) => ({ ts: new Date(2026, 10, d, h).getTime() });
	test("only at a group start that changes the day", () => {
		const tasks = [day(11), day(12), day(12, 13)];
		expect(needsDateSeparator(tasks, true, 1)).toBe(true);
		expect(needsDateSeparator(tasks, false, 1)).toBe(false);
		expect(needsDateSeparator(tasks, true, 2)).toBe(false);
	});
	test("never before the first task", () => {
		expect(needsDateSeparator([day(11)], true, 0)).toBe(false);
	});
});

describe("collapse", () => {
	const lines = (n: number) => Array.from({ length: n }, (_, i) => `l${i}`).join("\n");
	test("exactly the limit stays open, one more folds", () => {
		expect(shouldCollapse(lines(COLLAPSE_LINES))).toBe(false);
		expect(shouldCollapse(lines(COLLAPSE_LINES + 1))).toBe(true);
	});
	test("collapsedText keeps the first lines only", () => {
		const out = collapsedText(lines(COLLAPSE_LINES + 5));
		expect(out.split("\n")).toHaveLength(COLLAPSE_LINES);
		expect(out.startsWith("l0\n")).toBe(true);
	});
	test("a short text is returned whole", () => {
		expect(collapsedText("a\nb")).toBe("a\nb");
	});
});

describe("formatApprovalInput", () => {
	test("Bash shows the command", () => {
		expect(formatApprovalInput("Bash", { command: "ls -la" })).toBe("ls -la");
	});
	test("Write and Edit show the file path", () => {
		expect(formatApprovalInput("Write", { file_path: "/a/b.ts", content: "x" })).toBe("/a/b.ts");
		expect(formatApprovalInput("Edit", { file_path: "/a/c.ts" })).toBe("/a/c.ts");
	});
	test("other tools get key: value lines, capped at 6, objects as JSON", () => {
		const out = formatApprovalInput("Other", { a: "1", b: { x: 2 }, c: 3, d: 4, e: 5, f: 6, g: 7 });
		expect(out.split("\n")).toHaveLength(6);
		expect(out.split("\n")[0]).toBe("a: 1");
		expect(out.split("\n")[1]).toBe('b: {"x":2}');
	});
	test("Bash without a string command falls back to the lines", () => {
		expect(formatApprovalInput("Bash", { command: 5 })).toBe("command: 5");
	});
	test("non-object input is stringified", () => {
		expect(formatApprovalInput("Bash", "raw")).toBe("raw");
		expect(formatApprovalInput("Bash", null)).toBe("");
		expect(formatApprovalInput("Bash", undefined)).toBe("");
	});
});

describe("expanded set helpers", () => {
	test("expandKey builds distinct flags per task", () => {
		expect(expandKey.full("t1")).toBe("t1:full");
		expect(expandKey.dismissed("t1")).toBe("t1:dismissed");
	});
	test("withKey returns a new set and never mutates the old one", () => {
		const a = new Set(["x"]);
		const b = withKey(a, "y", true);
		expect(b).not.toBe(a);
		expect([...a]).toEqual(["x"]);
		expect([...b].sort()).toEqual(["x", "y"]);
		expect([...withKey(b, "x", false)]).toEqual(["y"]);
	});
});
