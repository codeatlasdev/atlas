// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { fmtTokens, usageBuckets, usageTotals } from "./usage.ts";

const DAY = 24 * 60 * 60 * 1000;
// Noon local time, so "today" and "yesterday" are unambiguous in any timezone.
const NOW = new Date(2026, 10, 13, 12, 0, 0).getTime();
const rec = (msAgo: number, input?: number, output?: number) => ({
	crewmate_id: "c1",
	engine: "omp",
	ts: Math.floor((NOW - msAgo) / 1000),
	input_tokens: input,
	output_tokens: output,
});

describe("usageBuckets", () => {
	test("no records: 30 empty buckets", () => {
		const b = usageBuckets([], NOW);
		expect(b).toHaveLength(30);
		expect(b.every((v) => v === 0)).toBe(true);
	});

	test("the newest day is the last bucket, 29 days ago the first", () => {
		const b = usageBuckets([rec(1000, 10, 5), rec(29 * DAY + 1000, 7, 0)], NOW);
		expect(b[29]).toBe(15);
		expect(b[0]).toBe(7);
	});

	test("records of one day add up; missing token fields count as 0", () => {
		const b = usageBuckets([rec(1000, 10, 5), rec(2000, undefined, 4), rec(3000, 1, undefined)], NOW);
		expect(b[29]).toBe(20);
	});

	test("records older than 30 days or in the future are ignored", () => {
		const b = usageBuckets([rec(30 * DAY + 1000, 99, 99), rec(-DAY, 50, 50)], NOW);
		expect(b.every((v) => v === 0)).toBe(true);
	});
});

describe("usageTotals", () => {
	test("empty", () => {
		expect(usageTotals([], NOW)).toEqual({ today: 0, week: 0, month: 0 });
	});

	test("today counts from local midnight, week and month are rolling windows", () => {
		const records = [
			rec(60 * 1000, 10, 0), // today
			rec(DAY, 20, 0), // yesterday noon: 7d and 30d only
			rec(10 * DAY, 30, 0), // 30d only
			rec(40 * DAY, 40, 0), // outside everything
		];
		expect(usageTotals(records, NOW)).toEqual({ today: 10, week: 30, month: 60 });
	});
});

describe("fmtTokens", () => {
	test("plain below 1000, one-decimal k above", () => {
		expect(fmtTokens(0)).toBe("0");
		expect(fmtTokens(999)).toBe("999");
		expect(fmtTokens(1000)).toBe("1.0k");
		expect(fmtTokens(1234)).toBe("1.2k");
	});
});
