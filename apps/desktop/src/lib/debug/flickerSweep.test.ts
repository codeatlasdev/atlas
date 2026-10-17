// @ts-nocheck bun test runtime types are not part of the app tsconfig
// Incident: the site videos (recorded from the real app) showed flicker the app's own tests never saw, because every
// existing check looks at DOM/main-thread numbers, not at the pixels the compositor actually showed. `flickerSweep.ts`
// judges a recording: these tests feed it frame grids built to look like real defects and like real, legitimate motion.
import { describe, expect, test } from "bun:test";
import {
	analyze,
	CLEAN_PROBE,
	blankFrames,
	blips,
	cropRect,
	type FlickerOpts,
	type Frame,
	frameStarts,
	GRID_H,
	GRID_W,
	gridOf,
	verdict,
} from "./flickerSweep.ts";

/** A frame of uniform luminance `l` (0..1) with optional per-cell overrides, at `t` seconds. */
function frame(t: number, l = 0.2, cells: Record<number, number> = {}): Frame {
	const grid = new Array<number>(GRID_W * GRID_H).fill(l);
	for (const [i, v] of Object.entries(cells)) grid[Number(i)] = v;
	return { t, grid };
}
/** `n` identical frames from `t0` at `fps`. */
const still = (n: number, t0 = 0, fps = 60, l = 0.2, cells: Record<number, number> = {}) =>
	Array.from({ length: n }, (_, i) => frame(t0 + i / fps, l, cells));

const opts: Partial<FlickerOpts> = {};
/**
 * Stand-in for the studio's `flashWindows` (BT.1702 is tested there, and with the real judge on a real flashing clip in
 * scripts/flicker-sweep.test.ts): reports a window when the luminance crosses 0.4 upward more than 3 times in a second.
 */
const flashWindows = (lum: number[], fps: number) => {
	const ups: number[] = [];
	for (let i = 1; i < lum.length; i++) if (lum[i - 1]! < 0.4 && lum[i]! >= 0.4) ups.push(i);
	return ups.length > 3 && ups[ups.length - 1]! - ups[0]! < fps
		? [{ start: ups[0]! / fps, end: (ups[ups.length - 1]! + 1) / fps }]
		: [];
};

describe("a single-frame blip is a flicker", () => {
	test("a region that differs for exactly one frame, neighbours equal: reported with its time and cell box", () => {
		const f = still(30);
		// a 3x2 block of cells jumps to white for one frame (frame 10)
		const hit = { 40: 0.9, 41: 0.9, 42: 0.9, [40 + GRID_W]: 0.9, [41 + GRID_W]: 0.9, [42 + GRID_W]: 0.9 };
		f[10] = frame(f[10]!.t, 0.2, hit);
		const found = blips(f, opts);
		expect(found).toHaveLength(1);
		expect(found[0]!.frame).toBe(10);
		expect(found[0]!.t).toBeCloseTo(10 / 60, 5);
		expect(found[0]!.cells).toBeGreaterThanOrEqual(6);
		// the reported box covers the changed cells, in grid units
		expect(found[0]!.box.x0).toBeLessThanOrEqual(40 % GRID_W);
		expect(found[0]!.box.x1).toBeGreaterThanOrEqual((42 % GRID_W) + 1);
	});

	test("a monotonic change (state A then state B, one transition) is NOT a blip", () => {
		const f = [...still(10), ...still(10, 10 / 60, 60, 0.8)];
		expect(blips(f, opts)).toEqual([]);
	});

	test("an overshoot that settles on a DIFFERENT value (0.2, 0.9, 0.5) is a transition, not a return: not a blip", () => {
		// away from both neighbours, but the neighbours disagree: the page moved to a new state through a peak
		const f = [...still(10, 0, 60, 0.2), frame(10 / 60, 0.9), ...still(10, 11 / 60, 60, 0.5)];
		expect(blips(f, opts)).toEqual([]);
	});

	test("a ramp (fade/slide over many frames) is NOT a blip: each frame sits between its neighbours", () => {
		const f = Array.from({ length: 20 }, (_, i) => frame(i / 60, 0.2 + i * 0.03));
		expect(blips(f, opts)).toEqual([]);
	});

	test("a 2-frame excursion that returns to the old state is a blip too (flash lasting 33 ms)", () => {
		const f = still(30);
		f[10] = frame(f[10]!.t, 0.9);
		f[11] = frame(f[11]!.t, 0.9);
		const found = blips(f, opts);
		expect(found.length).toBeGreaterThanOrEqual(1);
		expect(found[0]!.frame).toBe(10);
		expect(found[0]!.length).toBe(2);
	});

	test("a 3+ frame excursion is a deliberate state (e.g. a tooltip showing), not a blip", () => {
		const f = still(30);
		for (const i of [10, 11, 12, 13]) f[i] = frame(f[i]!.t, 0.9);
		expect(blips(f, opts)).toEqual([]);
	});

	test("a tiny region (a caret or a 1-cell spinner tick) under the area floor is ignored", () => {
		const f = still(30);
		f[10] = frame(f[10]!.t, 0.2, { 50: 0.9 });
		expect(blips(f, opts)).toEqual([]);
	});

	test("a difference below the luminance delta (antialiasing, a subtle hover tint) is ignored", () => {
		const f = still(30);
		const cells: Record<number, number> = {};
		for (let i = 0; i < 40; i++) cells[i] = 0.24;
		f[10] = frame(f[10]!.t, 0.2, cells);
		expect(blips(f, opts)).toEqual([]);
	});

	test("the first and last frame can never be a blip (no neighbour on one side)", () => {
		const f = still(10);
		f[0] = frame(0, 0.9);
		f[9] = frame(9 / 60, 0.9);
		expect(blips(f, opts)).toEqual([]);
	});

	test("two blips of different places are both reported, in time order", () => {
		const f = still(60);
		f[10] = frame(f[10]!.t, 0.9);
		f[40] = frame(f[40]!.t, 0.9);
		expect(blips(f, opts).map((b) => b.frame)).toEqual([10, 40]);
	});

	test("variable frame rate: a frame that sits between two far-apart timestamps is judged by order, not by clock", () => {
		// the recorder only emits a frame when the picture changed: a long gap is a still page, not a missing frame
		const f = [frame(0), frame(1.5), frame(1.516, 0.9), frame(1.533), frame(3.0)];
		expect(blips(f, opts)).toHaveLength(1);
		expect(blips(f, opts)[0]!.t).toBeCloseTo(1.516, 5);
	});
});

describe("blank frames", () => {
	test("a black frame inside lit content is blank (a layer lost for a frame)", () => {
		const f = still(20, 0, 60, 0.3);
		f[8] = frame(f[8]!.t, 0);
		expect(blankFrames(f, opts).map((b) => b.frame)).toEqual([8]);
	});

	test("a frame whose every cell is identical while neighbours are textured is blank (a cleared compositor surface)", () => {
		const textured = (t: number) => frame(t, 0.3, { 5: 0.8, 90: 0.1, 130: 0.6, 200: 0.05 });
		const f = [textured(0), textured(1 / 60), frame(2 / 60, 0.3), textured(3 / 60), textured(4 / 60)];
		expect(blankFrames(f, opts).map((b) => b.frame)).toEqual([2]);
	});

	test("a black run longer than maxLen next to content is a state the page chose (a closed view), not a blank frame", () => {
		const f = [...still(10, 0, 60, 0.3), ...still(5, 10 / 60, 60, 0), ...still(10, 15 / 60, 60, 0.3)];
		expect(blankFrames(f, opts)).toEqual([]);
		expect(blankFrames(f, { maxLen: 5 }).map((b) => b.frame)).toEqual([10, 11, 12, 13, 14]);
	});

	test("an app that is legitimately dark everywhere (a dark theme splash) is not blank", () => {
		expect(blankFrames(still(20, 0, 60, 0.01), opts)).toEqual([]);
	});

	test("black at the very start (the window not yet drawn) is the boot, not a flicker, only when `skipLeading` says so", () => {
		const f = [frame(0, 0), frame(1 / 60, 0), ...still(10, 2 / 60, 60, 0.3)];
		expect(blankFrames(f, { skipLeading: 2 })).toEqual([]);
		expect(blankFrames(f, { skipLeading: 0 }).map((b) => b.frame)).toEqual([0, 1]);
	});
});

describe("analyze: the one verdict a release reads", () => {
	test("clean content: no findings", () => {
		expect(analyze(still(120), flashWindows, opts)).toEqual({ blips: [], blanks: [], flashes: [] });
	});

	test("a blank frame is reported once, not also as a blip", () => {
		const f = still(30, 0, 60, 0.3);
		f[10] = frame(f[10]!.t, 0);
		const r = analyze(f, flashWindows, opts);
		expect(r.blanks.map((b) => b.frame)).toEqual([10]);
		expect(r.blips).toEqual([]);
	});

	test("whole-frame luminance flashing (BT.1702: > 3 pairs of opposing steps in a second) is reported", () => {
		// 8 whole-screen flips between dark and bright inside a second at 60 fps, each state held 3 frames
		const f: Frame[] = [];
		for (let i = 0; i < 60; i++) f.push(frame(i / 60, Math.floor(i / 3) % 2 ? 0.7 : 0.1));
		const r = analyze(f, flashWindows, opts);
		expect(r.flashes.length).toBeGreaterThan(0);
		expect(r.flashes[0]!.end).toBeGreaterThan(r.flashes[0]!.start);
	});

	test("one flash (a theme switch) is not flashing", () => {
		const f = [...still(30, 0, 60, 0.1), ...still(30, 0.5, 60, 0.7)];
		expect(analyze(f, flashWindows, opts).flashes).toEqual([]);
	});

	test("the flash judge is injected: the app's type program never imports the studio's Bun-typed code", () => {
		const calls: Array<{ n: number; fps: number }> = [];
		const spy = (lum: number[], fps: number) => (calls.push({ n: lum.length, fps }), [{ start: 1, end: 2 }]);
		const r = analyze(still(61), spy, opts);
		expect(calls).toEqual([{ n: 61, fps: 60 }]);
		expect(r.flashes).toEqual([{ start: 1, end: 2 }]);
	});

	test("fewer than 3 frames cannot be judged: the report says so instead of passing", () => {
		expect(() => analyze(still(2), flashWindows, opts)).toThrow(/at least 3 frames/);
	});
});

describe("gridOf: raw rgb24 -> the luminance grid the detector reads", () => {
	// 2 cells wide x 1 high of raw pixels would be too coarse; build a real GRID_W x GRID_H image of 2x2-pixel cells
	const px = 2;
	const w = GRID_W * px;
	const h = GRID_H * px;
	const rgb = (paint: (x: number, y: number) => [number, number, number]) => {
		const buf = new Uint8Array(w * h * 3);
		for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) buf.set(paint(x, y), (y * w + x) * 3);
		return buf;
	};

	test("a white image is luminance 1 in every cell, black is 0", () => {
		expect(
			gridOf(
				rgb(() => [255, 255, 255]),
				w,
				h,
			).every((v) => Math.abs(v - 1) < 1e-9),
		).toBe(true);
		expect(
			gridOf(
				rgb(() => [0, 0, 0]),
				w,
				h,
			).every((v) => v === 0),
		).toBe(true);
	});

	// Incident (alpha.13 site media): the judge called frame 471 of demo.gif "black" while it was the dark TUI with text
	// (sRGB mean 0.114). The grid is LINEAR luminance (WCAG), but `blackMean` 0.02 was an sRGB-looking number: linear
	// 0.02 is sRGB ~39/255, above the background of most dark themes. Through the real grid, a near-black theme
	// background between two lit frames must not be a blank frame; true black and a 5/255 near-black must be.
	test("a dark theme background is not black; black and near-black are", () => {
		const at = (t: number, c: [number, number, number]) => ({
			t,
			grid: gridOf(
				rgb(() => c),
				w,
				h,
			),
		});
		const lit: [number, number, number] = [200, 200, 200];
		for (const theme of [
			[24, 24, 27],
			[30, 31, 41],
			[18, 18, 18],
		] as [number, number, number][])
			expect(blankFrames([at(0, lit), at(1 / 60, theme), at(2 / 60, lit)], opts), String(theme)).toEqual([]);
		for (const black of [
			[0, 0, 0],
			[5, 5, 5],
		] as [number, number, number][])
			expect(
				blankFrames([at(0, lit), at(1 / 60, black), at(2 / 60, lit)], opts).map((b) => b.kind),
				String(black),
			).toEqual(["black"]);
	});

	test("each cell averages its own pixels: a white pixel in a black cell raises only that cell, by 1/(px*px)", () => {
		const g = gridOf(
			rgb((x, y) => (x === 0 && y === 0 ? [255, 255, 255] : [0, 0, 0])),
			w,
			h,
		);
		expect(g[0]).toBeCloseTo(1 / (px * px), 9);
		expect(g.slice(1).every((v) => v === 0)).toBe(true);
	});

	test("cells are row-major: a bright pixel in the 2nd cell of the 2nd row is index GRID_W + 1", () => {
		const g = gridOf(
			rgb((x, y) => (x === px && y === px ? [255, 255, 255] : [0, 0, 0])),
			w,
			h,
		);
		expect(g.findIndex((v) => v > 0)).toBe(GRID_W + 1);
	});

	test("luminance is perceptual (WCAG): pure green is far brighter than pure blue", () => {
		const green = gridOf(
			rgb(() => [0, 255, 0]),
			w,
			h,
		)[0]!;
		const blue = gridOf(
			rgb(() => [0, 0, 255]),
			w,
			h,
		)[0]!;
		expect(green).toBeGreaterThan(blue * 5);
	});

	test("a buffer of the wrong length is an error, not a silently shifted grid", () => {
		expect(() => gridOf(new Uint8Array(10), w, h)).toThrow(/rgb24/);
	});

	test("an image smaller than the grid cannot be sampled", () => {
		expect(() => gridOf(new Uint8Array(3 * 3 * 3), 3, 3)).toThrow(/smaller than/);
	});
});

describe("frameStarts: presentation times from ffprobe", () => {
	test("times are made relative to the first frame and sorted", () => {
		expect(frameStarts("5.5\n5.0\n5.25\n")).toEqual([0, 0.25, 0.5]);
	});

	test("ffprobe's trailing blank lines and `N/A` rows are dropped, never turned into NaN", () => {
		expect(frameStarts("1.0\nN/A\n\n1.5\n")).toEqual([0, 0.5]);
	});

	test("no frames: empty, so the caller can refuse to pass an empty recording", () => {
		expect(frameStarts("")).toEqual([]);
	});
});

describe("cropRect: where on the real frame a finding is, with context", () => {
	test("a grid box maps to pixels of the recording and is padded", () => {
		const r = cropRect({ x0: 4, y0: 3, x1: 6, y1: 5 }, 2400, 1500, 0);
		expect(r).toEqual({ x: 400, y: 300, w: 200, h: 200 });
	});

	test("padding grows the box in cells and never leaves the frame", () => {
		const r = cropRect({ x0: 0, y0: 0, x1: 1, y1: 1 }, 2400, 1500, 2);
		expect(r.x).toBe(0);
		expect(r.y).toBe(0);
		expect(r.w).toBe(300);
		const edge = cropRect({ x0: GRID_W - 1, y0: GRID_H - 1, x1: GRID_W, y1: GRID_H }, 2400, 1500, 2);
		expect(edge.x + edge.w).toBe(2400);
		expect(edge.y + edge.h).toBe(1500);
	});

	test("even sizes: the H.264/PNG tools reject odd crop dimensions", () => {
		const r = cropRect({ x0: 1, y0: 1, x1: 2, y1: 2 }, 2001, 1501, 0);
		expect(r.w % 2).toBe(0);
		expect(r.h % 2).toBe(0);
	});
});

describe("verdict: what the script prints and its exit code", () => {
	const clean = { blips: [], blanks: [], flashes: [] };

	test("clean -> exit 0", () => {
		expect(verdict([{ surface: "boot", frames: 200, findings: clean }])).toEqual({ code: 0, lines: [] });
	});

	test("a blip names the surface, the time and the box -> exit 1", () => {
		const v = verdict([
			{
				surface: "settings-open-close",
				frames: 200,
				findings: {
					...clean,
					blips: [
						{ frame: 10, t: 1.234, length: 1, cells: 9, box: { x0: 1, y0: 2, x1: 4, y1: 5 }, peak: 0.6 },
					],
				},
			},
		]);
		expect(v.code).toBe(1);
		expect(v.lines.join("\n")).toMatch(/settings-open-close/);
		expect(v.lines.join("\n")).toMatch(/1\.234/);
		expect(v.lines.join("\n")).toMatch(/blip/);
	});

	test("a surface that recorded too few frames to judge is a failure, never a pass", () => {
		const v = verdict([{ surface: "terminal", frames: 0, findings: clean }]);
		expect(v.code).toBe(1);
		expect(v.lines.join("\n")).toMatch(/terminal.*frames/);
	});

	test("one bad surface among clean ones still fails the sweep", () => {
		const v = verdict([
			{ surface: "a", frames: 100, findings: clean },
			{ surface: "b", frames: 100, findings: { ...clean, blanks: [{ frame: 3, t: 0.05, kind: "black" }] } },
		]);
		expect(v.code).toBe(1);
		expect(v.lines.some((l) => l.includes("b") && l.includes("black"))).toBe(true);
	});

	test("a surface whose script threw was not exercised: it fails the sweep and says why, even with clean frames", () => {
		const v = verdict([
			{
				surface: "dialogs",
				frames: 300,
				findings: clean,
				undriven: 'no element matches [aria-label="Settings"]',
			},
		]);
		expect(v.code).toBe(1);
		expect(v.lines.join("\n")).toMatch(/dialogs: could not be driven.*Settings/);
	});

	test("flashing is reported with its window", () => {
		const v = verdict([
			{ surface: "toast", frames: 100, findings: { ...clean, flashes: [{ start: 0.5, end: 1.4 }] } },
		]);
		expect(v.code).toBe(1);
		expect(v.lines.join("\n")).toMatch(/flash/);
	});
});

describe("CLEAN_PROBE: a surface starts and ends on the bare app", () => {
	// Incident (first full run): toast-error left the "Error details" dialog open; terminal, resize, browser and tabs were then
	// filmed with a modal over the app, and the sweep reported them clean. The probe is page JS the runner evaluates between
	// surfaces; it runs here against the real DOM (happy-dom is registered for every app test).
	const open = (html: string) => {
		document.body.innerHTML = html;
		return new Function(`return (${CLEAN_PROBE})`)() as string[];
	};

	test("an open dialog is reported by its label", () => {
		expect(open('<div role="dialog" data-state="open" aria-label="Error details"></div>')).toEqual([
			"Error details",
		]);
	});
	test("an open dialog without a label is reported by its role", () => {
		expect(open('<div role="alertdialog" data-state="open"></div>')).toEqual(["alertdialog"]);
	});
	test("a closed dialog (the kit keeps them mounted) is not reported", () => {
		expect(open('<div role="dialog" data-state="closed" aria-label="Settings"></div>')).toEqual([]);
	});
	test("two open dialogs are both reported", () => {
		expect(
			open(
				'<div role="dialog" data-state="open" aria-label="A"></div><div role="dialog" data-state="open" aria-label="B"></div>',
			),
		).toEqual(["A", "B"]);
	});
	test("the bare app, with toasts showing, is clean", () => {
		expect(
			open('<div role="region" aria-label="Errors"><div role="alert" class="toast"></div></div>'),
		).toEqual([]);
	});
});
