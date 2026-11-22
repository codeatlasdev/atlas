// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { BRACKET_CLASSES, scanBrackets } from "./bracketColors.ts";
import { growth, linearBound } from "../../../test/scaling.ts";

test("depth increases on openers and returns on closers", () => {
	const r = scanBrackets("({[]})", 0, [], 0, 100);
	expect(r.brackets.map((b) => b.depth)).toEqual([0, 1, 2, 2, 1, 0]);
	expect(r.depth).toBe(0);
});

test("depth cycles after six levels", () => {
	const r = scanBrackets("(".repeat(8), 0, [], 0, 100);
	expect(r.brackets.map((b) => b.depth)).toEqual([0, 1, 2, 3, 4, 5, 0, 1]);
	expect(BRACKET_CLASSES).toBe(6);
});

test("skipped ranges (strings, comments) are ignored", () => {
	const text = 'f("(", x) // )';
	const r = scanBrackets(
		text,
		100,
		[
			[102, 105],
			[112, 114],
		],
		0,
		100,
	);
	expect(r.brackets.map((b) => b.pos)).toEqual([101, 108]);
});

test("an unmatched closer never goes below depth 0", () => {
	const r = scanBrackets("))(", 0, [], 0, 100);
	expect(r.brackets.map((b) => b.depth)).toEqual([0, 0, 0]);
	expect(r.depth).toBe(1);
});

test("work is capped and reported", () => {
	const r = scanBrackets("()".repeat(10_000), 0, [], 0, 5_000);
	expect(r.brackets.length).toBe(5_000);
	expect(r.truncated).toBe(true);
});

// The scan runs on the visible text on every viewport change: it must cost time linear in the lines it is given (CPU time
// at 8x the lines over the cost at 1x, test/scaling.ts: ~8 for a linear pass, ~64 for a quadratic one), not a wall-clock
// budget, which a loaded machine fails with nothing regressed.
test("a viewport scan costs linear time in its lines", () => {
	const open = (lines: number) => {
		const text = "foo(bar[1], {a: (2)});\n".repeat(lines);
		return () => scanBrackets(text, 0, [], 0, Infinity);
	};
	expect(scanBrackets("foo(bar[1], {a: (2)});\n".repeat(10_000), 0, [], 0, Infinity).brackets).toHaveLength(
		80_000,
	);
	expect(growth(open, 500)).toBeLessThan(linearBound());
});
