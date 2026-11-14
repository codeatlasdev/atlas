/**
 * What the two onboarding illustrations (CrewArtIntro, CrewArtHow) share: eased progress over a time window and a
 * requestAnimationFrame loop that stops while the window is hidden.
 */

export const clamp01 = (x: number) => Math.max(0, Math.min(1, x));

/** Quadratic ease-in-out of a 0..1 progress. */
export const easeInOut = (p: number) => (p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2);

/** Eased 0 → 1 as `x` moves through [a, b]; 0 before a, 1 after b. */
export const segment = (x: number, a: number, b: number) => easeInOut(clamp01((x - a) / (b - a)));

/**
 * Call `tick(now, dt)` every animation frame (`dt` in seconds since the previous frame) until the returned stop
 * function runs. The loop is cancelled while `document.hidden`; on return `dt` restarts from zero (the hidden time
 * is never part of a frame). `elapsed(now)` is the time since the start in ms; with `pauseClock` the hidden time is
 * also left out of it (a looping scene resumes where it was), otherwise it keeps counting (a one-shot intro finishes
 * on schedule).
 */
export function startTimeline(
	tick: (now: number, dt: number) => void,
	opts: { pauseClock?: boolean } = {},
): { stop: () => void; elapsed: (now: number) => number } {
	let origin = performance.now();
	let last = origin;
	let hiddenAt = 0;
	let raf = 0;
	const frame = (now: number) => {
		tick(now, (now - last) / 1000);
		last = now;
		raf = requestAnimationFrame(frame);
	};
	raf = requestAnimationFrame(frame);
	const onVisibility = () => {
		if (document.hidden) {
			hiddenAt = performance.now();
			cancelAnimationFrame(raf);
			return;
		}
		const gone = performance.now() - hiddenAt;
		if (opts.pauseClock) origin += gone;
		last = performance.now();
		raf = requestAnimationFrame(frame);
	};
	document.addEventListener("visibilitychange", onVisibility);
	return {
		stop() {
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", onVisibility);
		},
		elapsed: (now) => now - origin,
	};
}
