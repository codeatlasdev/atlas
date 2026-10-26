/**
 * DiffView as a flat list of rows for `VirtualList`: file header, hunk header, a table of at most `CHUNK` lines,
 * a note for a binary file. A hunk of 20k lines is therefore a few hundred small rows, not one node tree, and the
 * list only mounts the ones near the viewport.
 */
import type { DiffHunk, FileDiff } from "#lib/api.ts";

/** Lines per table row: small enough that one row is cheap to mount, big enough to keep the row count low. */
export const CHUNK = 60;

export type DiffRow =
	| { kind: "file"; key: string; f: FileDiff }
	| { kind: "binary"; key: string; f: FileDiff }
	| { kind: "hunk"; key: string; f: FileDiff; hunk: DiffHunk; hi: number }
	| { kind: "table"; key: string; f: FileDiff; hunk: DiffHunk; hi: number; tokens: string[] | undefined };

/** `tokens` is keyed `path#hunkIndex` and holds highlighted HTML per line of that hunk. */
export function diffRows(files: readonly FileDiff[], tokens: Record<string, string[]> = {}): DiffRow[] {
	const out: DiffRow[] = [];
	for (const f of files) {
		out.push({ kind: "file", key: `f:${f.path}`, f });
		if (f.binary) out.push({ kind: "binary", key: `b:${f.path}`, f });
		f.hunks.forEach((hunk, hi) => {
			out.push({ kind: "hunk", key: `h:${f.path}#${hi}`, f, hunk, hi });
			const tk = tokens[`${f.path}#${hi}`];
			for (let at = 0; at < hunk.lines.length; at += CHUNK) {
				const piece =
					hunk.lines.length <= CHUNK ? hunk : { ...hunk, lines: hunk.lines.slice(at, at + CHUNK) };
				out.push({
					kind: "table",
					key: `t:${f.path}#${hi}:${at}`,
					f,
					hunk: piece,
					hi,
					tokens: tk && (hunk.lines.length <= CHUNK ? tk : tk.slice(at, at + CHUNK)),
				});
			}
		});
	}
	return out;
}

export const fileRowIndex = (rows: readonly DiffRow[], path: string) =>
	rows.findIndex((r) => r.kind === "file" && r.f.path === path);
