// @ts-nocheck
/**
 * Tests for automations.ts: cron ↔ builder round-trip, describeTrigger, describeNext.
 * Run with: bun test --conditions browser src/lib/crew/automations.test.ts
 */
import { describe, expect, test } from "bun:test";
import {
	scheduleFromCron,
	cronFromSchedule,
	describeTrigger,
	describeNext,
	intervalFromSecs,
	intervalToSecs,
	type ScheduleSpec,
} from "./automations.ts";

// ─── cron ↔ builder round-trips ──────────────────────────────────────────────

describe("scheduleFromCron", () => {
	test("every day (wildcard dow)", () => {
		const spec = scheduleFromCron("0 9 * * *");
		expect(spec).toEqual({ days: [0, 1, 2, 3, 4, 5, 6], hour: 9, minute: 0 });
	});

	test("weekdays range 1-5", () => {
		const spec = scheduleFromCron("0 9 * * 1-5");
		expect(spec).toEqual({ days: [1, 2, 3, 4, 5], hour: 9, minute: 0 });
	});

	test("single day", () => {
		const spec = scheduleFromCron("30 18 * * 1");
		expect(spec).toEqual({ days: [1], hour: 18, minute: 30 });
	});

	test("comma-separated days", () => {
		const spec = scheduleFromCron("0 8 * * 1,3,5");
		expect(spec).toEqual({ days: [1, 3, 5], hour: 8, minute: 0 });
	});

	test("step expression returns null (unsupported)", () => {
		// "0 */3 * * *" — hourly step
		expect(scheduleFromCron("0 */3 * * *")).toBeNull();
	});

	test("non-wildcard dom/month returns null", () => {
		expect(scheduleFromCron("0 9 1 * *")).toBeNull();
	});

	test("wrong field count returns null", () => {
		expect(scheduleFromCron("0 9 * *")).toBeNull();
	});
});

describe("cronFromSchedule → scheduleFromCron round-trip", () => {
	const cases: Array<[string, ScheduleSpec]> = [
		["every day", { days: [0, 1, 2, 3, 4, 5, 6], hour: 9, minute: 0 }],
		["weekdays", { days: [1, 2, 3, 4, 5], hour: 9, minute: 0 }],
		["single day Mon", { days: [1], hour: 18, minute: 30 }],
		["non-contiguous Mon Wed Fri", { days: [1, 3, 5], hour: 8, minute: 0 }],
		["weekends", { days: [0, 6], hour: 10, minute: 0 }],
	];

	for (const [label, spec] of cases) {
		test(label, () => {
			const cron = cronFromSchedule(spec);
			const parsed = scheduleFromCron(cron);
			// days are sorted by scheduleFromCron
			expect(parsed?.days).toEqual([...spec.days].sort((a, b) => a - b));
			expect(parsed?.hour).toBe(spec.hour);
			expect(parsed?.minute).toBe(spec.minute);
		});
	}

	test("contiguous range is compact (not comma-separated)", () => {
		expect(cronFromSchedule({ days: [1, 2, 3, 4, 5], hour: 9, minute: 0 })).toBe("0 9 * * 1-5");
	});

	test("all days uses wildcard", () => {
		expect(cronFromSchedule({ days: [0, 1, 2, 3, 4, 5, 6], hour: 9, minute: 0 })).toBe("0 9 * * *");
	});
});

// ─── describeTrigger ─────────────────────────────────────────────────────────

describe("describeTrigger", () => {
	test("schedule every day", () => {
		expect(describeTrigger({ type: "schedule", cron: "0 9 * * *" })).toBe("Every day at 09:00");
	});

	test("schedule weekdays", () => {
		expect(describeTrigger({ type: "schedule", cron: "0 9 * * 1-5" })).toBe("Weekdays at 09:00");
	});

	test("schedule specific days", () => {
		expect(describeTrigger({ type: "schedule", cron: "30 18 * * 1,3,5" })).toBe("Mon, Wed, Fri at 18:30");
	});

	test("schedule with tz appended", () => {
		expect(describeTrigger({ type: "schedule", cron: "0 9 * * 1-5" }, "America/Sao_Paulo")).toBe(
			"Weekdays at 09:00 (America/Sao_Paulo)",
		);
	});

	test("schedule custom cron fallback", () => {
		// step expression → not parseable → Custom: ...
		expect(describeTrigger({ type: "schedule", cron: "0 */3 * * *" })).toBe("Custom: 0 */3 * * *");
	});

	test("interval exact minutes", () => {
		expect(describeTrigger({ type: "interval", every_secs: 1800 })).toBe("Every 30 minutes");
	});

	test("interval 1 hour (singular)", () => {
		expect(describeTrigger({ type: "interval", every_secs: 3600 })).toBe("Every 1 hour");
	});

	test("interval 2 hours", () => {
		expect(describeTrigger({ type: "interval", every_secs: 7200 })).toBe("Every 2 hours");
	});

	test("interval fractional hours shows h m", () => {
		expect(describeTrigger({ type: "interval", every_secs: 5400 })).toBe("Every 1h 30m");
	});

	test("webhook", () => {
		expect(describeTrigger({ type: "webhook", token: "abc123" })).toBe("Webhook");
	});

	// Seen in the settings: "When a ci failed in codeatlasdev/ac". Each GitHub event reads as a phrase.
	test("github events read as sentences", () => {
		const d = (events) => describeTrigger({ type: "github", repo: "owner/repo", events });
		expect(d(["pr_opened"])).toBe("When a PR is opened in owner/repo");
		expect(d(["ci_failed"])).toBe("When CI fails in owner/repo");
		expect(d(["pr_review_requested"])).toBe("When your review is requested in owner/repo");
		expect(d(["issue_opened"])).toBe("When an issue is opened in owner/repo");
		expect(d(["release"])).toBe("When a release is published in owner/repo");
		expect(d(["pr_opened", "ci_failed"])).toBe("When a PR is opened or CI fails in owner/repo");
		expect(d(["pr_opened", "ci_failed", "release"])).toBe(
			"When a PR is opened, CI fails or a release is published in owner/repo",
		);
	});

	test("file with glob", () => {
		expect(describeTrigger({ type: "file", path: "/src", glob: "**/*.ts" })).toBe("When **/*.ts changes");
	});

	test("file without glob falls back to path", () => {
		expect(describeTrigger({ type: "file", path: "/src/index.ts" })).toBe("When /src/index.ts changes");
	});

	test("crewmate done", () => {
		expect(describeTrigger({ type: "crewmate", from_crewmate: "Reviewer", on: "done" })).toBe(
			"When Reviewer finishes",
		);
	});

	test("crewmate failed", () => {
		expect(describeTrigger({ type: "crewmate", from_crewmate: "Reviewer", on: "failed" })).toBe(
			"When Reviewer fails",
		);
	});

	test("service failed", () => {
		expect(describeTrigger({ type: "service", project_dir: "/app", service: "web", on: "failed" })).toBe(
			"When web fails",
		);
	});

	test("service recovered", () => {
		expect(describeTrigger({ type: "service", project_dir: "/app", service: "web", on: "recovered" })).toBe(
			"When web recovers",
		);
	});
});

// ─── describeNext ─────────────────────────────────────────────────────────────

describe("describeNext", () => {
	// Fixed reference: 2024-06-10 Monday 08:00 UTC
	const now = new Date("2024-06-10T08:00:00Z").getTime();

	test("same calendar day → 'today at …'", () => {
		const fire = Math.floor(new Date("2024-06-10T09:00:00Z").getTime() / 1000);
		expect(describeNext(fire, now)).toMatch(/^today at /);
	});

	test("next calendar day → 'tomorrow at …'", () => {
		const fire = Math.floor(new Date("2024-06-11T09:00:00Z").getTime() / 1000);
		expect(describeNext(fire, now)).toMatch(/^tomorrow at /);
	});

	test("further out → weekday abbreviation prefix", () => {
		// 2024-06-12 is Wednesday
		const fire = Math.floor(new Date("2024-06-12T09:00:00Z").getTime() / 1000);
		const result = describeNext(fire, now);
		// Not today or tomorrow — must start with a weekday (locale-dependent, just check it's not those)
		expect(result).not.toMatch(/^today/);
		expect(result).not.toMatch(/^tomorrow/);
		expect(result.length).toBeGreaterThan(3);
	});
});

// ─── interval ↔ builder ──────────────────────────────────────────────────────

describe("intervalFromSecs / intervalToSecs", () => {
	test("picks the biggest unit that divides evenly", () => {
		expect(intervalFromSecs(86400 * 2)).toEqual({ value: 2, unit: "days" });
		expect(intervalFromSecs(3600 * 5)).toEqual({ value: 5, unit: "hours" });
		expect(intervalFromSecs(1800)).toEqual({ value: 30, unit: "minutes" });
	});

	test("an interval that is not a whole number of minutes rounds to minutes", () => {
		expect(intervalFromSecs(90)).toEqual({ value: 2, unit: "minutes" });
	});

	test("round-trips through the builder", () => {
		for (const secs of [60, 1800, 3600, 7200, 86400, 172800]) {
			const { value, unit } = intervalFromSecs(secs);
			expect(intervalToSecs(value, unit)).toBe(secs);
		}
	});
});
