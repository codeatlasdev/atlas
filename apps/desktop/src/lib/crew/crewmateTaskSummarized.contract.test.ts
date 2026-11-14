// @ts-nocheck
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const task = JSON.parse(
	readFileSync(
		join(import.meta.dir, "../../../../mobile/test/fixtures/crewmate_task_summarized.json"),
		"utf8",
	),
);

describe("CrewmateTask summarized wire shape", () => {
	test("summarized is true", () => expect(task.summarized).toBe(true));
	test("result is the summary", () => expect(typeof task.result).toBe("string"));
	test("reply_full is the original", () => {
		expect(typeof task.reply_full).toBe("string");
		expect(task.reply_full.length).toBeGreaterThan((task.result as string).length);
	});
	test("state is done", () => expect(task.state).toBe("done"));
});
