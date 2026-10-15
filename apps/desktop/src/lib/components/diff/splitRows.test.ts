// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { changedSpan, splitRows } from "./splitRows.ts";

/** Parse a real `git diff` hunk body the way the Rust side hands it over (kind + text + line numbers). */
function hunk(body: string[], oldStart: number, newStart: number) {
	let o = oldStart;
	let n = newStart;
	return body.map((raw) => {
		const t = raw[0];
		const text = raw.slice(1);
		if (t === "+") return { kind: "add", text, new_no: n++ };
		if (t === "-") return { kind: "del", text, old_no: o++ };
		return { kind: "ctx", text, old_no: o++, new_no: n++ };
	});
}

const flat = (rows) =>
	rows.map((r) => [
		r.kind,
		r.left && `${r.left.no}:${r.left.text}`,
		r.right && `${r.right.no}:${r.right.text}`,
	]);

test("equal del/add runs pair one to one, context shows on both sides", () => {
	const lines = hunk([" keep", "-old a", "-old b", "+new a", "+new b", " tail"], 10, 10);
	expect(flat(splitRows(lines))).toEqual([
		["ctx", "10:keep", "10:keep"],
		["change", "11:old a", "11:new a"],
		["change", "12:old b", "12:new b"],
		["ctx", "13:tail", "13:tail"],
	]);
});

test("more deletions than additions leave the right side empty for the surplus", () => {
	const lines = hunk(["-a", "-b", "-c", "+x"], 1, 1);
	expect(flat(splitRows(lines))).toEqual([
		["change", "1:a", "1:x"],
		["change", "2:b", null],
		["change", "3:c", null],
	]);
});

test("more additions than deletions leave the left side empty for the surplus", () => {
	const lines = hunk(["-a", "+x", "+y", "+z"], 5, 5);
	expect(flat(splitRows(lines))).toEqual([
		["change", "5:a", "5:x"],
		["change", null, "6:y"],
		["change", null, "7:z"],
	]);
});

test("pure addition and pure deletion hunks have one empty side", () => {
	expect(flat(splitRows(hunk(["+a", "+b"], 0, 1)))).toEqual([
		["change", null, "1:a"],
		["change", null, "2:b"],
	]);
	expect(flat(splitRows(hunk(["-a", "-b"], 1, 0)))).toEqual([
		["change", "1:a", null],
		["change", "2:b", null],
	]);
});

test("each separate change run pairs on its own, never across context", () => {
	const lines = hunk(["-a", " mid", "+b"], 1, 1);
	expect(flat(splitRows(lines))).toEqual([
		["change", "1:a", null],
		["ctx", "2:mid", "1:mid"],
		["change", null, "2:b"],
	]);
});

test("additions before deletions in one run still pair (git emits - first, but order must not matter)", () => {
	const lines = [
		{ kind: "add", text: "x", new_no: 1 },
		{ kind: "del", text: "a", old_no: 1 },
	];
	expect(flat(splitRows(lines))).toEqual([["change", "1:a", "1:x"]]);
});

test("empty hunk gives no rows; every source line is shown exactly once", () => {
	expect(splitRows([])).toEqual([]);
	const lines = hunk([" c", "-a", "-b", "+x", " d", "+y", "+z"], 3, 3);
	const rows = splitRows(lines);
	// invariant: left side holds every ctx+del line, right side every ctx+add line, in order
	expect(rows.flatMap((r) => (r.left ? [r.left.text] : []))).toEqual(
		lines.filter((l) => l.kind !== "add").map((l) => l.text),
	);
	expect(rows.flatMap((r) => (r.right ? [r.right.text] : []))).toEqual(
		lines.filter((l) => l.kind !== "del").map((l) => l.text),
	);
});

test("rows keep the index of the source line so highlighting tokens line up", () => {
	const lines = hunk([" c", "-a", "+x", "+y"], 1, 1);
	const rows = splitRows(lines);
	expect(rows[1].left.idx).toBe(1);
	expect(rows[1].right.idx).toBe(2);
	expect(rows[2].right.idx).toBe(3);
});

test("changedSpan marks only the differing middle of a modified line", () => {
	expect(changedSpan("const a = 1;", "const a = 22;")).toEqual({ from: 10, oldTo: 11, newTo: 12 });
	// identical lines have nothing to mark
	expect(changedSpan("same", "same")).toBeNull();
	// pure insertion: the old side has an empty span, the new side the inserted text
	expect(changedSpan("ab", "aXb")).toEqual({ from: 1, oldTo: 1, newTo: 2 });
	// totally different
	expect(changedSpan("abc", "xyz")).toEqual({ from: 0, oldTo: 3, newTo: 3 });
	// prefix and suffix must not overlap on repeated characters
	expect(changedSpan("aa", "aaa")).toEqual({ from: 2, oldTo: 2, newTo: 3 });
});
