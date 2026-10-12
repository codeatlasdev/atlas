// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	MAX_FILES,
	MAX_MATCHES,
	Results,
	byteToIndex,
	gitGrepArgs,
	locator,
	normalizeGlob,
	rgArgs,
	rgError,
	splitGlobs,
} from "./parse.ts";
import { flatRows, previewRuns } from "./rows.ts";

const o = { query: "foo", regex: false, caseSensitive: false, wholeWord: false, include: "", exclude: "" };
const rgMatch = (path, line, text, subs) =>
	JSON.stringify({
		type: "match",
		data: { path: { text: path }, lines: { text }, line_number: line, submatches: subs },
	});

test("globs split on commas and whitespace; bare folders match their contents", () => {
	expect(splitGlobs("src/**, *.ts  dist")).toEqual(["src/**", "*.ts", "dist"]);
	expect(splitGlobs("  ,, ")).toEqual([]);
	expect(normalizeGlob("src")).toBe("src/**");
	expect(normalizeGlob("src/")).toBe("src/**");
	expect(normalizeGlob("*.ts")).toBe("*.ts");
	expect(normalizeGlob("Makefile.am")).toBe("Makefile.am");
});

test("rg args reflect the toggles and never go through a shell", () => {
	const a = rgArgs({
		...o,
		regex: true,
		caseSensitive: true,
		wholeWord: true,
		include: "src",
		exclude: "*.min.js",
	});
	expect(a).toContain("--case-sensitive");
	expect(a).toContain("--word-regexp");
	expect(a).not.toContain("--fixed-strings");
	expect(a.join(" ")).toContain("--glob src/**");
	expect(a.join(" ")).toContain("--glob !*.min.js");
	// the query follows `--` so a leading dash is never parsed as a flag
	expect(a.slice(-3)).toEqual(["--", "foo", "."]);
	expect(rgArgs(o)).toContain("--fixed-strings");
});

test("git grep fallback keeps scope pathspecs after --", () => {
	const a = gitGrepArgs({ ...o, include: "src", exclude: "dist" });
	expect(a.slice(0, 3)).toEqual(["git", "grep", "-n"]);
	expect(a).toContain("-F");
	expect(a).toContain(":(glob)src/**");
	expect(a).toContain(":(exclude,glob)dist/**");
	expect(gitGrepArgs({ ...o, caseSensitive: true, regex: true })).not.toContain("-i");
});

test("rg --json: matches with submatch ranges, non-match events ignored", () => {
	const r = new Results();
	r.addRgLine(JSON.stringify({ type: "begin", data: {} }));
	r.addRgLine(
		rgMatch("./src/a.ts", 3, "  const foo = foo();\n", [
			{ start: 8, end: 11 },
			{ start: 14, end: 17 },
		]),
	);
	r.addRgLine(rgMatch("src/b.ts", 1, "foo", [{ start: 0, end: 3 }]));
	r.addRgLine("not json");
	r.addRgLine("");
	expect(r.files.map((f) => f.path)).toEqual(["src/a.ts", "src/b.ts"]);
	expect(r.files[0].matches).toHaveLength(2);
	expect(r.files[0].matches[0]).toEqual({ line: 3, start: 8, end: 11, text: "  const foo = foo();" });
	expect(r.matches).toBe(3);
});

test("rg submatch offsets are bytes: multibyte text maps to UTF-16 indices", () => {
	expect(byteToIndex("héllo foo", 8)).toBe(7);
	expect(byteToIndex("😀foo", 4)).toBe(2);
	const r = new Results();
	r.addRgLine(rgMatch("a.md", 1, "héllo foo", [{ start: 7, end: 10 }]));
	const m = r.files[0].matches[0];
	expect(m.text.slice(m.start, m.end)).toBe("foo");
});

test("rg non-utf8 paths and lines arrive as base64 bytes", () => {
	const r = new Results();
	r.addRgLine(
		JSON.stringify({
			type: "match",
			data: {
				path: { bytes: btoa("a.bin") },
				lines: { bytes: btoa("foo") },
				line_number: 2,
				submatches: [{ start: 0, end: 3 }],
			},
		}),
	);
	expect(r.files[0].path).toBe("a.bin");
});

test("results stop at the match and file limits and flag truncation", () => {
	const r = new Results();
	let more = true;
	for (let i = 0; more && i < MAX_MATCHES + 5; i++)
		more = r.addRgLine(rgMatch("one.ts", i + 1, "foo", [{ start: 0, end: 3 }]));
	expect(more).toBe(false);
	expect(r.matches).toBe(MAX_MATCHES);
	expect(r.truncated).toBe(true);

	const f = new Results();
	more = true;
	for (let i = 0; more && i < MAX_FILES + 5; i++)
		more = f.addRgLine(rgMatch(`f${i}.ts`, 1, "foo", [{ start: 0, end: 3 }]));
	expect(f.files).toHaveLength(MAX_FILES);
	expect(f.truncated).toBe(true);
});

test("very long lines are windowed around the match", () => {
	const text = "x".repeat(1000) + "foo" + "y".repeat(1000);
	const r = new Results();
	r.addRgLine(rgMatch("min.js", 1, text, [{ start: 1000, end: 1003 }]));
	const m = r.files[0].matches[0];
	expect(m.text.length).toBeLessThanOrEqual(402);
	expect(m.text.slice(m.start, m.end)).toBe("foo");
});

test("git grep lines parse path:line:text, even with colons in the text", () => {
	const r = new Results();
	const locate = locator({ ...o, query: "foo" });
	r.addGrepLine("src/a.ts:12:const Foo = a:foo", locate);
	r.addGrepLine("garbage", locate);
	const f = r.files[0];
	expect(f.path).toBe("src/a.ts");
	expect(r.files).toHaveLength(1);
	expect(f.matches.map((m) => [m.line, m.start, m.end])).toEqual([
		[12, 6, 9],
		[12, 14, 17],
	]);
});

test("locator honors case, whole word and regex; invalid regex yields no ranges", () => {
	expect(locator(o)("Foo foo")).toEqual([
		[0, 3],
		[4, 7],
	]);
	expect(locator({ ...o, caseSensitive: true })("Foo foo")).toEqual([[4, 7]]);
	expect(locator({ ...o, wholeWord: true })("foobar foo")).toEqual([[7, 10]]);
	expect(locator({ ...o, query: "a.c" })("abc a.c")).toEqual([[4, 7]]);
	expect(locator({ ...o, regex: true, query: "a.c" })("abc a.c")).toEqual([
		[0, 3],
		[4, 7],
	]);
	expect(locator({ ...o, regex: true, query: "(" })("x(")).toEqual([]);
});

test("rg exit codes: 0/1 are fine, 2 reports the first stderr line", () => {
	expect(rgError(0, "")).toBeNull();
	expect(rgError(1, "")).toBeNull();
	expect(rgError(2, "rg: regex parse error:\n  (\n")).toBe("regex parse error:");
	expect(rgError(2, "")).toBe("Search failed (exit 2)");
});

test("rows: header per file, matches hidden when collapsed", () => {
	const files = [
		{
			path: "a.ts",
			matches: [
				{ line: 1, start: 0, end: 1, text: "x" },
				{ line: 2, start: 0, end: 1, text: "x" },
			],
		},
		{ path: "b.ts", matches: [{ line: 1, start: 0, end: 1, text: "x" }] },
	];
	expect(flatRows(files, {}).map((r) => r.kind)).toEqual(["file", "match", "match", "file", "match"]);
	expect(flatRows(files, { "a.ts": true }).map((r) => r.kind)).toEqual(["file", "file", "match"]);
});

test("preview drops indentation and keeps the highlight aligned", () => {
	const runs = previewRuns({ line: 1, start: 8, end: 11, text: "    let foo = 1;" });
	expect(runs.map((r) => r.text).join("")).toBe("let foo = 1;");
	expect(runs.find((r) => r.hit).text).toBe("foo");
});
