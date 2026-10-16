/** Side-by-side layout of one hunk: pairs deletions with additions into rows, context on both sides. */

import type { DiffHunk } from "#lib/api.ts";

type Line = DiffHunk["lines"][number];

/** One cell. `idx` is the line's index in `hunk.lines` (keys the highlight tokens). */
export interface Cell {
	idx: number;
	no: number | undefined;
	text: string;
}

export interface SplitRow {
	kind: "ctx" | "change";
	left: Cell | null;
	right: Cell | null;
}

/**
 * A run of consecutive del/add lines becomes rows pairing the n-th deletion with the n-th addition;
 * the surplus side of an uneven run gets empty cells. Context lines sit on both sides.
 */
export function splitRows(lines: Line[]): SplitRow[] {
	const rows: SplitRow[] = [];
	let dels: Cell[] = [];
	let adds: Cell[] = [];
	const flush = () => {
		for (let i = 0; i < Math.max(dels.length, adds.length); i++)
			rows.push({ kind: "change", left: dels[i] ?? null, right: adds[i] ?? null });
		dels = [];
		adds = [];
	};
	for (const [idx, l] of lines.entries()) {
		if (l.kind === "del") dels.push({ idx, no: l.old_no, text: l.text });
		else if (l.kind === "add") adds.push({ idx, no: l.new_no, text: l.text });
		else {
			flush();
			rows.push({
				kind: "ctx",
				left: { idx, no: l.old_no, text: l.text },
				right: { idx, no: l.new_no, text: l.text },
			});
		}
	}
	flush();
	return rows;
}

/**
 * Differing middle of a modified line, as offsets: `[from, oldTo)` in the old text and `[from, newTo)`
 * in the new one (common prefix and suffix trimmed, never overlapping). `null` when the texts are equal.
 */
export function changedSpan(a: string, b: string): { from: number; oldTo: number; newTo: number } | null {
	if (a === b) return null;
	let from = 0;
	while (from < a.length && from < b.length && a[from] === b[from]) from++;
	let oldTo = a.length;
	let newTo = b.length;
	while (oldTo > from && newTo > from && a[oldTo - 1] === b[newTo - 1]) {
		oldTo--;
		newTo--;
	}
	return { from, oldTo, newTo };
}
