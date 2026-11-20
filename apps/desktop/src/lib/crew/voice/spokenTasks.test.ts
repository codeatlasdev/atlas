// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { newWatch, nextSpoken } from "./spokenTasks.ts";

// Which reply is read aloud: only one that FINISHES while the chat is open, once, and only what hostd computed.
const task = (id: string, state: string, extra = {}) => ({
	id,
	state,
	ts: 1,
	prompt: "p",
	source: "desktop",
	...extra,
});

describe("which finished task is read", () => {
	test("the first list a chat shows is a baseline: old replies are never read when it opens", () => {
		const w = newWatch();
		expect(nextSpoken(w, [task("a", "done", { spoken: "Old news." })])).toBeNull();
		expect(nextSpoken(w, [task("a", "done", { spoken: "Old news." })])).toBeNull();
	});

	test("a task seen running that turns done is read, once", () => {
		const w = newWatch();
		nextSpoken(w, [task("a", "running")]);
		expect(nextSpoken(w, [task("a", "done", { spoken: "All done." })])).toBe("All done.");
		expect(nextSpoken(w, [task("a", "done", { spoken: "All done." })])).toBeNull();
	});

	test("a task that started after the chat opened and is already done at the next list is read", () => {
		const w = newWatch();
		nextSpoken(w, []);
		expect(nextSpoken(w, [task("b", "done", { spoken: "Quick one." })])).toBe("Quick one.");
	});

	test("an empty or missing `spoken` says nothing, and does not make the task readable later", () => {
		const w = newWatch();
		nextSpoken(w, [task("a", "running"), task("b", "running")]);
		expect(nextSpoken(w, [task("a", "done", { spoken: "" }), task("b", "done")])).toBeNull();
		expect(
			nextSpoken(w, [task("a", "done", { spoken: "late" }), task("b", "done", { spoken: "late" })]),
		).toBeNull();
	});

	test("failed and cancelled tasks are not read", () => {
		const w = newWatch();
		nextSpoken(w, [task("a", "running"), task("b", "running")]);
		expect(
			nextSpoken(w, [task("a", "failed", { spoken: "x" }), task("b", "cancelled", { spoken: "y" })]),
		).toBeNull();
	});

	test("two finishing together: the one that finished last is read", () => {
		const w = newWatch();
		nextSpoken(w, [task("a", "running"), task("b", "running")]);
		const said = nextSpoken(w, [
			task("a", "done", { spoken: "first", finished_at: 10 }),
			task("b", "done", { spoken: "second", finished_at: 20 }),
		]);
		expect(said).toBe("second");
	});
});
