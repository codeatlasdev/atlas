/**
 * Trackpad scroll traces: the raw `NSEvent` scroll stream (what `swipe_mac.rs` sees before WKWebView),
 * as data. One format for three producers and two consumers:
 *
 *  - producers: `atlas desktop debug gesture-record` (the user's real trackpad), the physics synthesizer
 *    below (`synth*`), and hand-written edge cases;
 *  - consumers: the Rust engine (`swipe_engine.rs`, same JSON in its tests) and `scripts/gesture-bench.ts`
 *    (replays into the running app, asserts the outcome).
 *
 * Deltas are in points with the sign AppKit reports: `scrollingDeltaX < 0` = fingers moved left
 * (natural scrolling) = the *next* page, exactly as `NSEvent.scrollingDeltaX`. `t` is ms from the first event.
 */

export type ScrollPhase = "none" | "mayBegin" | "began" | "changed" | "ended" | "cancelled";
export type MomentumPhase = "none" | "began" | "changed" | "ended";

export interface ScrollSample {
	/** ms since the first event of the trace. */
	t: number;
	phase: ScrollPhase;
	momentum: MomentumPhase;
	dx: number;
	dy: number;
	/** Trackpad / Magic Mouse (point deltas) as opposed to a wheel mouse (line deltas). */
	precise: boolean;
	/** Pointer position in the window, top-left origin, CSS px. */
	x: number;
	y: number;
}

/** What the app is expected to end up with. `space`/`tab` are offsets from where the gesture began. */
export interface Expect {
	/** Where the pointer is: decides which zone owns the gesture. */
	zone: "space" | "tab" | "history" | "none";
	/** What the pointer is over; the bench aims every sample at that zone of the live layout. */
	aim?: "sidebar" | "strip" | "history" | "outside";
	/** Net committed steps over the whole trace: +1 next, -1 previous, 0 stay. One gesture is never more than one. */
	steps: -2 | -1 | 0 | 1 | 2;
	/** The page the gesture must end on (a space id), when the bench knows it. */
	expectedEnd?: string;
	/** Distinct gestures the engine claims (default: 1 when `steps` ≠ 0, else not checked). A gesture = fingers down → lift. */
	gestures?: number;
	/** The gesture may not scroll/swipe anything at all (vertical, jitter, no zone). */
	untouched?: boolean;
	/** The first/last page: only a rubber band, never a commit. */
	edge?: "first" | "last";
	/**
	 * The pointer starts over a horizontal scroller (an editor with a long line) that can still go this way: `left`
	 * reveals more of its left side (fingers moving right), `right` of its right side (fingers moving left). The
	 * engine decides who owns the gesture from this when the fingers land; the bench puts such a scroller under the
	 * aimed pointer for the replay.
	 */
	scroller?: { left: boolean; right: boolean };
}

export interface Trace {
	name: string;
	/** Free-form: where it came from (`synth:flick`, `recorded:2026-10-13 user trackpad`), device, notes. */
	source: string;
	expect: Expect;
	events: ScrollSample[];
}

/** The 125 Hz rate of a MacBook trackpad's scroll samples (8 ms), what `NSEvent` delivers. */
export const SAMPLE_MS = 8;

/** Deterministic PRNG (mulberry32): a trace must be the same bytes on every machine. */
export function rng(seed: number) {
	let a = seed >>> 0;
	return () => {
		a = (a + 0x6d2b79f5) >>> 0;
		let t = a;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

export interface SynthOptions {
	/** Pointer: where the gesture happens. */
	x: number;
	y: number;
	/** Fingers' travel in px, signed like the finger: negative = left = next. */
	travel: number;
	/** Duration of the finger-down phase, ms. */
	ms: number;
	/** Peak of the velocity bell as a fraction of the duration (0.5 = symmetric, <0.5 = fast start). */
	peakAt?: number;
	/** Fraction of the cross-axis travel that leaks into dy (0.15 = a slightly diagonal swipe). */
	dyRatio?: number;
	/** Sensor noise, px per sample. */
	noise?: number;
	/** macOS momentum after the lift: initial velocity as a fraction of the lift velocity (0 = none). */
	momentum?: number;
	/** Seed. */
	seed?: number;
	/** Start time offset, ms. */
	at?: number;
	/** Trackpad precision. */
	precise?: boolean;
}

/**
 * One finger-down → lift → momentum sequence. The finger velocity is a smooth bell (accelerate,
 * peak, decelerate) integrated to `travel`; samples are 8 ms apart with a first `mayBegin`, a `began`,
 * `changed`s, an `ended` (zero delta) and, when the lift speed is high, momentum `began`/`changed`/`ended`
 * events decaying geometrically (~0.92 per sample, what AppKit's inertia looks like).
 */
export function synth(o: SynthOptions): ScrollSample[] {
	const rnd = rng(o.seed ?? 1);
	const at = o.at ?? 0;
	const n = Math.max(2, Math.round(o.ms / SAMPLE_MS));
	const peak = Math.min(0.9, Math.max(0.1, o.peakAt ?? 0.5));
	// Velocity bell: a skewed sine so the peak sits at `peak`.
	const weights = Array.from({ length: n }, (_, i) => {
		const u = (i + 0.5) / n;
		const w = u < peak ? u / peak : (1 - u) / (1 - peak);
		return Math.max(0.02, Math.sin((Math.PI / 2) * w) ** 1.4);
	});
	const sum = weights.reduce((a, b) => a + b, 0);
	const out: ScrollSample[] = [];
	const push = (t: number, phase: ScrollPhase, momentum: MomentumPhase, dx: number, dy: number) =>
		out.push({
			t: Math.round((at + t) * 100) / 100,
			phase,
			momentum,
			dx: Math.round(dx * 100) / 100,
			dy: Math.round(dy * 100) / 100,
			precise: o.precise ?? true,
			x: o.x,
			y: o.y,
		});
	push(0, "mayBegin", "none", 0, 0);
	push(SAMPLE_MS, "began", "none", 0, 0);
	let t = SAMPLE_MS * 2;
	let last = 0;
	for (let i = 0; i < n; i++) {
		const dx = (weights[i]! / sum) * o.travel + (rnd() - 0.5) * (o.noise ?? 0.3);
		const dy = dx * (o.dyRatio ?? 0) + (rnd() - 0.5) * (o.noise ?? 0.3);
		last = dx;
		push(t, "changed", "none", dx, dy);
		t += SAMPLE_MS;
	}
	push(t, "ended", "none", 0, 0);
	t += SAMPLE_MS;
	const m = o.momentum ?? 0;
	if (m > 0 && Math.abs(last) > 0.5) {
		let v = last * m;
		push(t, "none", "began", v, v * (o.dyRatio ?? 0));
		t += SAMPLE_MS;
		while (Math.abs(v) > 0.4) {
			v *= 0.92;
			push(t, "none", "changed", v, v * (o.dyRatio ?? 0));
			t += SAMPLE_MS;
		}
		push(t, "none", "ended", 0, 0);
	}
	return out;
}

/** Concatenate sequences, each starting `gap` ms after the previous one's last event. */
export function chain(parts: { events: ScrollSample[]; gap?: number }[]): ScrollSample[] {
	const out: ScrollSample[] = [];
	let end = 0;
	for (const p of parts) {
		const base = out.length ? end + (p.gap ?? 0) : 0;
		const t0 = p.events[0]?.t ?? 0;
		for (const e of p.events) out.push({ ...e, t: Math.round((base + e.t - t0) * 100) / 100 });
		end = out.at(-1)?.t ?? end;
	}
	return out;
}

/** Net finger travel and the first/last timestamps of a sequence: what the assertions compare against. */
export function summary(events: ScrollSample[]) {
	let dx = 0;
	let dy = 0;
	for (const e of events) {
		if (e.momentum !== "none") continue;
		dx += e.dx;
		dy += e.dy;
	}
	return { dx, dy, ms: (events.at(-1)?.t ?? 0) - (events[0]?.t ?? 0), n: events.length };
}

/** Geometry the synthetic suite assumes (the agent display is 1280×820, sidebar 260 wide, strip 40 tall). */
export interface Layout {
	sidebar: { x: number; y: number };
	content: { x: number; y: number };
	strip: { x: number; y: number };
	/** Over a native terminal/browser view inside the content area. */
	nativeView: { x: number; y: number };
	pageWidth: number;
}

/** Which pointer positions belong to which zone for the standard bench layout. */
export const BENCH_LAYOUT: Layout = {
	sidebar: { x: 130, y: 400 },
	content: { x: 700, y: 500 },
	strip: { x: 700, y: 62 },
	nativeView: { x: 700, y: 400 },
	pageWidth: 420,
};
