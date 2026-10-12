/**
 * Two-finger trackpad swipe recognizer (Arc-style), pure and clock-injected so it tests
 * without a DOM. macOS delivers a swipe as `wheel` events with `deltaX`, then a momentum
 * tail; there is no "end" event, so the end is a quiet gap (`endIdle`).
 *
 * One gesture = idle → tracking (axis locked after a few px) → end (commit or cancel) →
 * a quiet gap before the next one can start (the momentum tail after a commit is ignored).
 *
 * The fingers lifting has no event either, and the momentum tail keeps `wheel` events coming
 * for half a second or more, so waiting for silence makes the switch feel late. Instead the
 * release is read from the deltas: once they decay steadily (momentum is a geometric decay)
 * the gesture ends at once, and the rest of the tail is swallowed.
 */

export interface GestureOptions {
	/** Travel that decides the axis, in px. */
	lockPx?: number;
	/** Fraction of `width` that commits on release. */
	commitFrac?: number;
	/** Release speed (px/ms) that commits regardless of distance, once past `flickMinFrac`. */
	flickSpeed?: number;
	flickMinFrac?: number;
	/** No event for this long ends the gesture, in ms. */
	endIdle?: number;
	/** Rubber-band stiffness past the edge of the travel (0 = hard stop, 1 = linear). */
	resist?: number;
	/** Release detection from momentum decay (see the module doc). */
	decaySteps?: number;
	decayFrac?: number;
	/** Whether a direction can be committed: `1` = content moves left (next), `-1` = right (previous). */
	can?: (dir: 1 | -1) => boolean;
}

export type GesturePhase = "idle" | "tracking" | "ignored";

export interface GestureView {
	phase: GesturePhase;
	/** Rubber-banded translation in px, same sign as the finger (positive = fingers moved right). */
	offset: number;
	/** 0..1 of the commit distance reached; drives hint chips and edge glow. */
	progress: number;
	/** Direction being previewed: `1` = next, `-1` = previous, `0` = none yet. */
	dir: 1 | -1 | 0;
}

export type GestureEnd = { kind: "commit"; dir: 1 | -1 } | { kind: "cancel" } | null;

export const IDLE: GestureView = { phase: "idle", offset: 0, progress: 0, dir: 0 };

/** One recognizer per gesture surface. `feed` takes wheel deltas and a clock. */
export interface Gesture {
	readonly view: GestureView;
	readonly active: boolean;
	/** Feed one wheel event; the new view when it changed visibly, else `null`. */
	feed(deltaX: number, deltaY: number, now: number, surfaceWidth: number): GestureView | null;
	/** Poll with the clock; the gesture's end once the fingers were quiet for `endIdle`. */
	poll(now: number): GestureEnd;
	cancel(): void;
}

/**
 * `surfaceWidth` is the travel that equals a full swipe. macOS natural scrolling reports
 * fingers-right as negative `deltaX`; `feed` negates it so `offset` follows the finger, and
 * `dir` is "next" (1) when the fingers moved left.
 */
export function createGesture(opts: GestureOptions = {}): Gesture {
	const lockPx = opts.lockPx ?? 6;
	const commitFrac = opts.commitFrac ?? 0.3;
	const flickSpeed = opts.flickSpeed ?? 0.9;
	const flickMinFrac = opts.flickMinFrac ?? 0.08;
	const endIdle = opts.endIdle ?? 90;
	/** Consecutive shrinking deltas below `decayFrac` of the peak that read as the fingers lifting. */
	const decaySteps = opts.decaySteps ?? 3;
	const decayFrac = opts.decayFrac ?? 0.6;
	const resist = opts.resist ?? 0.35;
	const can = opts.can ?? (() => true);

	let phase: GesturePhase = "idle";
	let width = 1;
	let raw = 0; // finger travel, +right
	let rawY = 0;
	let last = -Infinity;
	let speed = 0; // px/ms, +right, smoothed
	let peak = 0; // largest |deltaX| this gesture
	let prevAbs = 0;
	let decays = 0;
	let released = false;
	let view: GestureView = IDLE;

	function band(x: number): number {
		// Past the allowed direction the content resists: a soft rubber band.
		const dir = x < 0 ? 1 : -1; // fingers left → next
		if (x === 0 || can(dir)) return x;
		return x * resist * Math.max(0, 1 - Math.abs(x) / (width * 2));
	}

	function compute(): GestureView {
		const dir = raw === 0 ? 0 : raw < 0 ? 1 : -1;
		const offset = band(raw);
		const progress = dir === 0 || !can(dir) ? 0 : Math.min(1, Math.abs(raw) / (width * commitFrac));
		return { phase, offset, progress, dir };
	}

	function reset() {
		phase = "idle";
		raw = rawY = speed = 0;
		peak = prevAbs = decays = 0;
		released = false;
		view = IDLE;
	}

	return {
		get view() {
			return view;
		},
		get active() {
			return phase === "tracking";
		},
		/**
		 * Feed one wheel event. Returns the new view when it changed visibly, else `null`.
		 * `deltaX` is the DOM value (natural scrolling: fingers right ⇒ negative).
		 */
		feed(deltaX: number, deltaY: number, now: number, surfaceWidth: number): GestureView | null {
			const gap = now - last;
			if (phase !== "idle" && gap > endIdle) reset(); // a stale gesture the poller never closed
			const fresh = phase === "idle" && gap > endIdle;
			const prevLast = last;
			last = now;
			width = Math.max(1, surfaceWidth);
			if (phase === "ignored") return null;
			if (phase === "idle" && !fresh) return null; // momentum tail right after a commit/cancel
			if (!Number.isFinite(deltaX) || !Number.isFinite(deltaY)) return null;
			if (phase === "idle") {
				phase = "tracking";
				raw = rawY = speed = 0;
				peak = prevAbs = decays = 0;
				released = false;
			}
			if (released) return null; // momentum after the lift: already decided
			const abs = Math.abs(deltaX);
			peak = Math.max(peak, abs);
			decays = abs < prevAbs && abs < peak * decayFrac ? decays + 1 : 0;
			prevAbs = abs;
			if (decays >= decaySteps && peak >= 4) released = true;
			raw -= deltaX;
			rawY += deltaY;
			if (view === IDLE || view.phase === "idle") {
				// Axis lock: until we have moved lockPx, a mostly-vertical drift belongs to scrolling.
				if (Math.abs(raw) < lockPx && Math.abs(rawY) < lockPx) return null;
				if (Math.abs(rawY) > Math.abs(raw)) {
					phase = "ignored";
					return null;
				}
			}
			const dt = Math.max(1, now - (Number.isFinite(prevLast) ? prevLast : now - 8));
			speed = speed * 0.6 + (-deltaX / dt) * 0.4;
			view = compute();
			return view;
		},
		/**
		 * Poll with the clock (a rAF or timer tick). Returns the gesture's end once the
		 * fingers have been quiet for `endIdle`; `null` while it is still going or idle.
		 */
		poll(now: number): GestureEnd {
			if (phase === "idle") return null;
			if (!released && now - last < endIdle) return null;
			const was = phase;
			const dir = raw === 0 ? 0 : raw < 0 ? 1 : -1;
			const frac = Math.abs(raw) / width;
			const flick =
				Math.abs(speed) >= flickSpeed && frac >= flickMinFrac && Math.sign(speed) === Math.sign(raw);
			const commit = was === "tracking" && dir !== 0 && can(dir) && (frac >= commitFrac || flick);
			// Stay in "idle" with `last` kept: events inside the next endIdle are the momentum tail.
			phase = "idle";
			raw = rawY = speed = 0;
			peak = prevAbs = decays = 0;
			released = false;
			view = IDLE;
			if (was === "ignored") return null;
			return commit ? { kind: "commit", dir: dir as 1 | -1 } : { kind: "cancel" };
		},
		cancel: reset,
	};
}
