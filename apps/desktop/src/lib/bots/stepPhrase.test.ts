// @ts-nocheck
import { describe, expect, test } from "bun:test";
import { stepGroups, stepPhrase, workingLabel, IDLE_WORDS } from "./stepPhrase.ts";

// A step is what hostd records per tool call: "<tool> <first string argument>" (crates/atlas-host/src/bots/runner.rs).
// The working line shows it as a short phrase, so the user sees what the crewmate is doing, not just "Working".
describe("stepPhrase", () => {
	test("reading a file names the file, not its full path", () => {
		expect(stepPhrase("Read /var/lib/atlas-bot/.atlas/bots/x/work/src/routes/login.svelte")).toBe(
			"Reading login.svelte",
		);
		expect(stepPhrase("read src/app.ts")).toBe("Reading app.ts");
	});

	test("edits and writes", () => {
		expect(stepPhrase("Edit src/lib/api.ts")).toBe("Editing api.ts");
		expect(stepPhrase("write CHANGELOG.md")).toBe("Writing CHANGELOG.md");
	});

	test("shell commands say what kind of command, by its program", () => {
		expect(stepPhrase("Bash git status")).toBe("Checking git");
		expect(stepPhrase("bash bun test src/lib")).toBe("Running the tests");
		expect(stepPhrase("bash cargo test -p atlas-host")).toBe("Running the tests");
		expect(stepPhrase("bash npm install")).toBe("Installing dependencies");
		expect(stepPhrase("bash gh pr create --fill")).toBe("Working on GitHub");
		expect(stepPhrase("bash ls -la")).toBe("Running ls");
	});

	test("searching", () => {
		expect(stepPhrase("Grep handleLogin")).toBe("Searching for “handleLogin”");
		expect(stepPhrase("glob **/*.svelte")).toBe("Looking for files");
	});

	test("a tool it does not know still reads as a sentence, never a raw id", () => {
		expect(stepPhrase('mcp__linear__save_issue {"x":1}')).toBe("Using save issue");
		expect(stepPhrase("todo")).toBe("Using todo");
	});

	test("nothing yet is the plain working line", () => {
		expect(stepPhrase(undefined)).toBe("Working");
		expect(stepPhrase("")).toBe("Working");
	});
});

// Between tool calls the agent is thinking: the line cycles through short verbs instead of freezing on one phrase.
describe("workingLabel", () => {
	const at = (s: number) => s * 1000;

	test("a fresh step shows what it did", () => {
		expect(workingLabel("Read src/app.ts", at(10), at(11))).toBe("Reading app.ts");
	});

	test("after a few quiet seconds it cycles idle words, changing every few seconds", () => {
		const a = workingLabel("Read src/app.ts", at(10), at(16));
		const b = workingLabel("Read src/app.ts", at(10), at(20));
		expect(IDLE_WORDS).toContain(a);
		expect(IDLE_WORDS).toContain(b);
		expect(a).not.toBe(b);
	});

	test("with no step yet it starts cycling right away", () => {
		expect(IDLE_WORDS).toContain(workingLabel(undefined, undefined, at(1)));
	});

	test("the same moment always gives the same word (no flicker on re-render)", () => {
		expect(workingLabel(undefined, undefined, at(7))).toBe(workingLabel(undefined, undefined, at(7)));
	});
});

// The live card grouped steps only by the latest title: when omp moved to its next piece of work the earlier steps
// vanished, and once the task ended only a tiny "activity" toggle was left. Each title is now a section of the chat.
describe("stepGroups", () => {
	const s = (text, group, ts = 0) => ({ ts, text, group });

	test("consecutive steps under one title form one group; a new title opens the next", () => {
		const g = stepGroups(
			[s("read a.ts", "CART: Testar"), s("bash bun test", "CART: Testar"), s("edit b.ts", "FIX: Corrigir")],
			true,
		);
		expect(g.map((x) => [x.title, x.steps.length, x.done])).toEqual([
			["CART: Testar", 2, true],
			["FIX: Corrigir", 1, false],
		]);
	});

	test("steps before omp's first real title join that first group instead of an untitled one", () => {
		const g = stepGroups([s("read memory.md"), s("bash git status"), s("read a.ts", "CART: Testar")], true);
		expect(g.map((x) => [x.title, x.steps.length])).toEqual([["CART: Testar", 3]]);
	});

	test("when the task is over every group is done; with no title at all there is one untitled group", () => {
		expect(stepGroups([s("read a.ts", "A"), s("read b.ts", "B")], false).every((x) => x.done)).toBe(true);
		expect(stepGroups([s("read a.ts")], true).map((x) => [x.title, x.done])).toEqual([[undefined, false]]);
		expect(stepGroups([], true)).toEqual([]);
	});

	// Seen live: omp's title reached the task only after its 12 steps, so none carried a group.
	test("steps that never got a title take the task's own title", () => {
		const g = stepGroups([s("read a.ts"), s("read b.ts")], false, "STORE: Explicar");
		expect(g.map((x) => [x.title, x.steps.length])).toEqual([["STORE: Explicar", 2]]);
		expect(stepGroups([s("read a", "A")], false, "T").map((x) => x.title)).toEqual(["A"]);
	});

	test("a title that comes back later is a new group, so the order of the work is kept", () => {
		const g = stepGroups([s("read a", "A"), s("read b", "B"), s("read c", "A")], false);
		expect(g.map((x) => x.title)).toEqual(["A", "B", "A"]);
	});
});

test("an omp hashline edit target names the file, not the [path#TAG] anchor", () => {
	expect(stepPhrase("edit [apps/store/src/lib/analytics/footer-contact.test.ts#07A1]")).toBe(
		"Editing footer-contact.test.ts",
	);
	// hostd keeps 80 characters of a step, which can cut the closing bracket.
	expect(stepPhrase("edit [apps/store/src/lib/analytics/footer-contact.test.ts#07")).toBe(
		"Editing footer-contact.test.ts",
	);
});

// Seen live: the agent runs "cd apps/store && bun test" and the card said "Running cd".
test("a leading cd into a folder does not hide the command it runs", () => {
	expect(stepPhrase("bash cd apps/store && bun test 2>&1 | tail")).toBe("Running the tests");
	expect(stepPhrase("bash cd apps/store && git status")).toBe("Checking git");
	expect(stepPhrase("bash cd /tmp; ls -la")).toBe("Running ls");
	expect(stepPhrase("bash cd apps/store")).toBe("Running cd");
});
