/**
 * Recording HUD logic (`routes/shots/hud`): what the pill shows for a `RecordState`, and what a key does.
 * Pure: Rust owns the clock (`elapsedMs` is frozen while paused), the page only renders and forwards.
 *
 * The HUD state is the shared `RecordState` plus what Rust's `RecordState` also sends for the recording options
 * (`HudState`): the size estimate, the notes of skipped options, the GIF encode progress and the trim step.
 */
import type { RecordState } from "./types.ts";

/** The extra keys of Rust's `RecordState` (`shots/record.rs`), serialized camelCase. */
export interface RecordExtras {
	/** About how many bytes the mp4 is by now; null for a GIF and outside recording/paused. */
	estimateBytes?: number | null;
	/** Sentences about options that were asked for and skipped (no permission, old macOS). */
	notes?: string[];
	/** GIF frames encoded so far, while saving a GIF. */
	progress?: { done: number; total: number } | null;
	/** The trim step after Stop. */
	trim?: { secs: number; posters: string[] } | null;
}

export type HudState = Omit<RecordState, "phase"> & RecordExtras & { phase: Phase };

export type Phase = RecordState["phase"] | "trim";
export type Tone = "rec" | "warn" | "idle" | "ok" | "error";
export type KeyAction = "stop" | "cancel" | "pause" | "resume" | "open" | "close" | "save";

/** The last stretch of a recording that shows the time left. */
export const WARN_MS = 30_000;
const COUNTDOWN_MS = 3000;

export interface RecordView {
	label: string;
	/** `m:ss`, `h:mm:ss` from one hour. Frozen while paused (Rust stops `elapsedMs`). */
	timer: string;
	/** `0:23 left` in the last 30 s of a recording or pause, otherwise null. */
	remaining: string | null;
	/** `≈ 12.4 MB` while an mp4 is recorded, otherwise null. */
	estimate: string | null;
	/** One sentence per option that was asked for and skipped. */
	notes: string[];
	/** The big 3-2-1 of the countdown, otherwise null. */
	digit: number | null;
	showStop: boolean;
	showPause: boolean;
	showResume: boolean;
	showCancel: boolean;
	showOpen: boolean;
	showClose: boolean;
	/** The trim step: the timeline, Save and Discard. */
	showTrim: boolean;
	/** Saving: nothing can be pressed. */
	disabled: boolean;
	tone: Tone;
	/** The red dot pulses only while actually recording, never paused. */
	pulse: boolean;
	/** Indeterminate progress (saving without frame counts). */
	progress: boolean;
	/** Determinate GIF encode progress, 0..1; null when unknown. Esc cancels it. */
	encode: number | null;
}

/** `m:ss`, `h:mm:ss` from one hour; junk is 0:00. */
export function formatTimer(ms: number): string {
	const total = Number.isFinite(ms) && ms > 0 ? Math.floor(ms / 1000) : 0;
	const h = Math.floor(total / 3600);
	const m = Math.floor((total % 3600) / 60);
	const s = String(total % 60).padStart(2, "0");
	return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** 3,3,2,1,1 across 0..3500 ms: held at 1 (never 0) until Rust flips the phase. */
export function countdownDigit(elapsedMs: number): number {
	const ms = Number.isFinite(elapsedMs) ? elapsedMs : 0;
	return Math.min(3, Math.max(1, Math.ceil((COUNTDOWN_MS - ms) / 1000)));
}

/** `≈ 12.4 MB`: decimal units like Finder; "about" because the encoder decides the real size. */
export function formatEstimate(bytes: number | null | undefined): string | null {
	if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return null;
	if (bytes < 1000) return `≈ ${Math.round(bytes)} B`;
	if (bytes < 1_000_000) return `≈ ${Math.round(bytes / 1000)} KB`;
	if (bytes < 1_000_000_000) return `≈ ${(bytes / 1_000_000).toFixed(1)} MB`;
	return `≈ ${(bytes / 1_000_000_000).toFixed(2)} GB`;
}

function labelOf(s: HudState): string {
	switch (s.phase) {
		case "countdown":
			return "Starting";
		case "recording":
			return s.kind === "gif" ? "Recording GIF" : "Recording";
		case "paused":
			return "Paused";
		case "trim":
			return "Trim, then save";
		case "saving":
			return s.progress && s.progress.total > 0
				? `Encoding GIF ${Math.round(encodeFraction(s.progress) * 100)}%`
				: "Saving…";
		case "done":
			return s.message || "Saved";
		case "error":
			return s.message || "Recording failed";
		default:
			return "";
	}
}

/** `done / total` clamped to 0..1; junk is 0. */
export function encodeFraction(p: { done: number; total: number } | null | undefined): number {
	if (!p || !(p.total > 0) || !Number.isFinite(p.done)) return 0;
	return Math.min(1, Math.max(0, p.done / p.total));
}

export function view(s: HudState): RecordView {
	const live = s.phase === "recording" || s.phase === "paused";
	const left = s.maxMs > 0 ? Math.max(0, s.maxMs - s.elapsedMs) : Infinity;
	const warn = live && left <= WARN_MS;
	let tone: Tone = "idle";
	if (s.phase === "recording") tone = warn ? "warn" : "rec";
	else if (s.phase === "paused") tone = warn ? "warn" : "idle";
	else if (s.phase === "done") tone = "ok";
	else if (s.phase === "error") tone = "error";
	const encoding = s.phase === "saving" && !!s.progress && s.progress.total > 0;
	return {
		label: labelOf(s),
		timer: formatTimer(s.elapsedMs),
		// Ceil: the last second reads 0:01, never 0:00 before the recording has really ended.
		remaining: warn ? `${formatTimer(Math.ceil(left / 1000) * 1000)} left` : null,
		// Only an mp4 in progress has an estimate (Rust sends null for a GIF).
		estimate: live || s.phase === "saving" ? formatEstimate(s.estimateBytes) : null,
		notes: s.notes ?? [],
		digit: s.phase === "countdown" ? countdownDigit(s.elapsedMs) : null,
		showStop: live,
		showPause: s.phase === "recording",
		showResume: s.phase === "paused",
		showCancel: live || s.phase === "countdown",
		showOpen: s.phase === "done" && !!s.entry,
		showClose: s.phase === "done" || s.phase === "error",
		showTrim: s.phase === "trim" && !!s.trim,
		disabled: s.phase === "saving",
		tone,
		pulse: s.phase === "recording",
		progress: s.phase === "saving" && !encoding,
		encode: encoding ? encodeFraction(s.progress) : null,
	};
}

/**
 * What a key does. Esc discards (countdown, recording, paused), Enter stops (recording, paused), `p` toggles pause.
 * After the end, Esc closes and Enter opens (or closes on an error). In the trim step Enter saves and Esc does
 * nothing (a finished clip is never thrown away by a stray Esc: Discard is a button). Saving ignores every key
 * except Esc while a GIF is being encoded (`encoding`), which stops the encode. Chords are not ours.
 */
export function keyAction(
	key: string,
	phase: Phase,
	mods: { meta?: boolean; ctrl?: boolean; alt?: boolean } = {},
	ctx: { encoding?: boolean } = {},
): KeyAction | null {
	if (mods.meta || mods.ctrl || mods.alt) return null;
	const live = phase === "recording" || phase === "paused";
	if (phase === "trim") return key === "Enter" ? "save" : null;
	if (phase === "saving") return key === "Escape" && ctx.encoding ? "cancel" : null;
	if (key === "Escape") {
		if (live || phase === "countdown") return "cancel";
		return phase === "done" || phase === "error" ? "close" : null;
	}
	if (key === "Enter") {
		if (live) return "stop";
		if (phase === "done") return "open";
		return phase === "error" ? "close" : null;
	}
	if (key === "p" || key === "P") {
		if (phase === "recording") return "pause";
		return phase === "paused" ? "resume" : null;
	}
	return null;
}

// ───────────────────────────── trim ─────────────────────────────
//
// The same rules as `clamp_trim` in `shots/recopts.rs` (the file Rust writes is cut by that function; the page
// shows the range with this one so what you see is what is kept).

/** A trim never leaves less than this many seconds. */
export const MIN_TRIM_SECS = 0.5;
const TRIM_SNAP_SECS = 0.15;

/** The range kept for handles dragged to `start` and `end` of a clip of `duration` seconds; null if untrimmable. */
export function clampTrim(duration: number, start: number, end: number): [number, number] | null {
	if (![duration, start, end].every(Number.isFinite) || duration < MIN_TRIM_SECS) return null;
	const snap = (v: number) => {
		const c = Math.min(Math.max(v, 0), duration);
		if (c <= TRIM_SNAP_SECS) return 0;
		if (duration - c <= TRIM_SNAP_SECS) return duration;
		return c;
	};
	let s = snap(Math.min(start, end));
	let e = snap(Math.max(start, end));
	if (e - s < MIN_TRIM_SECS) {
		const mid = (s + e) / 2;
		s = mid - MIN_TRIM_SECS / 2;
		e = s + MIN_TRIM_SECS;
		if (s < 0) {
			s = 0;
			e = MIN_TRIM_SECS;
		} else if (e > duration) {
			e = duration;
			s = duration - MIN_TRIM_SECS;
		}
	}
	return [s, e];
}

/** A pointer position (`x` within a track `left..left+width`) as seconds of a clip, clamped to the clip. */
export function secsAt(x: number, left: number, width: number, duration: number): number {
	if (!(width > 0) || !(duration > 0) || !Number.isFinite(x)) return 0;
	return Math.min(1, Math.max(0, (x - left) / width)) * duration;
}

/**
 * Move one handle to `secs` without letting it cross the other or leave less than the minimum: the live range
 * while dragging (the final cut is `clampTrim` on release).
 */
export function moveHandle(
	which: "start" | "end",
	secs: number,
	range: [number, number],
	duration: number,
): [number, number] {
	const [s, e] = range;
	if (which === "start") return [Math.min(Math.max(0, secs), Math.max(0, e - MIN_TRIM_SECS)), e];
	return [s, Math.max(Math.min(duration, secs), Math.min(duration, s + MIN_TRIM_SECS))];
}

/** `0.0`..`1.0` of the track for `secs`, for positioning handles. */
export function fractionOf(secs: number, duration: number): number {
	return duration > 0 ? Math.min(1, Math.max(0, secs / duration)) : 0;
}

/** `m:ss.t` for a trim handle: the tenth matters when cutting. */
export function formatSecs(secs: number): string {
	const t = Number.isFinite(secs) && secs > 0 ? Math.floor(secs * 10) / 10 : 0;
	const m = Math.floor(t / 60);
	const s = (t - m * 60).toFixed(1).padStart(4, "0");
	return `${m}:${s}`;
}

/** What Save sends: nothing when the whole clip is kept (no re-write), the clamped range otherwise. */
export function saveArgs(duration: number, range: [number, number]): { start?: number; end?: number } {
	const r = clampTrim(duration, range[0], range[1]);
	if (!r || (r[0] <= 0 && r[1] >= duration)) return {};
	return { start: r[0], end: r[1] };
}
