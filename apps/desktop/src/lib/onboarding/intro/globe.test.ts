// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { SHEETS, calm, litSheets, progressFrame } from "./globe.ts";
import { BEATS, TILES } from "./timeline.ts";

describe("litSheets: each project found lights one sheet of the globe", () => {
	test("n projects light n distinct sheets, in a fixed order, capped at the 12 sheets", () => {
		expect(litSheets(0)).toEqual([]);
		expect(litSheets(3).length).toBe(3);
		expect(new Set(litSheets(7)).size).toBe(7);
		expect(litSheets(40).length).toBe(SHEETS.length);
		expect(SHEETS.length).toBe(12);
	});
	test("monotone: the first n sheets of n+1 are the same (nothing re-lights or jumps as the scan streams)", () => {
		for (let n = 0; n < 12; n++) expect(litSheets(n + 1).slice(0, n)).toEqual(litSheets(n));
	});
	test("the order spreads: the first four sheets are not all neighbours (the globe fills like a scan, not a wipe)", () => {
		const first = litSheets(4).map((id) => TILES.find((t) => t.id === id));
		const xs = first.map((t) => t.cx);
		expect(Math.max(...xs) - Math.min(...xs)).toBeGreaterThan(150);
	});
	test("negative and fractional counts are safe", () => {
		expect(litSheets(-2)).toEqual([]);
		expect(litSheets(2.9).length).toBe(2);
	});
});

describe("progressFrame: the globe while the user's projects arrive", () => {
	test("sits fully assembled and dim with nothing lit (the calm empty state is this frame)", () => {
		const f = progressFrame({ lit: 0, doneAt: null, now: 10, litAt: {} });
		expect(f.tiles.every((t) => t.alpha > 0)).toBe(true);
		expect(Math.max(...f.tiles.map((t) => t.glow))).toBeLessThan(0.2);
	});
	test("lit sheets glow brighter than unlit ones, and a freshly lit one flares then settles", () => {
		const id = litSheets(1)[0];
		const fresh = progressFrame({ lit: 1, doneAt: null, now: 10.05, litAt: { [id]: 10 } });
		const later = progressFrame({ lit: 1, doneAt: null, now: 12, litAt: { [id]: 10 } });
		const g = (f) => f.tiles.find((t) => t.id === id).glow;
		expect(g(fresh)).toBeGreaterThan(g(later));
		expect(g(later)).toBeGreaterThan(0.3);
		const other = later.tiles.find((t) => t.id !== id && TILES.find((x) => x.id === t.id).kind === "sheet");
		expect(g(later)).toBeGreaterThan(other.glow);
	});
	test("completion: every sheet lights and the the completion hit fires (flash, ring, beams)", () => {
		const f = progressFrame({ lit: 12, doneAt: 10, now: 10 + 0.02, litAt: {} });
		expect(f.flash).toBeGreaterThan(0.5);
		expect(f.ring).toBeGreaterThan(0);
		expect(
			f.tiles.filter((t) => TILES.find((x) => x.id === t.id).kind === "sheet").every((t) => t.glow > 0.5),
		).toBe(true);
	});
	test("the hit decays: flash gone after a second", () => {
		const a = progressFrame({ lit: 12, doneAt: 10, now: 10 + 0.5, litAt: {} });
		expect(a.flash).toBeLessThan(0.1);
		const b = progressFrame({ lit: 12, doneAt: 10, now: 10 + 1.5, litAt: {} });
		expect(b.flash).toBe(0);
	});
	test("before completion there is no flash", () => {
		expect(progressFrame({ lit: 5, doneAt: null, now: 3, litAt: {} }).flash).toBe(0);
	});
	test("the frame's time field puts the renderer in the 'whole globe' branch (nodes visible)", () => {
		expect(progressFrame({ lit: 0, doneAt: null, now: 1, litAt: {} }).t).toBeGreaterThanOrEqual(BEATS.hit);
	});
});

describe("calm: the restrained empty-state variant", () => {
	test("is the whole globe at low intensity with one tile glinting at a time, deterministically", () => {
		const a = calm(0);
		const b = calm(4);
		expect(calm(0)).toEqual(a);
		const glints = (f) => f.tiles.filter((t) => t.glow > 0.12).length;
		expect(glints(a)).toBeLessThanOrEqual(2);
		expect(glints(a) + glints(b)).toBeGreaterThan(0);
		expect(a.flash).toBe(0);
		expect(a.ring).toBe(0);
	});
	test("the glint wanders: different times light different tiles", () => {
		const lit = (t) =>
			calm(t)
				.tiles.filter((x) => x.glow > 0.12)
				.map((x) => x.id)
				.join();
		expect(new Set([0, 4, 8, 12, 16].map(lit)).size).toBeGreaterThan(2);
	});
	test("it is a still stage with a slow breath", () => {
		const f = calm(3);
		expect(f.markScale).toBeGreaterThan(0.99);
		expect(f.markScale).toBeLessThan(1.02);
	});
});

describe("contrast: lit sheets must read against unlit ones", () => {
	test("the gap in glow between a lit sheet (after its flare) and an unlit one is at least 0.35", () => {
		const id = litSheets(1)[0];
		const f = progressFrame({ lit: 1, doneAt: null, now: 20, litAt: { [id]: 0 } });
		const lit = f.tiles.find((t) => t.id === id).glow;
		const unlit = Math.max(...f.tiles.filter((t) => t.id !== id).map((t) => t.glow));
		expect(lit - unlit).toBeGreaterThanOrEqual(0.35);
	});
});
