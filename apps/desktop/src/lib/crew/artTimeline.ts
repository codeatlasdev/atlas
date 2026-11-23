/**
 * What the two onboarding illustrations (CrewArtIntro, CrewArtHow) share: eased progress over a time window and a
 * requestAnimationFrame loop that stops while the window is hidden or resting.
 */
import { clock } from "#lib/clock.svelte.ts";

export const clamp01 = (x: number) => Math.max(0, Math.min(1, x));

/** Quadratic ease-in-out of a 0..1 progress. */
export const easeInOut = (p: number) => (p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2);

/** Eased 0 → 1 as `x` moves through [a, b]; 0 before a, 1 after b. */
export const segment = (x: number, a: number, b: number) => easeInOut(clamp01((x - a) / (b - a)));

/** The part of the app clock a loop reads: is the window resting (hidden, or blurred past the grace) and tell me when that changes. */
export interface RestSource {
	readonly resting: boolean;
	onRest(fn: (resting: boolean) => void): () => void;
}

/**
 * Call `tick(now, dt)` every animation frame (`dt` in seconds since the previous frame) until the returned stop
 * function runs. The loop is cancelled while `document.hidden` or while the app clock says the window rests (blurred
 * past its grace: WebKitGTK never reports a window behind another as hidden); on return `dt` restarts from zero (the
 * resting time is never part of a frame). `elapsed(now)` is the time since the start in ms; with `pauseClock` the
 * resting time is also left out of it (a looping scene resumes where it was), otherwise it keeps counting (a one-shot
 * intro finishes on schedule).
 */
export function startTimeline(
	tick: (now: number, dt: number) => void,
	opts: { pauseClock?: boolean; rest?: RestSource } = {},
): { stop: () => void; elapsed: (now: number) => number } {
	const rest = opts.rest ?? clock;
	let origin = performance.now();
	let last = origin;
	let haltedAt = 0;
	let raf = 0;
	let halted = false;
	const frame = (now: number) => {
		tick(now, (now - last) / 1000);
		last = now;
		raf = requestAnimationFrame(frame);
	};
	const sync = () => {
		const want = document.hidden || rest.resting;
		if (want === halted) return;
		halted = want;
		if (halted) {
			haltedAt = performance.now();
			cancelAnimationFrame(raf);
			return;
		}
		const gone = performance.now() - haltedAt;
		if (opts.pauseClock) origin += gone;
		last = performance.now();
		raf = requestAnimationFrame(frame);
	};
	halted = document.hidden || rest.resting;
	haltedAt = origin;
	if (!halted) raf = requestAnimationFrame(frame);
	document.addEventListener("visibilitychange", sync);
	const off = rest.onRest(sync);
	return {
		stop() {
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", sync);
			off();
		},
		elapsed: (now) => now - origin,
	};
}
