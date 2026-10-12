import type { FileResult, SearchMatch } from "#lib/search/parse.ts";

export type Row =
	| { kind: "file"; key: string; path: string; count: number }
	| { kind: "match"; key: string; path: string; match: SearchMatch };

/** Flat list for keyboard navigation: a header per file followed by its matches (unless collapsed). */
export function flatRows(files: FileResult[], collapsed: Record<string, boolean>): Row[] {
	const out: Row[] = [];
	for (const f of files) {
		out.push({ kind: "file", key: f.path, path: f.path, count: f.matches.length });
		if (collapsed[f.path]) continue;
		f.matches.forEach((m, i) =>
			out.push({ kind: "match", key: `${f.path}:${m.line}:${m.start}:${i}`, path: f.path, match: m }),
		);
	}
	return out;
}

/** Preview text split around the match; leading indentation is dropped so more of the line shows. */
export function previewRuns(m: SearchMatch): { text: string; hit: boolean }[] {
	const lead = m.text.length - m.text.trimStart().length;
	const start = Math.max(0, m.start - lead);
	const end = Math.max(start, m.end - lead);
	const t = m.text.slice(lead);
	if (end <= start) return [{ text: t, hit: false }];
	return [
		{ text: t.slice(0, start), hit: false },
		{ text: t.slice(start, end), hit: true },
		{ text: t.slice(end), hit: false },
	].filter((r) => r.text);
}
