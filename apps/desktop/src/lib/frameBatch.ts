/**
 * Collects items and hands them over once per animation frame, so a source that fires faster than the screen (omp's
 * streamed deltas, hostd's batches) costs one render per frame. `maxWaitMs` is a safety net: a webview that stops
 * running `requestAnimationFrame` (a hidden macOS window) must not hold the items back or let them grow unbounded.
 */
export interface FrameEnv {
	raf(cb: () => void): unknown;
	cancelRaf(h: unknown): void;
	setTimeout(cb: () => void, ms: number): unknown;
	clearTimeout(h: unknown): void;
}

const realEnv: FrameEnv = {
	raf: (cb) => requestAnimationFrame(cb),
	cancelRaf: (h) => cancelAnimationFrame(h as number),
	setTimeout: (cb, ms) => window.setTimeout(cb, ms),
	clearTimeout: (h) => window.clearTimeout(h as number),
};

export function frameBatch<T>(flush: (items: T[]) => void, maxWaitMs = 100, env: FrameEnv = realEnv) {
	let pending: T[] = [];
	let raf: unknown;
	let timer: unknown;
	let scheduled = false;

	function run() {
		if (scheduled) {
			env.cancelRaf(raf);
			env.clearTimeout(timer);
			scheduled = false;
		}
		if (!pending.length) return;
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
			raf = env.raf(run);
			timer = env.setTimeout(run, maxWaitMs);
		},
		/** Hand over what is pending now (before a reset that must not see it later). */
		flushNow: run,
		/** Forget what is pending (the view is gone). */
		stop() {
			if (scheduled) {
				env.cancelRaf(raf);
				env.clearTimeout(timer);
				scheduled = false;
			}
			pending = [];
		},
	};
}
