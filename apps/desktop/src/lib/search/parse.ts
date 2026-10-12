/** Project search: builds the `rg` / `git grep` command and parses their output (pure, for tests). */

export interface SearchOpts {
	query: string;
	regex: boolean;
	caseSensitive: boolean;
	wholeWord: boolean;
	/** Comma/space separated globs (`src/**`, `*.ts`). */
	include: string;
	/** Same, files to skip (`dist, *.min.js`). */
	exclude: string;
}

export interface SearchMatch {
	line: number;
	/** 0-based UTF-16 offset of the match start inside `text`. */
	start: number;
	end: number;
	text: string;
}

export interface FileResult {
	path: string;
	matches: SearchMatch[];
}

export const MAX_MATCHES = 2000;
export const MAX_FILES = 200;
/** Longest preview line kept (minified bundles would flood the list). */
const MAX_LINE = 400;

/** Split a glob list typed by the user: commas or whitespace, empties dropped. */
export function splitGlobs(s: string): string[] {
	return s
		.split(/[,\s]+/)
		.map((g) => g.trim())
		.filter(Boolean);
}

/** A bare folder (`src`, `src/`) matches everything under it, like VS Code. */
export function normalizeGlob(g: string): string {
	if (/[*?[{]/.test(g)) return g;
	return g.endsWith("/") ? `${g}**` : g.includes(".") ? g : `${g}/**`;
}

export function rgArgs(o: SearchOpts): string[] {
	const a = [
		"rg",
		"--json",
		"--no-config",
		"--max-columns",
		String(MAX_LINE),
		"--max-columns-preview",
		"--max-count",
		"200",
	];
	a.push(o.caseSensitive ? "--case-sensitive" : "--ignore-case");
	if (!o.regex) a.push("--fixed-strings");
	if (o.wholeWord) a.push("--word-regexp");
	for (const g of splitGlobs(o.include)) a.push("--glob", normalizeGlob(g));
	for (const g of splitGlobs(o.exclude)) a.push("--glob", `!${normalizeGlob(g)}`);
	a.push("--glob", "!.git", "--", o.query, ".");
	return a;
}

/** `git grep` fallback: works on tracked files only, no per-match columns from the tool. */
export function gitGrepArgs(o: SearchOpts): string[] {
	const a = ["git", "grep", "-n", "-I", "--no-color", "--untracked"];
	if (!o.caseSensitive) a.push("-i");
	a.push(o.regex ? "-E" : "-F");
	if (o.wholeWord) a.push("-w");
	a.push("-e", o.query, "--");
	const inc = splitGlobs(o.include).map((g) => `:(glob)${normalizeGlob(g)}`);
	a.push(...(inc.length ? inc : ["."]));
	for (const g of splitGlobs(o.exclude)) a.push(`:(exclude,glob)${normalizeGlob(g)}`);
	return a;
}

const dec = new TextDecoder();

/** rg `--json` text/bytes object → string. */
function rgText(v: { text?: string; bytes?: string } | undefined): string {
	if (!v) return "";
	if (v.text !== undefined) return v.text;
	if (v.bytes !== undefined) {
		try {
			return dec.decode(Uint8Array.from(atob(v.bytes), (c) => c.charCodeAt(0)));
		} catch {
			return "";
		}
	}
	return "";
}

/** UTF-8 byte offset → UTF-16 index of `s` (rg reports submatch offsets in bytes). */
export function byteToIndex(s: string, byteOff: number): number {
	if (byteOff <= 0) return 0;
	let bytes = 0;
	let i = 0;
	while (i < s.length && bytes < byteOff) {
		const cp = s.codePointAt(i)!;
		bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
		i += cp > 0xffff ? 2 : 1;
	}
	return i;
}

const clean = (p: string) => p.replace(/^\.\//, "");

function trimLine(text: string, start: number, end: number): { text: string; start: number; end: number } {
	const t = text.replace(/\r?\n$/, "");
	if (t.length <= MAX_LINE) return { text: t, start, end };
	const from = Math.max(0, Math.min(start - 40, t.length - MAX_LINE));
	return {
		text: (from ? "…" : "") + t.slice(from, from + MAX_LINE),
		start: start - from + (from ? 1 : 0),
		end: end - from + (from ? 1 : 0),
	};
}

/** Accumulates results keeping the limits; `add*` return false once a limit is hit. */
export class Results {
	files: FileResult[] = [];
	matches = 0;
	truncated = false;
	private byPath = new Map<string, FileResult>();

	private push(path: string, m: SearchMatch): boolean {
		if (this.matches >= MAX_MATCHES) return ((this.truncated = true), false);
		let f = this.byPath.get(path);
		if (!f) {
			if (this.files.length >= MAX_FILES) return ((this.truncated = true), false);
			f = { path, matches: [] };
			this.byPath.set(path, f);
			this.files.push(f);
		}
		f.matches.push(m);
		this.matches++;
		return true;
	}

	/** One line of `rg --json`; returns false when limits stop the search. */
	addRgLine(line: string): boolean {
		if (!line) return true;
		let ev: {
			type: string;
			data: {
				path?: { text?: string; bytes?: string };
				lines?: { text?: string; bytes?: string };
				line_number?: number;
				submatches?: { start: number; end: number }[];
			};
		};
		try {
			ev = JSON.parse(line);
		} catch {
			return true;
		}
		if (ev.type !== "match") return true;
		const path = clean(rgText(ev.data.path));
		const text = rgText(ev.data.lines);
		for (const s of ev.data.submatches ?? [{ start: 0, end: 0 }]) {
			const start = byteToIndex(text, s.start);
			const end = byteToIndex(text, s.end);
			const t = trimLine(text, start, end);
			if (!this.push(path, { line: ev.data.line_number ?? 1, ...t })) return false;
		}
		return true;
	}

	/** One `path:line:text` line of `git grep -n`; the match range is found in the text with `locate`. */
	addGrepLine(line: string, locate: (text: string) => [number, number][]): boolean {
		const m = /^(.+?):(\d+):(.*)$/.exec(line);
		if (!m) return true;
		const text = m[3];
		for (const [start, end] of locate(text).length ? locate(text) : [[0, 0] as [number, number]]) {
			const t = trimLine(text, start, end);
			if (!this.push(clean(m[1]), { line: Number(m[2]), ...t })) return false;
		}
		return true;
	}
}

/** Finds match ranges in a line for the git-grep fallback (mirrors the query options). */
export function locator(o: SearchOpts): (text: string) => [number, number][] {
	const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
	let src = o.regex ? o.query : esc(o.query);
	if (o.wholeWord) src = `\\b(?:${src})\\b`;
	let re: RegExp;
	try {
		re = new RegExp(src, o.caseSensitive ? "gu" : "giu");
	} catch {
		return () => [];
	}
	return (text) => {
		const out: [number, number][] = [];
		for (const m of text.matchAll(re)) {
			if (m[0].length) out.push([m.index, m.index + m[0].length]);
		}
		return out;
	};
}

/** rg exits 1 for "no match" and 2 for errors (an invalid regex lands here with stderr). */
export function rgError(code: number | null, stderr: string): string | null {
	if (code === 0 || code === 1 || code === null) return null;
	const first = stderr.split("\n").find((l) => l.trim());
	return first ? first.replace(/^rg: /, "") : `Search failed (exit ${code})`;
}
