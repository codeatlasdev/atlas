// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { insertDictation } from "./insert.ts";

describe("insertDictation: at the caret", () => {
	test("into an empty draft the text goes in as is, caret after it", () => {
		expect(insertDictation("", 0, 0, "open src/main.rs")).toEqual({ value: "open src/main.rs", cursor: 16 });
	});

	test("after a word a space is added, so the words do not run together", () => {
		const e = insertDictation("fix the", 7, 7, "login bug");
		expect(e.value).toBe("fix the login bug");
		expect(e.cursor).toBe(e.value.length);
	});

	test("after a space or a newline no second space is added", () => {
		expect(insertDictation("fix the ", 8, 8, "login bug").value).toBe("fix the login bug");
		expect(insertDictation("line one\n", 9, 9, "line two").value).toBe("line one\nline two");
	});

	test("in the middle of a draft both sides get their space and the caret stays after the dictated words", () => {
		const e = insertDictation("fixbug", 3, 3, "the login");
		expect(e.value).toBe("fix the login bug");
		expect(e.value.slice(0, e.cursor)).toBe("fix the login");
	});

	test("before a word that follows a space no extra space is added", () => {
		expect(insertDictation("the bug", 0, 0, "fix").value).toBe("fix the bug");
		expect(insertDictation(" bug", 0, 0, "fix").value).toBe("fix bug");
	});

	test("text that starts with punctuation hangs on the previous word", () => {
		expect(insertDictation("see the log", 11, 11, ", then retry").value).toBe("see the log, then retry");
		expect(insertDictation("done", 4, 4, ".").value).toBe("done.");
	});

	test("an opening bracket, quote or path start glues the next word", () => {
		expect(insertDictation("call foo(", 9, 9, "bar").value).toBe("call foo(bar");
		expect(insertDictation('say "', 5, 5, "hi").value).toBe('say "hi');
		expect(insertDictation("open src/", 9, 9, "main.rs").value).toBe("open src/main.rs");
	});

	test("a closing mark after the caret is not pushed away by a space", () => {
		expect(insertDictation("(see)", 4, 4, "the log").value).toBe("(see the log)");
	});

	test("the dictation's own surrounding whitespace is trimmed", () => {
		expect(insertDictation("a", 1, 1, "  b c \n").value).toBe("a b c");
	});
});

describe("insertDictation: over the selection", () => {
	test("the selected words are replaced, the rest of the draft untouched", () => {
		const e = insertDictation("fix the foo bug", 8, 11, "login");
		expect(e.value).toBe("fix the login bug");
		expect(e.value.slice(0, e.cursor)).toBe("fix the login");
	});

	test("a selection of the whole draft is replaced entirely", () => {
		expect(insertDictation("old draft", 0, 9, "new one").value).toBe("new one");
	});

	test("a selection made backwards (end before start) is the same selection", () => {
		expect(insertDictation("fix the foo bug", 11, 8, "login").value).toBe("fix the login bug");
	});
});

describe("insertDictation: what it refuses to do", () => {
	test("a blank transcript leaves the draft and its selection alone (silence must not delete the selection)", () => {
		expect(insertDictation("keep this", 0, 4, "   ")).toEqual({ value: "keep this", cursor: 4 });
		expect(insertDictation("keep this", 2, 2, "")).toEqual({ value: "keep this", cursor: 2 });
	});

	test("offsets outside the draft are clamped instead of corrupting it", () => {
		expect(insertDictation("abc", 99, 120, "d").value).toBe("abc d");
		expect(insertDictation("abc", -5, -1, "d").value).toBe("d abc");
	});
});
