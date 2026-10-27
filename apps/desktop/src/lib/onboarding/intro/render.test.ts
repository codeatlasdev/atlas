// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { paint } from "./render.ts";
import { calm, progressFrame } from "./globe.ts";

/**
 * A 2D context that does the one thing these tests need: compose the transform (`setTransform`, `scale`, `translate`,
 * `save`/`restore`) and report every fillRect in DEVICE pixels. happy-dom has no canvas, and a pixel test would only
 * restate the renderer.
 */
function recordingContext(canvas = { width: 0, height: 0 }) {
	let m = [1, 0, 0, 1, 0, 0]; // a b c d e f
	const stack = [];
	const fills = [];
	const mul = (n) => {
		const [a, b, c, d, e, f] = m;
		m = [
			a * n[0] + c * n[1],
			b * n[0] + d * n[1],
			a * n[2] + c * n[3],
			b * n[2] + d * n[3],
			a * n[4] + c * n[5] + e,
			b * n[4] + d * n[5] + f,
		];
	};
	const ctx = {
		canvas,
		matrix: () => [...m],
		setTransform: (a, b, c, d, e, f) => (m = [a, b, c, d, e, f]),
		resetTransform: () => (m = [1, 0, 0, 1, 0, 0]),
		scale: (x, y) => mul([x, 0, 0, y, 0, 0]),
		translate: (x, y) => mul([1, 0, 0, 1, x, y]),
		rotate: (r) => mul([Math.cos(r), Math.sin(r), -Math.sin(r), Math.cos(r), 0, 0]),
		transform: (...n) => mul(n),
		save: () => stack.push(m),
		restore: () => (m = stack.pop() ?? m),
		fillRect: (x, y, w, h) =>
			fills.push({ x: m[0] * x + m[2] * y + m[4], y: m[1] * x + m[3] * y + m[5], w: m[0] * w, h: m[3] * h }),
		measureText: () => ({ width: 10 }),
		createRadialGradient: () => ({ addColorStop() {} }),
		createLinearGradient: () => ({ addColorStop() {} }),
		// everything else the renderer calls is a drawing primitive with no bearing on the transform
		fill() {},
		stroke() {},
		beginPath() {},
		closePath() {},
		moveTo() {},
		lineTo() {},
		arc() {},
		rect() {},
		clip() {},
		fillText() {},
		strokeText() {},
		clearRect() {},
		drawImage() {},
		setLineDash() {},
		quadraticCurveTo() {},
		bezierCurveTo() {},
	};
	// fillStyle, globalAlpha, font, … are plain assignments; any other drawing call is a no-op
	const proxy = new Proxy(ctx, {
		set: (t, k, v) => ((t[k] = v), true),
		get: (t, k) => (k in t ? t[k] : () => {}),
	});
	return { ctx: proxy, fills };
}

// happy-dom has no Path2D; the tile paths only matter for pixels, not for the transform under test.
const hadPath2D = "Path2D" in globalThis;
beforeAll(() => {
	if (!hadPath2D) globalThis.Path2D = class {};
});
afterAll(() => {
	if (!hadPath2D) delete globalThis.Path2D;
});

describe("paint lays the scene out in CSS px under the caller's transform", () => {
	// Incident (welcome recording, 1280x800 at 2x): the globe and sky filled only the top-left quarter of the picture
	// although the canvas buffer and box were both correct. Globe.draw() sets setTransform(dpr, …) and calls
	// paint(ctx, frame, cssW, cssH), and paint() began with setTransform(1,0,0,1,0,0): it threw the scale away and
	// painted a CSS-sized scene into a buffer twice that size.
	for (const dpr of [1, 2, 3]) {
		test(`the stage fill covers the whole buffer at a pixel ratio of ${dpr}`, () => {
			const [w, h] = [1280, 790];
			const { ctx, fills } = recordingContext({ width: w * dpr, height: h * dpr });
			ctx.setTransform(dpr, 0, 0, dpr, 0, 0); // what Globe.draw() does
			paint(ctx, calm(0), w, h);
			const stage = fills[0];
			expect(stage).toMatchObject({ x: 0, y: 0, w: w * dpr, h: h * dpr });
		});
	}

	test("the same holds for the progress and calm frames", () => {
		for (const f of [progressFrame({ lit: 2, doneAt: null, now: 3, litAt: {} }), calm(2)]) {
			const { ctx, fills } = recordingContext();
			ctx.setTransform(2, 0, 0, 2, 0, 0);
			paint(ctx, f, 900, 600);
			expect(fills[0]).toMatchObject({ w: 1800, h: 1200 });
		}
	});

	test("without a caller transform it still paints at 1x (the storyboard path)", () => {
		const { ctx, fills } = recordingContext();
		paint(ctx, calm(0), 640, 400);
		expect(fills[0]).toMatchObject({ x: 0, y: 0, w: 640, h: 400 });
	});

	test("paint leaves the caller's transform as it found it, so a later frame composes the same way", () => {
		const { ctx, fills } = recordingContext();
		ctx.setTransform(2, 0, 0, 2, 0, 0);
		const before = ctx.matrix();
		paint(ctx, calm(1), 500, 300);
		expect(ctx.matrix()).toEqual(before);
		paint(ctx, calm(1.1), 500, 300);
		expect(ctx.matrix()).toEqual(before);
		// each paint opens with its stage fill over the whole 2x buffer
		expect(
			fills.filter((f) => f.x === 0 && f.y === 0 && f.w === 1000 && f.h === 600).length,
		).toBeGreaterThanOrEqual(2);
	});
});
