// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { DEFAULT_CANVAS, dprQuery, planFrame, SizeWatch } from "./sizing.ts";

// Incident (user, live): "the intro/background canvas renders small in a corner at the top". A canvas that is never given
// a buffer keeps the HTML default 300x150; painting a full-window scene into it shows a thumbnail. The contract: the buffer
// is ALWAYS (CSS box x DPR) once the box has a size, and nothing is painted before that.
describe("planFrame: one decision per frame, from one measurement", () => {
	const buf = (w, h) => ({ width: w, height: h });

	test("backing size is the CSS box times the device pixel ratio, and paint size is the CSS box", () => {
		const p = planFrame({ cssW: 1280, cssH: 820, dpr: 2 }, buf(2560, 1640));
		expect(p).toMatchObject({ paint: true, resize: false, scale: 2, w: 1280, h: 820 });
	});

	test("never paints into the HTML default 300x150 buffer when the box is larger: it resizes first", () => {
		const p = planFrame({ cssW: 1280, cssH: 820, dpr: 2 }, buf(DEFAULT_CANVAS.width, DEFAULT_CANVAS.height));
		expect(p.resize).toEqual({ width: 2560, height: 1640 });
		expect(p.paint).toBe(true);
	});

	test("a 0x0 box at mount paints nothing and asks for a retry (window not shown or not sized yet)", () => {
		const p = planFrame({ cssW: 0, cssH: 0, dpr: 2 }, buf(300, 150));
		expect(p.paint).toBe(false);
		expect(p.retry).toBe(true);
		expect(p.resize).toBe(false);
	});

	test("a collapsed axis counts as no box (a 1280x0 strip is not a canvas)", () => {
		expect(planFrame({ cssW: 1280, cssH: 0, dpr: 1 }, buf(300, 150)).paint).toBe(false);
		expect(planFrame({ cssW: 0, cssH: 820, dpr: 1 }, buf(300, 150)).paint).toBe(false);
	});

	test("DPR changes between screens (Retina to external): the same box gets a new buffer", () => {
		const retina = planFrame({ cssW: 1000, cssH: 600, dpr: 2 }, buf(2000, 1200));
		expect(retina.resize).toBe(false);
		const ext = planFrame({ cssW: 1000, cssH: 600, dpr: 1 }, buf(2000, 1200));
		expect(ext.resize).toEqual({ width: 1000, height: 600 });
		expect(ext.scale).toBe(1);
	});

	test("a window resize re-plans the buffer from the new box", () => {
		const p = planFrame({ cssW: 900, cssH: 560, dpr: 2 }, buf(2560, 1640));
		expect(p.resize).toEqual({ width: 1800, height: 1120 });
	});

	test("the pixel ratio is capped at 2 and a bad ratio falls back to 1", () => {
		expect(planFrame({ cssW: 100, cssH: 100, dpr: 3 }, buf(1, 1)).scale).toBe(2);
		expect(planFrame({ cssW: 100, cssH: 100, dpr: 0 }, buf(1, 1)).scale).toBe(1);
		expect(planFrame({ cssW: 100, cssH: 100, dpr: Number.NaN }, buf(1, 1)).scale).toBe(1);
	});

	test("fractional boxes round to whole device pixels and never produce a zero-pixel buffer", () => {
		const p = planFrame({ cssW: 0.4, cssH: 0.4, dpr: 1 }, buf(300, 150));
		expect(p.paint).toBe(false);
		const q = planFrame({ cssW: 333.4, cssH: 100.6, dpr: 1 }, buf(300, 150));
		expect(q.resize).toEqual({ width: 333, height: 101 });
	});

	test("an absurd box is clamped (a runaway layout must not allocate a 100000 px buffer)", () => {
		const p = planFrame({ cssW: 90000, cssH: 90000, dpr: 2 }, buf(300, 150));
		expect(p.resize.width).toBeLessThanOrEqual(8192);
		expect(p.resize.height).toBeLessThanOrEqual(8192);
	});
});

describe("SizeWatch: re-measure until the first real size, then on every change", () => {
	test("a canvas that is 0x0 at mount keeps asking to be re-measured each frame until it has a size", () => {
		const w = new SizeWatch();
		expect(w.shouldMeasure({ w: 0, h: 0 })).toBe(true);
		expect(w.shouldMeasure({ w: 0, h: 0 })).toBe(true);
		expect(w.shouldMeasure({ w: 1280, h: 820 })).toBe(true);
		expect(w.sized).toBe(true);
	});

	test("the first non-zero size is reported once, so the owner can repaint and drop its fallback polling", () => {
		const w = new SizeWatch();
		w.shouldMeasure({ w: 0, h: 0 });
		expect(w.firstSize({ w: 1280, h: 820 })).toBe(true);
		expect(w.firstSize({ w: 1280, h: 820 })).toBe(false);
	});

	test("a later change in box or ratio is a change; an identical measurement is not", () => {
		const w = new SizeWatch();
		w.firstSize({ w: 1280, h: 820 });
		expect(w.changed({ w: 1280, h: 820 }, 2)).toBe(true); // first call records the baseline
		expect(w.changed({ w: 1280, h: 820 }, 2)).toBe(false);
		expect(w.changed({ w: 1000, h: 820 }, 2)).toBe(true);
		expect(w.changed({ w: 1000, h: 820 }, 1)).toBe(true);
		expect(w.changed({ w: 1000, h: 820 }, 1)).toBe(false);
	});
});

describe("dprQuery: the media query that fires when the pixel ratio changes", () => {
	// A window dragged from a Retina screen to an external one keeps the same CSS box, so a ResizeObserver stays silent and
	// a static frame (reduced motion, the settled globe) would stay blurry or oversized. The query is the only signal.
	test("is an exact-resolution query for the current ratio", () => {
		expect(dprQuery(2)).toBe("(resolution: 2dppx)");
		expect(dprQuery(1)).toBe("(resolution: 1dppx)");
		expect(dprQuery(1.5)).toBe("(resolution: 1.5dppx)");
	});
	test("a bad ratio falls back to 1, never produces an invalid query", () => {
		expect(dprQuery(0)).toBe("(resolution: 1dppx)");
		expect(dprQuery(Number.NaN)).toBe("(resolution: 1dppx)");
		expect(dprQuery(Infinity)).toBe("(resolution: 1dppx)");
	});
});

describe("the two mount sites share the same sizing path", () => {
	// Both the first-run overlay (position: fixed, full window) and the empty pane (position: relative in a grid) mount the
	// same Globe in the same Hero; only the box differs. The plan must handle both boxes without special cases.
	test("full-window overlay and a pane-sized box both plan a correct buffer", () => {
		const overlay = planFrame({ cssW: 1280, cssH: 820, dpr: 2 }, { width: 300, height: 150 });
		const pane = planFrame({ cssW: 1012, cssH: 764, dpr: 2 }, { width: 300, height: 150 });
		expect(overlay.resize).toEqual({ width: 2560, height: 1640 });
		expect(pane.resize).toEqual({ width: 2024, height: 1528 });
		expect(overlay.w).toBe(1280);
		expect(pane.w).toBe(1012);
	});
});
