// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";

// #289 (docs/perf/linux-webkit-study.md): every crewmate face ran a requestAnimationFrame loop for as long as the app was
// open, and it paused only on `document.hidden`, which WebKitGTK on Wayland never reports for a window behind another one.
// A crew list of N faces woke the page 60 x N times a second for nothing. The face now ends its loop when the app clock says
// the window rests and starts it again when it is back. The clock itself is faked here (its 60 s grace is tested in
// clock.test.ts); the face, its springs and its loop are real.
let resting = false;
const listeners = new Set<(r: boolean) => void>();
mock.module("#lib/clock.svelte.ts", () => ({
	clock: {
		get resting() {
			return resting;
		},
		onRest: (f) => (listeners.add(f), () => listeners.delete(f)),
		every: () => () => {},
	},
}));
const { flushSync, mount, unmount } = await import("svelte");
const { default: CrewmateFace } = await import("./CrewmateFace.svelte");

let queue: Map<number, (t: number) => void>;
let n: number;
let now: number;
const real = {
	raf: globalThis.requestAnimationFrame,
	cancel: globalThis.cancelAnimationFrame,
	now: performance.now.bind(performance),
};
let done: (() => void) | undefined;

beforeEach(() => {
	queue = new Map();
	n = 0;
	now = 1000;
	resting = false;
	listeners.clear();
	globalThis.requestAnimationFrame = (cb) => (queue.set(++n, cb), n);
	globalThis.cancelAnimationFrame = (id) => void queue.delete(id);
	performance.now = () => now;
});
afterEach(() => {
	done?.();
	done = undefined;
	globalThis.requestAnimationFrame = real.raf;
	globalThis.cancelAnimationFrame = real.cancel;
	performance.now = real.now;
});

/** Run the pending frames once; returns how many ran. */
function frame(ms = 16) {
	now += ms;
	const cbs = [...queue.values()];
	queue.clear();
	cbs.forEach((cb) => cb(now));
	return cbs.length;
}
function face(state = "idle") {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(CrewmateFace, { target, props: { seed: "a", state, size: 40 } }));
	done = () => (unmount(app), target.remove());
}
const setResting = (r: boolean) => ((resting = r), [...listeners].forEach((f) => f(r)));

test("a face animates while the window is awake, ends its loop when it rests, and resumes when it wakes", () => {
	face();
	expect(frame()).toBe(1);
	expect(frame()).toBe(1);
	setResting(true);
	expect(frame()).toBe(1); // the frame already queued runs once, sees the rest and does not ask for another
	expect(frame()).toBe(0);
	expect(frame()).toBe(0);
	setResting(false);
	expect(frame()).toBe(1);
	expect(frame()).toBe(1);
});

test("a face that mounts into a resting window never starts a loop", () => {
	resting = true;
	face();
	expect(queue.size).toBe(0);
	setResting(false);
	expect(queue.size).toBe(1);
});

test("unmounting ends the loop and the clock subscription", () => {
	face();
	done();
	done = undefined;
	expect(queue.size).toBe(0);
	expect(listeners.size).toBe(0);
});
