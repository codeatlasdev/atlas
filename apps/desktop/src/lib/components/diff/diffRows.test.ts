// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { CHUNK, diffRows, fileRowIndex } from "./diffRows.ts";

const lines = (n: number) =>
	Array.from({ length: n }, (_, i) => ({ kind: "ctx", text: `l${i}`, old_no: i + 1, new_no: i + 1 }));
const hunk = (n: number, header = "@@ -1 +1 @@") => ({
	header,
	old_start: 1,
	old_lines: n,
	new_start: 1,
	new_lines: n,
	lines: lines(n),
});
const file = (path: string, hunks = [hunk(3)], extra = {}) => ({
	path,
	status: "modified",
	staged: false,
	binary: false,
	hunks,
	...extra,
});

test("a file becomes its header, then per hunk a head and its tables", () => {
	const rows = diffRows([file("a.ts", [hunk(3), hunk(2, "@@ -9 +9 @@")])]);
	expect(rows.map((r) => r.kind)).toEqual(["file", "hunk", "table", "hunk", "table"]);
});

test("a hunk longer than CHUNK is split into tables of at most CHUNK lines (one 20k-line hunk must not be one node tree)", () => {
	const rows = diffRows([file("big.ts", [hunk(CHUNK * 2 + 5)])]);
	const tables = rows.filter((r) => r.kind === "table");
	expect(tables.map((t) => t.hunk.lines.length)).toEqual([CHUNK, CHUNK, 5]);
});

test("a chunk keeps the tokens of its own lines (highlight is indexed by line within the hunk)", () => {
	const tokens = Array.from({ length: CHUNK + 2 }, (_, i) => `t${i}`);
	const rows = diffRows([file("big.ts", [hunk(CHUNK + 2)])], { "big.ts#0": tokens });
	const [a, b] = rows.filter((r) => r.kind === "table");
	expect(a.tokens[0]).toBe("t0");
	expect(b.tokens[0]).toBe(`t${CHUNK}`);
	expect(b.tokens).toHaveLength(2);
});

test("tokens that have not resolved yet stay undefined, not an empty array", () => {
	const rows = diffRows([file("a.ts")]);
	expect(rows.find((r) => r.kind === "table").tokens).toBeUndefined();
});

test("a chunk remembers its hunk index (stage-hunk acts on the whole hunk, not the chunk)", () => {
	const rows = diffRows([file("a.ts", [hunk(1), hunk(CHUNK + 1)])]);
	const t = rows.filter((r) => r.kind === "table");
	expect(t.map((x) => x.hi)).toEqual([0, 1, 1]);
	expect(t[2].hunk).not.toBe(t[1].hunk);
	expect(rows.find((r) => r.kind === "hunk" && r.hi === 1).hunk.lines.length).toBe(CHUNK + 1);
});

test("a binary file shows a note row instead of tables", () => {
	const rows = diffRows([file("logo.png", [], { binary: true })]);
	expect(rows.map((r) => r.kind)).toEqual(["file", "binary"]);
});

test("every row has a unique key, stable across a re-run with the same input", () => {
	const input = [file("a.ts", [hunk(CHUNK * 3)]), file("b.ts")];
	const a = diffRows(input).map((r) => r.key);
	expect(new Set(a).size).toBe(a.length);
	expect(diffRows(input).map((r) => r.key)).toEqual(a);
});

test("fileRowIndex finds the header row of a file for jump-to-file", () => {
	const rows = diffRows([file("a.ts", [hunk(CHUNK * 2)]), file("b.ts")]);
	expect(fileRowIndex(rows, "a.ts")).toBe(0);
	expect(fileRowIndex(rows, "b.ts")).toBe(rows.findIndex((r) => r.kind === "file" && r.f.path === "b.ts"));
	expect(fileRowIndex(rows, "nope")).toBe(-1);
});

test("a 20k-line diff yields a bounded number of rows per screen worth, and the row count is linear", () => {
	const rows = diffRows([file("big.ts", [hunk(20_000)])]);
	expect(rows.length).toBe(1 + 1 + Math.ceil(20_000 / CHUNK));
});
