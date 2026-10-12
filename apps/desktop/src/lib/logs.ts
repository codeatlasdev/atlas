/**
 * Pure engine behind the Logs tab: a bounded ring of lines, an incrementally
 * maintained filtered view, search over that view and link detection. No Svelte,
 * no DOM: everything here is unit-tested and measured by `logs.test.ts`.
 *
 * Cost model (n = buffered lines, k = new lines in a batch):
 *  - append: O(k) amortized; the ring trims in slabs, never per line.
 *  - filtered view: O(k) on append (only the new lines meet the filter); a filter
 *    change re-scans in time-sliced chunks (`Refilter`) so typing never blocks.
 *  - search: scans the filtered view lazily, in chunks, and counts matches.
 */

import { stripAnsi } from "#lib/ansi.ts";

export type Level = "error" | "warn" | "info";

export interface LogLine {
	seq: number;
	name: string;
	/** Plain text (escapes removed): search, level, copy. */
	line: string;
	/** Original text when it carried escapes: parsed lazily for visible rows only. */
	raw?: string;
	stream: string;
	ts: number;
	level: Level;
	/** Marker rows (service restart, stack start) instead of output. */
	mark?: string;
	/** Which separator a marker row is: a service `restart` or a user/system `mark`. */
	kind?: "restart" | "mark";
}

/** One log entry as the runner / desktop batch sends it (`seq`/`level`/`kind`/`trunc` need a current runner). */
export interface RawLog {
	name: string;
	line: string;
	stream: string;
	ts: number;
	level?: "debug" | "info" | "warn" | "error";
	kind?: "restart" | "mark";
	trunc?: boolean;
}

export const RANK: Record<Level, number> = { info: 0, warn: 1, error: 2 };

/**
 * Same rule as the Rust `LogLevel::detect` (`atlas-tui/src/runtime/logs.rs`), minus `debug`, which the
 * desktop shows as info: whole-word `error|err|fatal|panic` (error) or `warn|warning` (warn) tokens, not
 * preceded by `/ . - _` (paths, URLs, identifiers). `"0 errors found"` is info. Both sides are tested against
 * `testdata/log-levels.json`.
 */
const LEVEL_RE = /(?<![\p{L}\p{N}_])(error|err|fatal|panic|warn|warning)(?![\p{L}\p{N}_])/giu;
export function levelOf(line: string): Level {
	let level: Level = "info";
	for (const m of line.matchAll(LEVEL_RE)) {
		if (m.index > 0 && "/.-_".includes(line[m.index - 1]!)) continue;
		const found: Level = /^warn/i.test(m[1]!) ? "warn" : "error";
		if (RANK[found] > RANK[level]) level = found;
	}
	return level;
}

// ───────────────────────── ring buffer ─────────────────────────

/**
 * Fixed-capacity line store. Appends push to a plain array and trim in slabs
 * (`slack`) so a full buffer does not shift `cap` entries per line. `base` is the
 * global index of `items[0]`, which stays stable across trims so views can keep
 * absolute positions.
 */
export class Ring<T> {
	items: T[] = [];
	/** Number of items ever dropped from the front. */
	base = 0;
	constructor(
		readonly cap: number,
		readonly slack = Math.max(1, cap >> 3),
	) {}

	get length() {
		return this.items.length;
	}

	/** Append; returns how many items were trimmed from the front (0 most of the time). */
	push(batch: T[]): number {
		for (const x of batch) this.items.push(x);
		if (this.items.length <= this.cap + this.slack) return 0;
		const drop = this.items.length - this.cap;
		this.items = this.items.slice(drop);
		this.base += drop;
		return drop;
	}

	clear() {
		this.base += this.items.length;
		this.items = [];
	}
}

// ───────────────────────── time range ─────────────────────────

/** What the user picked in the range chip. */
export type TimeRange =
	| { kind: "all" }
	| { kind: "last"; minutes: number }
	| { kind: "restart" }
	| { kind: "start" }
	/** Clock times of today, seconds from local midnight; `toS` null = open-ended, inclusive of its whole second. */
	| { kind: "custom"; fromS: number; toS: number | null };

export const ALL_TIME: TimeRange = { kind: "all" };
export const LAST_PRESETS = [1, 5, 15, 60] as const;

/** A range resolved against the ring and the clock; the filter applies it per line. */
export interface TimeWindow {
	/** Lines at ring index below this are out (since restart / since stack start). */
	minAbs: number;
	/** Sliding window length in ms (`cutoff = now - lastMs`), 0 = not sliding. */
	lastMs: number;
	/** Absolute epoch-ms bounds; `toMs` 0 = open. */
	fromMs: number;
	toMs: number;
}

export const NO_WINDOW: TimeWindow = { minAbs: 0, lastMs: 0, fromMs: 0, toMs: 0 };

export function inWindow(w: TimeWindow, ts: number, abs: number, cutoff: number): boolean {
	return abs >= w.minAbs && ts >= cutoff && ts >= w.fromMs && (!w.toMs || ts <= w.toMs);
}

/**
 * Would `w` hide the moment `ts`? Used by "open logs at that moment": a seek target outside the
 * range clears it. `minAbs` is a ring position, so it hides what happened before the line there.
 */
export function windowHides(w: TimeWindow, ring: Ring<LogLine>, ts: number, now: number): boolean {
	const edge = w.minAbs > ring.base ? ring.items[w.minAbs - ring.base]?.ts : undefined;
	return (
		(edge !== undefined && ts < edge) ||
		ts < slideCutoff(w, now) ||
		ts < w.fromMs ||
		(w.toMs > 0 && ts > w.toMs)
	);
}

/** Epoch ms of `seconds` after local midnight of the day containing `now`. */
function localSecond(now: number, seconds: number): number {
	const d = new Date(now);
	d.setHours(0, 0, 0, 0);
	d.setSeconds(seconds);
	return d.getTime();
}

/**
 * Resolve `range` for a view over `only` (`null` = All) at `now`. `restart` starts at the newest
 * restart marker of that service (of any service on All); with none it is the whole ring.
 * `start` starts at the oldest line held. Returns `NO_WINDOW` for `all`.
 */
export function resolveWindow(
	range: TimeRange,
	ring: Ring<LogLine>,
	only: string | null,
	now: number,
): TimeWindow {
	switch (range.kind) {
		case "all":
			return NO_WINDOW;
		case "last":
			return { ...NO_WINDOW, lastMs: range.minutes * 60_000 };
		case "start":
			return { ...NO_WINDOW, minAbs: ring.base };
		case "restart": {
			for (let i = ring.items.length - 1; i >= 0; i--) {
				const l = ring.items[i];
				if (l.kind === "restart" && (!only || l.name === only))
					return { ...NO_WINDOW, minAbs: ring.base + i };
			}
			return { ...NO_WINDOW, minAbs: ring.base };
		}
		case "custom":
			return {
				...NO_WINDOW,
				fromMs: localSecond(now, range.fromS),
				toMs: range.toS === null ? 0 : localSecond(now, range.toS) + 999,
			};
	}
}

/** Cutoff of a sliding window at `now` (0 when not sliding). */
export function slideCutoff(w: TimeWindow, now: number): number {
	return w.lastMs ? Math.max(0, now - w.lastMs) : 0;
}

function parseClock(s: string): number | null {
	const m = /^\s*(\d{1,2}):(\d{1,2})(?::(\d{1,2}))?\s*$/.exec(s);
	if (!m) return null;
	const [h, min, sec] = [Number(m[1]), Number(m[2]), Number(m[3] ?? 0)];
	return h > 23 || min > 59 || sec > 59 ? null : h * 3600 + min * 60 + sec;
}

/**
 * `14:05` (from, open-ended) or `14:05–14:20` (`-`, `–`, `—` or `..` separate; both ends inclusive).
 * Returns the range or the reason it was rejected.
 */
export function parseCustomRange(input: string): { range: TimeRange } | { error: string } {
	const text = input.trim();
	if (!text) return { error: "Type a time: 14:05 or 14:05–14:20" };
	const parts = text.split(/\s*(?:\.\.|[–—-])\s*/);
	if (parts.length > 2) return { error: "Use one range: 14:05–14:20" };
	const fromS = parseClock(parts[0]);
	if (fromS === null) return { error: `Not a time: "${parts[0].trim()}"` };
	if (parts.length === 1 || parts[1].trim() === "") return { range: { kind: "custom", fromS, toS: null } };
	const toS = parseClock(parts[1]);
	if (toS === null) return { error: `Not a time: "${parts[1].trim()}"` };
	if (toS < fromS) return { error: "The end is before the start" };
	return { range: { kind: "custom", fromS, toS } };
}

const hhmm = (s: number) => `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}`;

/** Chip text: `Last 5 min`, `Since restart`, `14:05–14:20`. */
export function rangeLabel(r: TimeRange): string {
	switch (r.kind) {
		case "all":
			return "All time";
		case "last":
			return r.minutes % 60 === 0 ? `Last ${r.minutes / 60} h` : `Last ${r.minutes} min`;
		case "restart":
			return "Since restart";
		case "start":
			return "Since stack start";
		case "custom":
			return r.toS === null ? `From ${hhmm(r.fromS)}` : `${hhmm(r.fromS)}–${hhmm(r.toS)}`;
	}
}

// ───────────────────────── filter ─────────────────────────

export interface Filter {
	/** Service name; `null` = all. */
	only: string | null;
	minLevel: Level;
	/** In filter mode, lines that do not match `matcher` are hidden. */
	matcher: RegExp | null;
	hideNonMatching: boolean;
	/** Time bounds (`NO_WINDOW` = unbounded). */
	win: TimeWindow;
	/** Epoch ms below which a sliding window drops lines; set when the filter is built. */
	cutoff?: number;
}

export const NO_FILTER: Filter = {
	only: null,
	minLevel: "info",
	matcher: null,
	hideNonMatching: false,
	win: NO_WINDOW,
};

/** Whether `l` (at absolute ring index `abs`) passes the filter. Marks respect the service and the time window. */
export function accepts(f: Filter, l: LogLine, abs: number): boolean {
	if (f.win !== NO_WINDOW && !inWindow(f.win, l.ts, abs, f.cutoff ?? 0)) return false;
	if (l.mark !== undefined) return !f.only || l.name === f.only;
	if (f.only && l.name !== f.only) return false;
	if (RANK[l.level] < RANK[f.minLevel]) return false;
	if (f.matcher && f.hideNonMatching) {
		f.matcher.lastIndex = 0;
		return f.matcher.test(l.line);
	}
	return true;
}

/**
 * Compile the search box into a global, case-aware RegExp. Returns `null` for an
 * empty query and `"bad"` for an invalid regex.
 */
export function compileQuery(query: string, regex: boolean, matchCase: boolean): RegExp | null | "bad" {
	if (!query) return null;
	try {
		const src = regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
		const re = new RegExp(src, matchCase ? "g" : "gi");
		// A pattern that matches the empty string (`a*`) would highlight nothing and loop.
		return re.test("") ? "bad" : re;
	} catch {
		return "bad";
	}
}

/**
 * Index list of the lines passing a filter, as *absolute* line numbers (`ring.base + i`).
 * Appends are incremental; `trim` forgets entries the ring dropped.
 */
export class View {
	/** Absolute indexes (ring.base + position) of the visible lines, ascending. */
	idx: number[] = [];
	/** Level rank of each entry (parallel to `idx`), so trimming never needs the dropped lines. */
	#lv: number[] = [];
	/** Timestamp of each entry (parallel to `idx`): a sliding window drops by it without the ring. */
	#ts: number[] = [];
	/** Monotonic queue of `[abs, ts]` with strictly increasing ts: the front is the oldest timestamp. */
	#minTs: [number, number][] = [];
	#minHead = 0;
	/** Per-level counts of the visible lines. */
	errors = 0;
	warns = 0;

	add(f: Filter, ring: Ring<LogLine>, from: number) {
		const { items, base } = ring;
		for (let i = from - base < 0 ? 0 : from - base; i < items.length; i++) {
			if (accepts(f, items[i], base + i)) this.push(items[i], base + i);
		}
	}

	push(l: LogLine, abs: number) {
		const rank = l.mark === undefined ? RANK[l.level] : 0;
		this.idx.push(abs);
		this.#lv.push(rank);
		this.#ts.push(l.ts);
		const q = this.#minTs;
		while (q.length > this.#minHead && q[q.length - 1][1] >= l.ts) q.pop();
		q.push([abs, l.ts]);
		if (rank === 2) this.errors++;
		else if (rank === 1) this.warns++;
	}

	/** Drop entries below `base` (trimmed by the ring); returns how many went. */
	trim(base: number): number {
		let n = 0;
		while (n < this.idx.length && this.idx[n] < base) n++;
		return this.#dropFront(n);
	}

	/**
	 * Move a sliding window's cutoff: drop the entries older than `cutoff` (epoch ms).
	 * Lines arrive in time order, so they are almost always a prefix and this costs O(dropped).
	 * A straggler hiding behind a newer entry (a replayed backlog, a clock step) cannot be
	 * popped from the front: the oldest-timestamp tracker reports it in O(1) and `slide`
	 * answers `"rebuild"`, so the caller re-filters once instead of keeping a stale row.
	 * Returns how many leading rows went, or `"rebuild"`.
	 */
	slide(cutoff: number): number | "rebuild" {
		let n = 0;
		while (n < this.#ts.length && this.#ts[n] < cutoff) n++;
		const dropped = this.#dropFront(n);
		const q = this.#minTs;
		return this.#minHead < q.length && q[this.#minHead][1] < cutoff ? "rebuild" : dropped;
	}

	#dropFront(n: number): number {
		if (!n) return 0;
		for (let i = 0; i < n; i++) {
			if (this.#lv[i] === 2) this.errors--;
			else if (this.#lv[i] === 1) this.warns--;
		}
		const next = this.idx[n]; // first survivor (undefined when everything went)
		this.idx = this.idx.slice(n);
		this.#lv = this.#lv.slice(n);
		this.#ts = this.#ts.slice(n);
		// The tracker forgets entries that left the front (they are all below `next`).
		const q = this.#minTs;
		while (this.#minHead < q.length && (next === undefined || q[this.#minHead][0] < next)) this.#minHead++;
		if (this.#minHead > 64 && this.#minHead * 2 > q.length) {
			this.#minTs = q.slice(this.#minHead);
			this.#minHead = 0;
		}
		return n;
	}

	get length() {
		return this.idx.length;
	}
}

/**
 * Time-sliced rebuild of a `View` after a filter change. `step()` scans for at most
 * `budgetMs` and reports whether it is done; the caller schedules it between frames.
 * Lines that arrive meanwhile are picked up because the scan always runs to the
 * ring's current end.
 */
export class Refilter {
	view = new View();
	#at: number;
	constructor(
		readonly filter: Filter,
		readonly ring: Ring<LogLine>,
	) {
		this.#at = ring.base;
	}

	get done() {
		return this.#at >= this.ring.base + this.ring.length;
	}

	step(budgetMs = 6, chunk = 2048): boolean {
		const start = performance.now();
		while (!this.done) {
			const { items, base } = this.ring;
			// The ring may have trimmed while we were yielding: forget what it dropped.
			if (this.#at < base) this.#at = base;
			this.view.trim(base);
			const end = Math.min(items.length, this.#at - base + chunk);
			for (let i = this.#at - base; i < end; i++) {
				if (accepts(this.filter, items[i], base + i)) this.view.push(items[i], base + i);
			}
			this.#at = base + end;
			if (performance.now() - start > budgetMs) break;
		}
		return this.done;
	}
}

// ───────────────────────── search ─────────────────────────

export interface Hit {
	/** Position inside the view (not the absolute line number). */
	row: number;
	start: number;
	end: number;
}

/** All non-empty matches of `re` in `text`. */
export function matchesIn(re: RegExp, text: string): [number, number][] {
	const out: [number, number][] = [];
	re.lastIndex = 0;
	for (let m = re.exec(text); m; m = re.exec(text)) {
		if (!m[0]) {
			re.lastIndex++;
			continue;
		}
		out.push([m.index, m.index + m[0].length]);
	}
	return out;
}

/**
 * Rows of `view` whose text matches `re`, scanned incrementally. `scan()` advances a
 * cursor in chunks; `rows` are ascending view positions, so next/prev are binary
 * searches. `count` is the number of matching *rows* (not occurrences).
 */
export class Search {
	rows: number[] = [];
	#at = 0;
	constructor(
		readonly re: RegExp,
		readonly view: View,
		readonly ring: Ring<LogLine>,
	) {}

	get done() {
		return this.#at >= this.view.length;
	}

	/** Forget rows below `drop` view entries (the view trimmed its head) and shift the rest. */
	shift(drop: number) {
		if (!drop) return;
		let n = 0;
		while (n < this.rows.length && this.rows[n] < drop) n++;
		this.rows = this.rows.slice(n).map((r) => r - drop);
		this.#at = Math.max(0, this.#at - drop);
	}

	scan(budgetMs = 4, chunk = 4096): boolean {
		const start = performance.now();
		const { idx } = this.view;
		const { items, base } = this.ring;
		while (this.#at < idx.length) {
			const end = Math.min(idx.length, this.#at + chunk);
			for (let r = this.#at; r < end; r++) {
				const l = items[idx[r] - base];
				if (!l || l.mark !== undefined) continue;
				this.re.lastIndex = 0;
				if (this.re.test(l.line)) this.rows.push(r);
			}
			this.#at = end;
			if (performance.now() - start > budgetMs) break;
		}
		return this.done;
	}

	/** First matching row strictly after `row` (wraps), or -1. */
	next(row: number): number {
		const { rows } = this;
		if (!rows.length) return -1;
		let lo = 0;
		let hi = rows.length;
		while (lo < hi) {
			const mid = (lo + hi) >> 1;
			if (rows[mid] <= row) lo = mid + 1;
			else hi = mid;
		}
		return rows[lo === rows.length ? 0 : lo];
	}

	/** Last matching row strictly before `row` (wraps), or -1. */
	prev(row: number): number {
		const { rows } = this;
		if (!rows.length) return -1;
		let lo = 0;
		let hi = rows.length;
		while (lo < hi) {
			const mid = (lo + hi) >> 1;
			if (rows[mid] < row) lo = mid + 1;
			else hi = mid;
		}
		return rows[lo === 0 ? rows.length - 1 : lo - 1];
	}

	/** 1-based ordinal of `row` among the matches (0 when it is not one). */
	ordinal(row: number): number {
		const { rows } = this;
		let lo = 0;
		let hi = rows.length;
		while (lo < hi) {
			const mid = (lo + hi) >> 1;
			if (rows[mid] < row) lo = mid + 1;
			else hi = mid;
		}
		return rows[lo] === row ? lo + 1 : 0;
	}
}

/** Nearest view row after/before `row` holding a line of at least `min` level (wraps), or -1. */
export function jumpLevel(view: View, ring: Ring<LogLine>, row: number, dir: 1 | -1, min: Level): number {
	const n = view.length;
	if (!n) return -1;
	const { items, base } = ring;
	for (let k = 1; k <= n; k++) {
		const r = (((row + dir * k) % n) + n) % n;
		const l = items[view.idx[r] - base];
		if (l && l.mark === undefined && RANK[l.level] >= RANK[min]) return r;
	}
	return -1;
}

/** First view row whose line is at or after `ts` (timestamps ascend with the ring); the last row when all are older, -1 when empty. */
export function rowAtTime(view: View, ring: Ring<LogLine>, ts: number): number {
	const n = view.length;
	if (!n) return -1;
	let lo = 0;
	let hi = n;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		const l = ring.items[view.idx[mid] - ring.base];
		if (l && l.ts < ts) lo = mid + 1;
		else hi = mid;
	}
	return Math.min(lo, n - 1);
}

// ───────────────────────── links ─────────────────────────

export interface LinkSpan {
	start: number;
	end: number;
	kind: "url" | "file";
	/** URL, or the file path as written. */
	target: string;
	line?: number;
	col?: number;
}

const URL_RE = /\bhttps?:\/\/[^\s<>"'`)\]}]+/g;
// `path:line[:col]` or `path(line,col)`; the path needs a slash or a known extension to avoid
// matching `12:34:56` timestamps and `key:value` pairs.
const FILE_RE =
	/(?<![\w@/.:-])((?:\.{0,2}\/|[A-Za-z]:[\\/])?(?:[\w@.~+-]+\/)*[\w@.+-]+\.[A-Za-z][A-Za-z0-9]{0,7})(?::(\d+)(?::(\d+))?|\((\d+)(?:,(\d+))?\))/g;
const TRAIL = /[.,;:!?]+$/;

/** URLs and `file:line[:col]` references in `text`, ordered, non-overlapping. */
export function findLinks(text: string): LinkSpan[] {
	if (!text.includes("/") && !text.includes(".")) return [];
	const out: LinkSpan[] = [];
	if (text.includes("://")) {
		URL_RE.lastIndex = 0;
		for (let m = URL_RE.exec(text); m; m = URL_RE.exec(text)) {
			const target = m[0].replace(TRAIL, "");
			out.push({ start: m.index, end: m.index + target.length, kind: "url", target });
		}
	}
	FILE_RE.lastIndex = 0;
	for (let m = FILE_RE.exec(text); m; m = FILE_RE.exec(text)) {
		const start = m.index;
		const end = start + m[0].length;
		if (out.some((u) => start < u.end && end > u.start)) continue;
		out.push({
			start,
			end,
			kind: "file",
			target: m[1],
			line: Number(m[2] ?? m[4]),
			col: (m[3] ?? m[5]) ? Number(m[3] ?? m[5]) : undefined,
		});
	}
	return out.sort((a, b) => a.start - b.start);
}

/**
 * Worktree-relative path for a file reference, or `null` when it points outside
 * `root`. Relative paths resolve against `serviceDir` (relative to the root) first.
 */
export function resolvePath(target: string, root: string, serviceDir?: string | null): string | null {
	const r = root.replace(/\/+$/, "");
	let abs: string;
	if (target.startsWith("/")) abs = target;
	else if (/^[A-Za-z]:[\\/]/.test(target)) return null;
	else abs = `${r}/${serviceDir ? `${serviceDir.replace(/^\.?\/|\/+$/g, "")}/` : ""}${target}`;
	const parts: string[] = [];
	for (const seg of abs.split("/")) {
		if (seg === "..") parts.pop();
		else if (seg && seg !== ".") parts.push(seg);
	}
	const norm = `/${parts.join("/")}`;
	if (norm !== r && !norm.startsWith(`${r}/`)) return null;
	return norm.slice(r.length + 1) || null;
}

// ───────────────────────── text helpers ─────────────────────────

const pad = (n: number, w = 2) => String(n).padStart(w, "0");

/** `HH:MM:SS` (local time), or with `.mmm` for the clipboard. */
export function hms(ts: number, ms = false): string {
	const d = new Date(ts);
	const t = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
	return ms ? `${t}.${pad(d.getMilliseconds(), 3)}` : t;
}

/** Plain-text export of lines, as `HH:MM:SS [service] text`. */
export function format(lines: LogLine[], withTime = true): string {
	return lines
		.map((l) =>
			l.mark !== undefined ? `── ${l.mark} ──` : `${withTime ? `${hms(l.ts)} ` : ""}[${l.name}] ${l.line}`,
		)
		.join("\n");
}

/** Hue (0–359) for a service name: stable chip/label color. */
export function hue(name: string): number {
	let h = 0;
	for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) % 360;
	return h;
}

/** Estimated wrapped height in rows of a line, given the columns available. */
export function wrappedRows(len: number, cols: number): number {
	return len <= cols || cols < 8 ? 1 : Math.ceil(len / cols);
}

/**
 * Wire entry → buffered line. Escapes are stripped for search/level and the original is kept
 * only when it differs (parsed lazily for visible rows). The runner's level wins, `debug` ranks
 * as info, and an older runner without `level` falls back to `levelOf`, which follows the same word-token rule as the TUI.
 * `restart`/`mark` rows become separators whose text is the line without its `────` rules.
 */
export function toLogLine(r: RawLog, seq: number): LogLine {
	const raw = r.line;
	const line = raw.includes("\x1b") || raw.includes("\r") ? stripAnsi(raw) : raw;
	const level = r.level ? (r.level === "debug" ? "info" : r.level) : levelOf(line);
	const l: LogLine = { seq, name: r.name, line, stream: r.stream, ts: r.ts, level };
	if (line !== raw) l.raw = raw;
	if (r.kind) {
		l.mark = line.replace(/^[─\s]+|[─\s]+$/g, "");
		l.kind = r.kind;
	}
	return l;
}
