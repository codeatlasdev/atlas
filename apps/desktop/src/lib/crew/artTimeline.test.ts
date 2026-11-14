// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { clamp01, easeInOut, segment, startTimeline } from "./artTimeline.ts";

describe("easing", () => {
	test("clamp01 pins to the unit range", () => {
		expect([clamp01(-3), clamp01(0.4), clamp01(9)]).toEqual([0, 0.4, 1]);
	});

	test("easeInOut starts slow, is 0.5 in the middle, ends slow and is symmetric", () => {
		expect(easeInOut(0)).toBe(0);
		expect(easeInOut(0.5)).toBe(0.5);
		expect(easeInOut(1)).toBe(1);
		expect(easeInOut(0.25)).toBeLessThan(0.25);
		expect(easeInOut(0.75)).toBeGreaterThan(0.75);
		expect(easeInOut(0.2) + easeInOut(0.8)).toBeCloseTo(1);
	});

	test("segment is 0 before the window, 1 after it and eased inside", () => {
		expect(segment(0.1, 0.2, 0.6)).toBe(0);
		expect(segment(0.9, 0.2, 0.6)).toBe(1);
		expect(segment(0.4, 0.2, 0.6)).toBeCloseTo(0.5);
	});
});

describe("startTimeline", () => {
	let frames: Array<(now: number) => void>;
	let clock: number;
	let hidden: boolean;
	const realRaf = globalThis.requestAnimationFrame;
	const realCancel = globalThis.cancelAnimationFrame;
	const realNow = performance.now.bind(performance);
	const realHidden = Object.getOwnPropertyDescriptor(Document.prototype, "hidden");

	beforeEach(() => {
		frames = [];
		clock = 1000;
		hidden = false;
		globalThis.requestAnimationFrame = (cb) => frames.push(cb);
		globalThis.cancelAnimationFrame = (id) => void (frames[id - 1] = () => {});
		performance.now = () => clock;
		Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
	});

	afterEach(() => {
		globalThis.requestAnimationFrame = realRaf;
		globalThis.cancelAnimationFrame = realCancel;
		performance.now = realNow;
		delete document.hidden;
		if (realHidden) Object.defineProperty(Document.prototype, "hidden", realHidden);
	});

	const step = (ms: number) => {
		clock += ms;
		const cb = frames.pop();
		cb?.(clock);
	};
	const setHidden = (v: boolean) => {
		hidden = v;
		document.dispatchEvent(new Event("visibilitychange"));
	};

	test("ticks every frame with dt in seconds", () => {
		const dts: number[] = [];
		const t = startTimeline((_now, dt) => dts.push(dt));
		step(16);
		step(500);
		expect(dts).toEqual([0.016, 0.5]);
		t.stop();
	});

	test("hiding stops the loop and the hidden time never shows up in dt", () => {
		const dts: number[] = [];
		const t = startTimeline((_now, dt) => dts.push(dt));
		step(1000);
		setHidden(true);
		clock += 60_000;
		setHidden(false);
		step(16);
		expect(dts).toEqual([1, 0.016]);
		t.stop();
	});

	test("by default elapsed keeps counting through the hidden time (a one-shot intro stays on schedule)", () => {
		const t = startTimeline(() => {});
		setHidden(true);
		clock += 5000;
		setHidden(false);
		expect(t.elapsed(clock)).toBe(5000);
		t.stop();
	});

	test("pauseClock leaves the hidden time out of elapsed (a looping scene resumes where it was)", () => {
		const t = startTimeline(() => {}, { pauseClock: true });
		step(2000);
		setHidden(true);
		clock += 5000;
		setHidden(false);
		expect(t.elapsed(clock)).toBe(2000);
		t.stop();
	});

	test("stop ends the loop and removes the visibility listener", () => {
		let ticks = 0;
		const t = startTimeline(() => ticks++);
		step(16);
		t.stop();
		step(16);
		setHidden(true);
		setHidden(false);
		step(16);
		expect(ticks).toBe(1);
	});
});
