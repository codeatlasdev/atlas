/**
 * Collects items and hands them over once per animation frame, so a source that fires faster than the screen (omp's
 * streamed deltas, hostd's batches) costs one render per frame at most, or per `minIntervalMs`. `maxWaitMs` is a safety net: a webview that stops
 * running `requestAnimationFrame` (a hidden macOS window) must not hold the items back or let them grow unbounded.
 */
export interface FrameEnv {
	now(): number;
	raf(cb: () => void): unknown;
	cancelRaf(h: unknown): void;
	setTimeout(cb: () => void, ms: number): unknown;
	clearTimeout(h: unknown): void;
}

const realEnv: FrameEnv = {
	now: () => performance.now(),
	raf: (cb) => requestAnimationFrame(cb),
	cancelRaf: (h) => cancelAnimationFrame(h as number),
	setTimeout: (cb, ms) => window.setTimeout(cb, ms),
	clearTimeout: (h) => window.clearTimeout(h as number),
};

export function frameBatch<T>(
	flush: (items: T[]) => void,
	{ minIntervalMs = 0, maxWaitMs = 100 }: { minIntervalMs?: number; maxWaitMs?: number } = {},
	env: FrameEnv = realEnv,
) {
	let lastRun = -Infinity;
	let delay: unknown;
	let pending: T[] = [];
	let raf: unknown;
	let timer: unknown;
	let scheduled = false;

	function run() {
		if (scheduled) {
			env.cancelRaf(raf);
			env.clearTimeout(timer);
			env.clearTimeout(delay);
			scheduled = false;
		}
		if (!pending.length) return;
		lastRun = env.now();
		const items = pending;
		pending = [];
		flush(items);
	}

	return {
		push(items: readonly T[]) {
			if (!items.length) return;
			pending.push(...items);
			if (scheduled) return;
			scheduled = true;
			// At most one flush per `minIntervalMs` (a stream faster than the eye needs, cheaper than every frame), then the
			// next frame; `maxWaitMs` still bounds the wait when the webview never runs a frame.
			const wait = Math.max(0, lastRun + minIntervalMs - env.now());
			if (wait > 0) delay = env.setTimeout(() => (raf = env.raf(run)), wait);
			else raf = env.raf(run);
			timer = env.setTimeout(run, wait + maxWaitMs);
		},
		/** Hand over what is pending now (before a reset that must not see it later). */
		flushNow: run,
		/** Forget what is pending (the view is gone). */
		stop() {
			if (scheduled) {
				env.cancelRaf(raf);
				env.clearTimeout(timer);
				env.clearTimeout(delay);
				scheduled = false;
			}
			pending = [];
		},
	};
}
