/**
 * Time-range filtering: what the user picks in the range chip, how it resolves against the ring
 * and the clock, and the custom `HH:MM[–HH:MM]` parser. Pure.
 */

import { pad } from "./text.ts";
import type { LogLine, Ring } from "./model.ts";
import { m } from "#lib/i18n/m.ts";

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
	const match = /^\s*(\d{1,2}):(\d{1,2})(?::(\d{1,2}))?\s*$/.exec(s);
	if (!match) return null;
	const [h, min, sec] = [Number(match[1]), Number(match[2]), Number(match[3] ?? 0)];
	return h > 23 || min > 59 || sec > 59 ? null : h * 3600 + min * 60 + sec;
}

/**
 * `14:05` (from, open-ended) or `14:05–14:20` (`-`, `–`, `—` or `..` separate; both ends inclusive).
 * Returns the range or the reason it was rejected.
 */
export function parseCustomRange(input: string): { range: TimeRange } | { error: string } {
	const text = input.trim();
	if (!text) return { error: m.logs_range_error_type_time() };
	const parts = text.split(/\s*(?:\.\.|[–—-])\s*/);
	if (parts.length > 2) return { error: m.logs_range_error_one_range() };
	const fromS = parseClock(parts[0]);
	if (fromS === null) return { error: m.logs_range_error_not_a_time({ text: parts[0].trim() }) };
	if (parts.length === 1 || parts[1].trim() === "") return { range: { kind: "custom", fromS, toS: null } };
	const toS = parseClock(parts[1]);
	if (toS === null) return { error: m.logs_range_error_not_a_time({ text: parts[1].trim() }) };
	if (toS < fromS) return { error: m.logs_range_error_end_before_start() };
	return { range: { kind: "custom", fromS, toS } };
}

const hhmm = (s: number) => `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}`;

/** Chip text: `Last 5 min`, `Since restart`, `14:05–14:20`. */
export function rangeLabel(r: TimeRange): string {
	switch (r.kind) {
		case "all":
			return m.logs_range_all_time();
		case "last":
			return r.minutes % 60 === 0
				? m.logs_range_last_hours({ hours: r.minutes / 60 })
				: m.logs_range_last_minutes({ minutes: r.minutes });
		case "restart":
			return m.logs_range_since_restart();
		case "start":
			return m.logs_range_since_stack_start();
		case "custom":
			return r.toS === null ? m.logs_range_from({ time: hhmm(r.fromS) }) : `${hhmm(r.fromS)}–${hhmm(r.toS)}`;
	}
}
