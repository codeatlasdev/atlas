// @ts-nocheck
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// The task as hostd serializes it after a restart interrupted it (#199); written by the Rust test
// `crewmate_task_resumed_fixture_is_real_payload` (UPDATE_FIXTURES=1), never by hand.
const task = JSON.parse(
	readFileSync(join(import.meta.dir, "../../../../mobile/test/fixtures/crewmate_task_resumed.json"), "utf8"),
);

describe("CrewmateTask resumed wire shape", () => {
	test("says it was resumed and how many restarts interrupted it", () => {
		expect(task.resumed).toBe(true);
		expect(task.restarts).toBe(1);
	});
	test("is queued again with a step that says the server restarted", () => {
		expect(task.state).toBe("queued");
		expect(task.steps.some((s) => s.text.includes("restarted"))).toBe(true);
	});
});
