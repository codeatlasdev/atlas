// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	MAX_OPACITY,
	MAX_ZOOM,
	MIN_OPACITY,
	MIN_ZOOM,
	clampOpacity,
	clampZoom,
	fitZoom,
	initialState,
	keyEvent,
	percent,
	reduce,
	sizeAt,
	stepOpacity,
	stepZoom,
	wheelOpacity,
	wheelZoom,
	zoomOf,
} from "./pin.ts";

const IMG = { w: 400, h: 300 };
const run = (s, ...evs) => {
	const effects = [];
	for (const ev of evs) {
		const r = reduce(s, ev);
		s = r.state;
		effects.push(...r.effects);
	}
	return { state: s, effects };
};

describe("clamping", () => {
	test("zoom stays between 25% and 400%; garbage is 100%", () => {
		expect(clampZoom(0.01)).toBe(MIN_ZOOM);
		expect(clampZoom(99)).toBe(MAX_ZOOM);
		expect(clampZoom(1.5)).toBe(1.5);
		for (const bad of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY])
			expect(clampZoom(bad)).toBe(1);
	});

	test("opacity stays between 20% and 100%, in whole percents", () => {
		expect(clampOpacity(0)).toBe(MIN_OPACITY);
		expect(clampOpacity(7)).toBe(MAX_OPACITY);
		expect(clampOpacity(0.5)).toBe(0.5);
		expect(clampOpacity(0.123456)).toBe(0.2);
		expect(clampOpacity(0.456)).toBe(0.46);
		expect(clampOpacity(Number.NaN)).toBe(MAX_OPACITY);
	});
});

describe("stepping", () => {
	test("each +/- is a 25% multiplicative step and stops at the limits", () => {
		expect(stepZoom(1, 1)).toBe(1.25);
		expect(stepZoom(1, -1)).toBe(0.8);
		let z = 1;
		for (let i = 0; i < 40; i++) z = stepZoom(z, 1);
		expect(z).toBe(MAX_ZOOM);
		for (let i = 0; i < 40; i++) z = stepZoom(z, -1);
		expect(z).toBe(MIN_ZOOM);
	});

	test("zooming in then out returns to where it started (no drift while inside the limits)", () => {
		expect(stepZoom(stepZoom(1, 1), -1)).toBeCloseTo(1, 10);
	});

	test("[ and ] move the opacity by 10 points without float drift, and stop at 20% / 100%", () => {
		let o = 1;
		const seen = [];
		for (let i = 0; i < 12; i++) {
			o = stepOpacity(o, -1);
			seen.push(o);
		}
		expect(seen.slice(0, 3)).toEqual([0.9, 0.8, 0.7]);
		expect(o).toBe(MIN_OPACITY);
		for (let i = 0; i < 12; i++) o = stepOpacity(o, 1);
		expect(o).toBe(MAX_OPACITY);
		// ten steps from 20% are exactly 100% - 0.2 + ... : no 0.30000000000000004
		let p = 0.2;
		for (let i = 0; i < 3; i++) p = stepOpacity(p, 1);
		expect(p).toBe(0.5);
	});
});

describe("wheel", () => {
	test("scrolling up zooms in, down zooms out; a pinch is more sensitive than a wheel", () => {
		expect(wheelZoom(1, -50, false)).toBeGreaterThan(1);
		expect(wheelZoom(1, 50, false)).toBeLessThan(1);
		expect(wheelZoom(1, -5, true)).toBeGreaterThan(wheelZoom(1, -5, false));
	});

	test("one event never moves more than the cap, however wild the delta", () => {
		expect(wheelZoom(1, -100_000, false)).toBe(wheelZoom(1, -100, false));
		expect(wheelZoom(1, Number.NaN, false)).toBe(1);
	});

	test("the zoom never leaves 25%..400% by wheel", () => {
		let z = 1;
		for (let i = 0; i < 500; i++) z = wheelZoom(z, -100, true);
		expect(z).toBe(MAX_ZOOM);
		for (let i = 0; i < 500; i++) z = wheelZoom(z, 100, true);
		expect(z).toBe(MIN_ZOOM);
	});

	test("Alt + wheel fades: up is more opaque, down more transparent, inside 20%..100%", () => {
		expect(wheelOpacity(0.5, -50)).toBeGreaterThan(0.5);
		expect(wheelOpacity(0.5, 50)).toBeLessThan(0.5);
		expect(wheelOpacity(0.21, 100)).toBe(MIN_OPACITY);
		expect(wheelOpacity(0.99, -100)).toBe(MAX_OPACITY);
	});
});

describe("fit to the display", () => {
	const D = { w: 1512, h: 982 };

	test("an image that fits opens at 100%, never scaled up", () => {
		expect(fitZoom(IMG, D)).toBe(1);
		expect(fitZoom({ w: 10, h: 10 }, D)).toBe(1);
	});

	test("an image bigger than 80% of the display is scaled down to exactly 80%, keeping its aspect", () => {
		// a full-display screenshot
		expect(sizeAt({ w: 1512, h: 982 }, fitZoom({ w: 1512, h: 982 }, D))).toEqual({ w: 1210, h: 786 });
		// wide image: limited by width
		expect(sizeAt({ w: 3000, h: 300 }, fitZoom({ w: 3000, h: 300 }, D))).toEqual({ w: 1210, h: 121 });
		// tall image: limited by height
		const k = fitZoom({ w: 300, h: 3000 }, D);
		expect(k).toBeCloseTo((982 * 0.8) / 3000, 10);
	});

	test("these are the numbers Rust's surface::pin_fit uses (same inputs, same sizes)", () => {
		// crates/atlas-desktop/src/shots/surface.rs tests: (1512,982)->(1210,786), (3000,300)->(1210,121)
		expect(sizeAt({ w: 1512, h: 982 }, fitZoom({ w: 1512, h: 982 }, D))).toEqual({ w: 1210, h: 786 });
		expect(sizeAt({ w: 3000, h: 300 }, fitZoom({ w: 3000, h: 300 }, D))).toEqual({ w: 1210, h: 121 });
	});

	test("a degenerate image never divides by zero", () => {
		expect(Number.isFinite(fitZoom({ w: 0, h: 0 }, D))).toBe(true);
		expect(sizeAt({ w: 0, h: 0 }, 1)).toEqual({ w: 1, h: 1 });
	});

	test("the window size at a zoom is whole points", () => {
		expect(sizeAt(IMG, 1.25)).toEqual({ w: 500, h: 375 });
		expect(sizeAt({ w: 333, h: 111 }, 0.25)).toEqual({ w: 83, h: 28 });
	});

	test("the zoom a window shows is read from the size Rust really applied", () => {
		expect(zoomOf(IMG, { w: 200, h: 150 })).toBe(0.5);
		// Rust clamped the request to the display: the label tells the truth, inside the limits
		expect(zoomOf(IMG, { w: 99_999, h: 99_999 })).toBe(MAX_ZOOM);
		expect(zoomOf({ w: 0, h: 0 }, { w: 10, h: 10 })).toBe(1);
	});

	test("percent", () => {
		expect([percent(1), percent(1.25), percent(0.25), percent(4)]).toEqual(["100%", "125%", "25%", "400%"]);
	});
});

describe("reducer", () => {
	test("zoom in asks Rust for the new window size", () => {
		const r = run(initialState(IMG), { type: "zoomIn" });
		expect(r.state.zoom).toBe(1.25);
		expect(r.effects).toEqual([{ type: "resize", size: { w: 500, h: 375 } }]);
	});

	test("at a zoom limit nothing changes and Rust is not asked", () => {
		const max = initialState(IMG, MAX_ZOOM);
		expect(run(max, { type: "zoomIn" }).effects).toEqual([]);
		const min = initialState(IMG, MIN_ZOOM);
		expect(run(min, { type: "zoomOut" }).effects).toEqual([]);
		expect(run(min, { type: "wheel", deltaY: 100, alt: false, pinch: false }).effects).toEqual([]);
	});

	test("double click resets to 100% from any zoom, and is a no-op when already there", () => {
		const r = run(initialState(IMG, 0.4), { type: "reset" });
		expect(r.state.zoom).toBe(1);
		expect(r.effects).toEqual([{ type: "resize", size: { w: 400, h: 300 } }]);
		expect(run(initialState(IMG), { type: "reset" }).effects).toEqual([]);
	});

	test("the wheel zooms; Alt + wheel fades and never resizes", () => {
		const z = run(initialState(IMG), { type: "wheel", deltaY: -100, alt: false, pinch: false });
		expect(z.state.zoom).toBeGreaterThan(1);
		expect(z.effects).toHaveLength(1);
		const o = run(initialState(IMG), { type: "wheel", deltaY: 100, alt: true, pinch: false });
		expect(o.state.zoom).toBe(1);
		expect(o.state.opacity).toBeLessThan(1);
		expect(o.effects).toEqual([]);
	});

	test("a pinch zooms the same way as the wheel", () => {
		const r = run(initialState(IMG), { type: "wheel", deltaY: -10, alt: false, pinch: true });
		expect(r.state.zoom).toBeGreaterThan(1);
	});

	test("opacity keys change only the opacity", () => {
		const r = run(initialState(IMG), { type: "opacityDown" }, { type: "opacityDown" });
		expect(r.state.opacity).toBe(0.8);
		expect(r.state.zoom).toBe(1);
		expect(r.effects).toEqual([]);
		expect(run(r.state, { type: "opacityUp" }).state.opacity).toBe(0.9);
	});

	test("the label follows the size Rust applied, not the size asked for", () => {
		const r = run(initialState(IMG), { type: "zoomIn" }, { type: "applied", size: { w: 450, h: 337 } });
		expect(r.state.zoom).toBe(1.125);
	});

	test("Esc / close asks to close the window and changes nothing else", () => {
		const s = initialState(IMG, 2);
		const r = reduce(s, { type: "close" });
		expect(r.effects).toEqual([{ type: "close" }]);
		expect(r.state).toEqual(s);
	});

	test("a pin that opened scaled down (fit) goes to 100% on double click, even though that is bigger than the window", () => {
		const D = { w: 1512, h: 982 };
		const big = { w: 3000, h: 2000 };
		const s = initialState(big, fitZoom(big, D));
		expect(s.zoom).toBeLessThan(1);
		const r = reduce(s, { type: "reset" });
		expect(r.state.zoom).toBe(1);
		expect(r.effects).toEqual([{ type: "resize", size: big }]);
	});
});

describe("keys", () => {
	test("+, = and - , _ zoom; [ and ] fade; Escape closes", () => {
		expect(keyEvent({ key: "+" })).toEqual({ type: "zoomIn" });
		expect(keyEvent({ key: "=" })).toEqual({ type: "zoomIn" });
		expect(keyEvent({ key: "-" })).toEqual({ type: "zoomOut" });
		expect(keyEvent({ key: "_" })).toEqual({ type: "zoomOut" });
		expect(keyEvent({ key: "[" })).toEqual({ type: "opacityDown" });
		expect(keyEvent({ key: "]" })).toEqual({ type: "opacityUp" });
		expect(keyEvent({ key: "Escape" })).toEqual({ type: "close" });
	});

	test("other keys, and shortcuts with ⌘ or ⌃, are not ours", () => {
		for (const key of ["a", "Enter", " ", "0", "ArrowUp"]) expect(keyEvent({ key })).toBe(null);
		expect(keyEvent({ key: "+", metaKey: true })).toBe(null);
		expect(keyEvent({ key: "-", ctrlKey: true })).toBe(null);
		expect(keyEvent({ key: "Escape", metaKey: true })).toBe(null);
	});
});
