// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { Heights, windowFor } from "./virtual.ts";

test("unmeasured items use the estimate", () => {
	const h = new Heights(20);
	h.resize(100);
	expect(h.total).toBe(2000);
	expect(h.top(0)).toBe(0);
	expect(h.top(10)).toBe(200);
	expect(h.at(0)).toBe(0);
	expect(h.at(39)).toBe(1);
	expect(h.at(40)).toBe(2);
});

test("a measured height shifts everything below it", () => {
	const h = new Heights(20);
	h.resize(10);
	h.set(2, 100);
	expect(h.total).toBe(9 * 20 + 100);
	expect(h.top(2)).toBe(40);
	expect(h.top(3)).toBe(140);
	expect(h.at(139)).toBe(2);
	expect(h.at(140)).toBe(3);
});

test("set reports whether the layout changed, so a stable measure schedules no work", () => {
	const h = new Heights(20);
	h.resize(5);
	expect(h.set(1, 20)).toBe(false);
	expect(h.set(1, 33)).toBe(true);
	expect(h.set(1, 33)).toBe(false);
	expect(h.set(1, 33.2)).toBe(false); // sub-pixel noise from a ResizeObserver is not a change
});

test("keyed measures survive items being prepended or dropped", () => {
	const h = new Heights(20);
	h.resize(3, ["a", "b", "c"]);
	h.set(1, 60);
	h.resize(4, ["z", "a", "b", "c"]);
	expect(h.top(2)).toBe(20 + 20); // z, a
	expect(h.top(3)).toBe(20 + 20 + 60); // z, a, b(60)
	h.resize(2, ["b", "c"]);
	expect(h.total).toBe(60 + 20);
});

test("windowFor returns the visible slice plus overscan, clamped", () => {
	const h = new Heights(20);
	h.resize(1000);
	const w = windowFor(h, 2000, 200, 3);
	expect(w.first).toBe(100 - 3);
	expect(w.last).toBe(110 + 1 + 3);
	expect(windowFor(h, 0, 200, 3).first).toBe(0);
	expect(windowFor(h, 1e9, 200, 3).last).toBe(1000);
	expect(windowFor(new Heights(20), 0, 200, 3)).toEqual({ first: 0, last: 0 });
});

test("a 20k-item list renders a bounded window whatever its size", () => {
	const h = new Heights(18);
	h.resize(20_000);
	const w = windowFor(h, 9_000 * 18, 600, 10);
	expect(w.last - w.first).toBeLessThan(60);
});

test("bottom offset: total minus viewport is where the end sits", () => {
	const h = new Heights(20);
	h.resize(50);
	expect(h.total - 400).toBe(600);
	const w = windowFor(h, h.total - 400, 400, 2);
	expect(w.last).toBe(50);
});

test("resizing with the same keys array changes nothing (a scroll re-derives the window, it must not rebuild the sums)", () => {
	const h = new Heights(20);
	const keys = Array.from({ length: 1000 }, (_, i) => `k${i}`);
	h.resize(1000, keys);
	h.set(5, 100);
	const total = h.total;
	h.resize(1000, keys);
	expect(h.total).toBe(total);
	expect(h.top(6)).toBe(5 * 20 + 100);
});
