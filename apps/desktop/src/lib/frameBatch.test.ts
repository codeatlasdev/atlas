// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { frameBatch } from "./frameBatch.ts";

// A source that fires faster than the screen (omp's deltas) must cost one render per frame, and a webview that never runs
// requestAnimationFrame (a hidden macOS window) must still deliver within `maxWaitMs` instead of queueing forever.
function fakeEnv() {
	const cbs = new Map<number, () => void>();
	let n = 0;
	const add = (cb) => (cbs.set(++n, cb), n);
	return {
		env: {
			raf: add,
			cancelRaf: (h) => cbs.delete(h),
			setTimeout: (cb) => add(cb),
			clearTimeout: (h) => cbs.delete(h),
		},
		pending: () => cbs.size,
		/** The first scheduled callback to fire: the frame (1st) or the timer (2nd). */
		fire: (which: 1 | 2) => cbs.get(which === 1 ? Math.min(...cbs.keys()) : Math.max(...cbs.keys()))?.(),
	};
}

test("everything pushed before the frame is delivered once, in order", () => {
	const f = fakeEnv();
	const got: string[][] = [];
	const b = frameBatch<string>((x) => got.push(x), 100, f.env);
	b.push(["a"]);
	b.push(["b", "c"]);
	b.push([]);
	expect(got).toEqual([]);
	f.fire(1);
	expect(got).toEqual([["a", "b", "c"]]);
	expect(f.pending()).toBe(0);
	b.push(["d"]);
	f.fire(1);
	expect(got).toEqual([["a", "b", "c"], ["d"]]);
});

test("without a frame the timer delivers, and the late frame does not deliver twice", () => {
	const f = fakeEnv();
	const got: string[][] = [];
	const b = frameBatch<string>((x) => got.push(x), 100, f.env);
	b.push(["a"]);
	f.fire(2);
	expect(got).toEqual([["a"]]);
	expect(f.pending()).toBe(0);
});

test("flushNow hands over what waits; stop forgets it", () => {
	const f = fakeEnv();
	const got: string[][] = [];
	const b = frameBatch<string>((x) => got.push(x), 100, f.env);
	b.push(["a"]);
	b.flushNow();
	expect(got).toEqual([["a"]]);
	b.push(["b"]);
	b.stop();
	expect(f.pending()).toBe(0);
	b.flushNow();
	expect(got).toEqual([["a"]]);
});
