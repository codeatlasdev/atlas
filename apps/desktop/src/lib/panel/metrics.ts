/**
 * Pure engine behind the project panel's charts: fixed-resolution ring buffers of
 * per-service samples (cpu, memory, log throughput, errors), state bands and the
 * aggregates the KPI row shows. No Svelte, no DOM: the store in `metrics.svelte.ts`
 * feeds it and bumps one revision counter.
 */

/** Resolution of the history: one slot per `STEP_MS`. */
export const STEP_MS = 3000;
/** Slots kept per series: 200 × 3 s = 10 minutes. */
export const SLOTS = 200;

/** Fixed-length numeric ring addressed by absolute slot index (`ts / STEP_MS`). */
export class Track {
	/** Absolute index of the newest slot written (-1 when empty). */
	head = -1;
	readonly data = new Float64Array(SLOTS);

	/** Write `v` at `slot`; slots skipped since the last write are filled with `gap` (or carried when `carry`). */
	set(slot: number, v: number, carry = false) {
		if (slot < this.head) return;
		if (this.head >= 0 && slot > this.head) {
			const from = Math.max(this.head + 1, slot - SLOTS + 1);
			const fill = carry ? this.data[this.head % SLOTS] : 0;
			for (let s = from; s < slot; s++) this.data[s % SLOTS] = fill;
		}
		this.data[slot % SLOTS] = v;
		this.head = slot;
	}

	/** Add to the current slot (`slot` must be ≥ head); new slots start at 0. */
	add(slot: number, v: number) {
		if (slot < this.head) slot = this.head;
		if (slot > this.head) this.set(slot, 0);
		this.data[slot % SLOTS] += v;
	}

	/** The last `n` slots ending at `end` (defaults to head), oldest first; missing history is 0. */
	last(n: number, end = this.head): number[] {
		const out = new Array<number>(n);
		for (let i = 0; i < n; i++) {
			const s = end - (n - 1 - i);
			out[i] = s < 0 || s > this.head || s <= this.head - SLOTS ? 0 : this.data[s % SLOTS];
		}
		return out;
	}
}

export type Band = "running" | "starting" | "failed" | "stopped";

/** Maps a runner state to its band; anything not up or coming up is "stopped". */
export function bandOf(state: string): Band {
	return state === "running" || state === "starting" || state === "failed" ? state : "stopped";
}

/** One contiguous stretch of a state band. `end` is null while it is the current one. */
export interface Segment {
	band: Band;
	start: number;
	end: number | null;
}

/** Per-service history. */
export class ServiceHistory {
	readonly cpu = new Track();
	readonly mem = new Track();
	/** Log lines per slot. */
	readonly lines = new Track();
	/** Error-level lines per slot. */
	readonly errors = new Track();
	segments: Segment[] = [];

	/** Record a state at `ts`; consecutive equal bands merge. Segments older than the window drop. */
	state(band: Band, ts: number) {
		const cur = this.segments.at(-1);
		if (cur?.band === band) return;
		if (cur) cur.end = ts;
		this.segments.push({ band, start: ts, end: null });
		const cutoff = ts - SLOTS * STEP_MS;
		while (this.segments.length > 1 && (this.segments[0].end ?? ts) < cutoff) this.segments.shift();
	}
}

export const slotOf = (ts: number) => Math.floor(ts / STEP_MS);

/**
 * Incremental per-service counter over a log ring: remembers the absolute index of the next
 * unseen line, so each update only walks the lines appended since (never a full scan).
 */
export class LogCursor {
	/** Absolute index (`ring.base + i`) of the next line to read. */
	next = 0;

	/**
	 * Feed new lines from `ring` to `onLine`. When the ring dropped lines we never saw
	 * (`base > next`), the cursor jumps to the oldest kept line. Returns how many lines were read.
	 */
	read<T>(ring: { items: T[]; base: number }, onLine: (l: T) => void): number {
		if (this.next < ring.base) this.next = ring.base;
		const start = this.next - ring.base;
		const end = ring.items.length;
		for (let i = start; i < end; i++) onLine(ring.items[i]);
		this.next = ring.base + end;
		return Math.max(0, end - start);
	}

	/** After a resync the ring object is replaced: forget the old position. */
	reset() {
		this.next = 0;
	}
}

/** Total of the latest value of each track (e.g. memory across services). */
export function sumLatest(tracks: Track[]): number {
	let t = 0;
	for (const k of tracks) if (k.head >= 0) t += k.data[k.head % SLOTS];
	return t;
}

/** Element-wise sum of the last `n` slots of several tracks (same `end`). */
export function stackSum(tracks: Track[], n: number, end: number): number[] {
	const out = new Array<number>(n).fill(0);
	for (const k of tracks) {
		const v = k.last(n, end);
		for (let i = 0; i < n; i++) out[i] += v[i];
	}
	return out;
}

/** Sum over the last `windowMs` ending at `end` slot. */
export function windowSum(track: Track, windowMs: number, end: number): number {
	const n = Math.max(1, Math.round(windowMs / STEP_MS));
	let t = 0;
	for (const v of track.last(n, end)) t += v;
	return t;
}

/** `Mm Ss` / `Hh Mm` / `Dd Hh` for a duration in ms; `—` when negative or unknown. */
export function fmtUptime(ms: number | null): string {
	if (ms == null || ms < 0 || !Number.isFinite(ms)) return "—";
	const s = Math.floor(ms / 1000);
	if (s < 60) return `${s}s`;
	const m = Math.floor(s / 60);
	if (m < 60) return `${m}m ${s % 60}s`;
	const h = Math.floor(m / 60);
	if (h < 24) return `${h}h ${m % 60}m`;
	return `${Math.floor(h / 24)}d ${h % 24}h`;
}

/** `HH:MM` labels for the x axis of the last `n` slots ending at `end`. */
export function slotLabels(n: number, end: number): string[] {
	const out = new Array<string>(n);
	for (let i = 0; i < n; i++) {
		const d = new Date((end - (n - 1 - i)) * STEP_MS);
		out[i] =
			`${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}`;
	}
	return out;
}

/** Health summary of a set of service states. */
export type Tone = "ok" | "warn" | "fail" | "off";

export function summarize(states: string[], running: boolean): { tone: Tone; label: string } {
	if (!running) return { tone: "off", label: "Stopped" };
	let up = 0;
	let starting = 0;
	let failed = 0;
	for (const s of states) {
		if (s === "running") up++;
		else if (s === "starting") starting++;
		else if (s === "failed") failed++;
	}
	if (failed) return { tone: "fail", label: `${failed} failed` };
	if (starting) return { tone: "warn", label: `${starting} starting` };
	if (states.length && up === states.length) return { tone: "ok", label: "All healthy" };
	return { tone: "warn", label: `${up} of ${states.length} up` };
}
