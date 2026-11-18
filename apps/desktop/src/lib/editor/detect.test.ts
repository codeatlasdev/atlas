// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	applySavePolicy,
	detectEol,
	detectIndent,
	hasFinalNewline,
	normalizeEol,
	trimTrailingWhitespace,
} from "./detect.ts";
import { growth, linearBound } from "../../../test/scaling.ts";

const FALLBACK = { style: "space", size: 4 } as const;

test("detectEol: pure endings, none, and mixed", () => {
	expect(detectEol("a\nb\nc")).toBe("lf");
	expect(detectEol("a\r\nb\r\n")).toBe("crlf");
	expect(detectEol("a\rb\r")).toBe("cr");
	expect(detectEol("no breaks")).toBe("lf");
	expect(detectEol("")).toBe("lf");
	expect(detectEol("a\r\nb\nc")).toBe("mixed");
	expect(detectEol("a\rb\nc")).toBe("mixed");
	expect(detectEol("a\r\nb\rc")).toBe("mixed");
});

test("detectEol only samples the first megabyte", () => {
	const head = "a\n".repeat(600_000); // 1.2 MB of LF
	expect(detectEol(head + "b\r\n")).toBe("lf");
});

test("detectIndent: 4 spaces, 2 spaces, 8-wide", () => {
	const four = "fn a() {\n    if x {\n        y();\n    }\n    z();\n}\n";
	expect(detectIndent(four, { style: "space", size: 2 })).toEqual({ style: "space", size: 4 });
	const two = "a:\n  b:\n    c: 1\n    d: 2\n  e: 3\nf: 4\n";
	expect(detectIndent(two, FALLBACK)).toEqual({ style: "space", size: 2 });
});

test("detectIndent: tabs when most indented lines start with a tab", () => {
	const text = "a {\n\tb;\n\tc {\n\t\td;\n\t}\n}\n";
	expect(detectIndent(text, FALLBACK)).toEqual({ style: "tab", size: 4 });
	// the configured size is kept when the fallback is already tabs
	expect(detectIndent(text, { style: "tab", size: 8 })).toEqual({ style: "tab", size: 8 });
});

test("detectIndent: a few tab lines among spaces stay spaces", () => {
	const text = "a\n  b\n  c\n\td\n  e\n";
	expect(detectIndent(text, FALLBACK)).toEqual({ style: "space", size: 2 });
});

test("detectIndent: no evidence returns the fallback", () => {
	expect(detectIndent("a\nb\nc\n", { style: "space", size: 3 })).toEqual({ style: "space", size: 3 });
	expect(detectIndent("", FALLBACK)).toEqual(FALLBACK);
	expect(detectIndent("a\n\n\n   \nb\n", FALLBACK)).toEqual(FALLBACK);
});

test("detectIndent: blank and CRLF lines do not skew the result", () => {
	const text = "a\r\n  b\r\n\r\n  c\r\n    d\r\n";
	expect(detectIndent(text, FALLBACK)).toEqual({ style: "space", size: 2 });
});

test("detectIndent: continuation alignment deltas above 8 are ignored", () => {
	const text = "x\n    y\n                          z\n    w\n";
	expect(detectIndent(text, FALLBACK)).toEqual({ style: "space", size: 4 });
});

test("normalizeEol converts every ending and leaves the target alone", () => {
	expect(normalizeEol("a\nb\r\nc\rd", "lf")).toBe("a\nb\nc\nd");
	expect(normalizeEol("a\nb\r\nc\rd", "crlf")).toBe("a\r\nb\r\nc\r\nd");
	expect(normalizeEol("a\nb\r\nc\rd", "cr")).toBe("a\rb\rc\rd");
	expect(normalizeEol("a\r\nb\r\n", "crlf")).toBe("a\r\nb\r\n");
	expect(normalizeEol("one line", "crlf")).toBe("one line");
	expect(normalizeEol("", "lf")).toBe("");
	expect(normalizeEol("\n\n", "crlf")).toBe("\r\n\r\n");
});

test("hasFinalNewline", () => {
	expect(hasFinalNewline("a\n")).toBe(true);
	expect(hasFinalNewline("a\r\n")).toBe(true);
	expect(hasFinalNewline("a\r")).toBe(true);
	expect(hasFinalNewline("a")).toBe(false);
	expect(hasFinalNewline("")).toBe(false);
});

test("trimTrailingWhitespace: every line, every ending kind, last line", () => {
	expect(trimTrailingWhitespace("a  \nb\t\r\nc \rd  ")).toBe("a\nb\r\nc\rd");
	expect(trimTrailingWhitespace("  keep leading  \n")).toBe("  keep leading\n");
	expect(trimTrailingWhitespace("   \n\t\n")).toBe("\n\n");
	expect(trimTrailingWhitespace("")).toBe("");
});

test("trimTrailingWhitespace: skipLines (set or array) are untouched", () => {
	expect(trimTrailingWhitespace("a  \nb  \nc  ", { skipLines: new Set([1]) })).toBe("a\nb  \nc");
	expect(trimTrailingWhitespace("a  \nb  ", { skipLines: [0] })).toBe("a  \nb");
});

test("trimTrailingWhitespace returns the same string when clean", () => {
	const s = "a\nb\n";
	expect(trimTrailingWhitespace(s)).toBe(s);
});

test("applySavePolicy combines trim, final newline and eol", () => {
	expect(applySavePolicy("a  \nb", { trim: true, finalNewline: true, eol: "lf" })).toBe("a\nb\n");
	expect(applySavePolicy("a  \nb", { trim: true, finalNewline: true, eol: "crlf" })).toBe("a\r\nb\r\n");
	expect(applySavePolicy("a  \nb", { trim: false, finalNewline: false, eol: "lf" })).toBe("a  \nb");
	expect(applySavePolicy("a\n", { trim: true, finalNewline: true, eol: "lf" })).toBe("a\n");
	expect(applySavePolicy("", { trim: true, finalNewline: true, eol: "lf" })).toBe("");
	expect(applySavePolicy("a\r\nb\n", { trim: false, finalNewline: false, eol: "cr" })).toBe("a\rb\r");
});

// Incident class (editor core, large files): a scan that backtracks or rebuilds a string per character is fine on a
// normal file and freezes the editor on a minified one (5 MB, one line). The guard is how the cost GROWS with the one
// line: CPU time at 8x the size over the cost at 1x (test/scaling.ts), ~8 for a linear pass and ~64 for a quadratic one,
// whatever the machine or its load. It was `< 500 ms` on 5 MB, which failed under load with nothing regressed; the sizes
// are small so that a quadratic regression is red in seconds instead of hanging a 5 MB scan.
const minified = (n: number) => "var a=1;".repeat(n / 8); // no newline, nothing to trim
const scans: [string, (text: string) => unknown, (text: string, result: unknown) => void][] = [
	["detectEol", (t) => detectEol(t), (_, r) => expect(r).toBe("lf")],
	["detectIndent", (t) => detectIndent(t, FALLBACK), (_, r) => expect(r).toEqual(FALLBACK)],
	["trimTrailingWhitespace", (t) => trimTrailingWhitespace(t), (t, r) => expect(r).toBe(t)],
	["normalizeEol", (t) => normalizeEol(t, "crlf"), (t, r) => expect(r).toBe(t)],
	[
		"applySavePolicy",
		(t) => applySavePolicy(t, { trim: true, finalNewline: false, eol: "lf" }),
		(t, r) => expect(r).toBe(t),
	],
];

/** The scan of `make(n)` as a timed operation; it also checks the result, so a fast wrong answer is not a pass. */
const over =
	(
		make: (n: number) => string,
		scan: (text: string) => unknown,
		check?: (text: string, r: unknown) => void,
	) =>
	(n: number) => {
		const text = make(n);
		check?.(text, scan(text));
		return () => scan(text);
	};

for (const [name, scan, check] of scans)
	test(`${name} reads a minified single line right and costs linear time in its length`, () => {
		expect(growth(over(minified, scan, check), 16_384)).toBeLessThan(linearBound());
	});

test("a long run of trailing spaces does not blow up", () => {
	// Quadratic here is the `/\s+$/` backtracking trap: every start of the run rescans to its end.
	const spaces = over(
		(n) => "x" + " ".repeat(n) + "\ny",
		trimTrailingWhitespace,
		(_, r) => expect(r).toBe("x\ny"),
	);
	expect(growth(spaces, 16_384)).toBeLessThan(linearBound());
});
