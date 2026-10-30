// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	applyBlur,
	arrowHead,
	BEAUTIFY_DEFAULT,
	BEAUTIFY_GRADIENTS,
	BEAUTIFY_PADDINGS,
	BEAUTIFY_RADII,
	beautifyLayout,
	blurPatch,
	centerView,
	counterRadius,
	markerBounds,
	spotlightHoles,
	textBg,
	textFont,
	paintBackdrop,
	paintBeautified,
	exportSize,
	fitZoom,
	imageToView,
	paintItems,
	pixelateBlocks,
	textBox,
	viewToImage,
	zoomAt,
} from "./render.ts";

/** A plain image: `fn(x, y)` returns the grey level of each pixel, alpha opaque. */
function grey(width, height, fn) {
	const data = new Uint8ClampedArray(width * height * 4);
	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			const i = (y * width + x) * 4;
			data[i] = data[i + 1] = data[i + 2] = fn(x, y);
			data[i + 3] = 255;
		}
	}
	return { data, width, height };
}
const px = (img, x, y) => img.data[(y * img.width + x) * 4];
const variance = (img, r) => {
	const v = [];
	for (let y = r.y; y < r.y + r.h; y++) for (let x = r.x; x < r.x + r.w; x++) v.push(px(img, x, y));
	const m = v.reduce((a, b) => a + b, 0) / v.length;
	return v.reduce((a, b) => a + (b - m) ** 2, 0) / v.length;
};

describe("arrowHead", () => {
	test("the barbs sit head-length behind the tip at +-28 degrees from the shaft", () => {
		const [a, b] = arrowHead({ x: 0, y: 0 }, { x: 100, y: 0 }, 5); // len = 4*5 = 20
		const rad = (28 * Math.PI) / 180;
		expect(a.x).toBeCloseTo(100 - 20 * Math.cos(rad), 6);
		expect(Math.abs(a.y)).toBeCloseTo(20 * Math.sin(rad), 6);
		expect(b.x).toBeCloseTo(a.x, 6);
		expect(b.y).toBeCloseTo(-a.y, 6);
	});

	test("head length is 4*width clamped to 10..40", () => {
		const len = (w) => {
			const [a] = arrowHead({ x: 0, y: 0 }, { x: 500, y: 0 }, w);
			return Math.hypot(500 - a.x, a.y);
		};
		expect(len(1)).toBeCloseTo(10, 6);
		expect(len(5)).toBeCloseTo(20, 6);
		expect(len(100)).toBeCloseTo(40, 6);
	});

	test("rotates with the arrow direction", () => {
		const [a, b] = arrowHead({ x: 0, y: 0 }, { x: 0, y: 100 }, 5);
		// pointing down: both barbs are above the tip, mirrored around x = 0
		expect(a.y).toBeLessThan(100);
		expect(a.x + b.x).toBeCloseTo(0, 6);
	});

	test("a zero-length arrow produces finite points (no NaN)", () => {
		for (const p of arrowHead({ x: 7, y: 7 }, { x: 7, y: 7 }, 4)) {
			expect(Number.isFinite(p.x)).toBe(true);
			expect(Number.isFinite(p.y)).toBe(true);
		}
	});
});

describe("pixelateBlocks", () => {
	const area = (bs) => bs.reduce((s, b) => s + b.w * b.h, 0);

	test("block size is max(2, round(amount))", () => {
		expect(pixelateBlocks({ x: 0, y: 0, w: 4, h: 4 }, 0)[0]).toEqual({ x: 0, y: 0, w: 2, h: 2 });
		expect(pixelateBlocks({ x: 0, y: 0, w: 40, h: 40 }, 9.6)[0]).toEqual({ x: 0, y: 0, w: 10, h: 10 });
	});

	test("covers the rect exactly: partial last row/column, no overlap, nothing outside", () => {
		const r = { x: 3, y: 5, w: 11, h: 7 };
		const bs = pixelateBlocks(r, 4);
		expect(area(bs)).toBe(r.w * r.h);
		const seen = new Set();
		for (const b of bs) {
			expect(b.x).toBeGreaterThanOrEqual(r.x);
			expect(b.y).toBeGreaterThanOrEqual(r.y);
			expect(b.x + b.w).toBeLessThanOrEqual(r.x + r.w);
			expect(b.y + b.h).toBeLessThanOrEqual(r.y + r.h);
			for (let y = b.y; y < b.y + b.h; y++) {
				for (let x = b.x; x < b.x + b.w; x++) {
					const k = `${x},${y}`;
					expect(seen.has(k)).toBe(false);
					seen.add(k);
				}
			}
		}
		// 11 = 4+4+3 columns, 7 = 4+3 rows
		expect(bs).toHaveLength(6);
		expect(bs.at(-1)).toEqual({ x: 11, y: 9, w: 3, h: 3 });
	});

	test("an empty or inverted rect has no blocks", () => {
		expect(pixelateBlocks({ x: 0, y: 0, w: 0, h: 10 }, 4)).toEqual([]);
		expect(pixelateBlocks({ x: 0, y: 0, w: 10, h: -2 }, 4)).toEqual([]);
	});
});

describe("zoom and pan", () => {
	test("fitZoom fits the image in the viewport minus padding and never upscales", () => {
		expect(fitZoom({ w: 2000, h: 1000 }, { w: 1100, h: 800 }, 50)).toBeCloseTo(0.5, 6);
		expect(fitZoom({ w: 1000, h: 2000 }, { w: 1100, h: 1100 }, 50)).toBeCloseTo(0.5, 6);
		expect(fitZoom({ w: 100, h: 50 }, { w: 1100, h: 800 }, 50)).toBe(1);
	});

	test("fitZoom survives a viewport smaller than its padding and an empty image", () => {
		const z = fitZoom({ w: 2000, h: 1000 }, { w: 40, h: 40 }, 50);
		expect(Number.isFinite(z)).toBe(true);
		expect(z).toBeGreaterThan(0);
		expect(fitZoom({ w: 0, h: 0 }, { w: 800, h: 600 }, 10)).toBe(1);
	});

	test("imageToView and viewToImage round trip", () => {
		const v = { zoom: 0.37, x: 120.5, y: -33 };
		for (const p of [
			{ x: 0, y: 0 },
			{ x: 1234, y: 567 },
			{ x: -20, y: 9999 },
		]) {
			const back = viewToImage(imageToView(p, v), v);
			expect(back.x).toBeCloseTo(p.x, 9);
			expect(back.y).toBeCloseTo(p.y, 9);
		}
		expect(imageToView({ x: 10, y: 20 }, { zoom: 2, x: 5, y: 7 })).toEqual({ x: 25, y: 47 });
	});

	test("centerView puts the image centre at the viewport centre", () => {
		const v = centerView({ w: 400, h: 200 }, { w: 1000, h: 600 }, 2);
		expect(imageToView({ x: 200, y: 100 }, v)).toEqual({ x: 500, y: 300 });
	});

	test("zoomAt keeps the image point under the anchor fixed", () => {
		const v = { zoom: 1, x: 40, y: 10 };
		const anchor = { x: 300, y: 200 };
		const before = viewToImage(anchor, v);
		const z = zoomAt(v, 3, anchor);
		expect(z.zoom).toBe(3);
		const after = viewToImage(anchor, z);
		expect(after.x).toBeCloseTo(before.x, 9);
		expect(after.y).toBeCloseTo(before.y, 9);
	});

	test("zoomAt clamps the zoom to a sane range", () => {
		expect(zoomAt({ zoom: 1, x: 0, y: 0 }, 1000, { x: 0, y: 0 }).zoom).toBeLessThanOrEqual(32);
		expect(zoomAt({ zoom: 1, x: 0, y: 0 }, 0, { x: 0, y: 0 }).zoom).toBeGreaterThan(0);
	});
});

describe("exportSize", () => {
	test("is the crop size when cropped, the full size otherwise", () => {
		expect(exportSize({ width: 800, height: 600, crop: null })).toEqual({ w: 800, h: 600 });
		expect(exportSize({ width: 800, height: 600, crop: { x: 10, y: 20, w: 300, h: 150 } })).toEqual({
			w: 300,
			h: 150,
		});
	});
});

describe("textBox", () => {
	test("is approximate: grows with the length and the size, has a minimum width for an empty string", () => {
		expect(textBox("abcd", 20).w).toBeGreaterThan(textBox("ab", 20).w);
		expect(textBox("ab", 40).w).toBeGreaterThan(textBox("ab", 20).w);
		expect(textBox("ab", 40).h).toBeGreaterThan(textBox("ab", 20).h);
		expect(textBox("", 20).w).toBeGreaterThan(0);
		expect(textBox("", 20).h).toBeGreaterThanOrEqual(20);
	});
});

describe("applyBlur: the pixels are replaced, not overlaid", () => {
	const checker = () => grey(4, 4, (x, y) => ((x + y) % 2 ? 0 : 255));

	test("pixelate on a 4x4 checker averages each 2x2 block", () => {
		const img = checker();
		applyBlur(img, { x: 0, y: 0, w: 4, h: 4 }, "pixelate", 2);
		for (let y = 0; y < 4; y++) {
			for (let x = 0; x < 4; x++) {
				expect(px(img, x, y)).toBe(128); // (255+255+0+0)/4 = 127.5
				expect(img.data[(y * 4 + x) * 4 + 3]).toBe(255);
			}
		}
	});

	test("pixelate with different blocks keeps each block's own mean", () => {
		const img = grey(4, 2, (x) => (x < 2 ? 0 : 200));
		applyBlur(img, { x: 0, y: 0, w: 4, h: 2 }, "pixelate", 2);
		expect([px(img, 0, 0), px(img, 1, 1), px(img, 2, 0), px(img, 3, 1)]).toEqual([0, 0, 200, 200]);
		const mixed = grey(4, 2, (x) => (x === 0 ? 100 : x === 1 ? 0 : 50));
		applyBlur(mixed, { x: 0, y: 0, w: 4, h: 2 }, "pixelate", 2);
		expect(px(mixed, 0, 0)).toBe(50);
		expect(px(mixed, 1, 1)).toBe(50);
		expect(px(mixed, 2, 0)).toBe(50);
	});

	test("pixelate leaves every pixel outside the rect untouched", () => {
		const img = grey(8, 8, (x, y) => (x * 31 + y * 17) % 256);
		const before = Uint8ClampedArray.from(img.data);
		applyBlur(img, { x: 2, y: 3, w: 4, h: 3 }, "pixelate", 2);
		let changedInside = 0;
		for (let y = 0; y < 8; y++) {
			for (let x = 0; x < 8; x++) {
				const i = (y * 8 + x) * 4;
				const inside = x >= 2 && x < 6 && y >= 3 && y < 6;
				if (!inside) expect(img.data.slice(i, i + 4)).toEqual(before.slice(i, i + 4));
				else if (img.data[i] !== before[i]) changedInside++;
			}
		}
		expect(changedInside).toBeGreaterThan(0);
	});

	test("blur mode is monotonic: more amount, less variance, and the region differs from the source", () => {
		const noise = () => grey(32, 32, (x, y) => (x * 73 + y * 151 + x * y * 7) % 256);
		const src = noise();
		const rect = { x: 4, y: 4, w: 24, h: 24 };
		const v0 = variance(src, rect);
		const light = noise();
		applyBlur(light, rect, "blur", 1);
		const strong = noise();
		applyBlur(strong, rect, "blur", 4);
		const v1 = variance(light, rect);
		const v2 = variance(strong, rect);
		expect(v1).toBeLessThan(v0);
		expect(v2).toBeLessThan(v1);
		expect(light.data).not.toEqual(src.data);
	});

	test("blur mode leaves every pixel outside the rect untouched and never reads outside it", () => {
		// Outside the rect is pure white: if the blur sampled it, the inside would brighten beyond black.
		const img = grey(16, 16, (x, y) => (x >= 4 && x < 12 && y >= 4 && y < 12 ? 0 : 255));
		const before = Uint8ClampedArray.from(img.data);
		applyBlur(img, { x: 4, y: 4, w: 8, h: 8 }, "blur", 3);
		for (let y = 0; y < 16; y++) {
			for (let x = 0; x < 16; x++) {
				const i = (y * 16 + x) * 4;
				const inside = x >= 4 && x < 12 && y >= 4 && y < 12;
				if (inside) expect(img.data[i]).toBe(0);
				else expect(img.data.slice(i, i + 4)).toEqual(before.slice(i, i + 4));
			}
		}
	});

	test("a rect hanging off the image is clipped to it: right/bottom overflow averages only the visible pixels", () => {
		const img = grey(6, 6, (x) => (x === 4 ? 200 : x === 5 ? 100 : 0));
		applyBlur(img, { x: 4, y: 4, w: 50, h: 50 }, "pixelate", 2);
		// the visible 2x2 block at (4,4) is {200,100,200,100}: mean 150, and nothing else changed
		expect([px(img, 4, 4), px(img, 5, 4), px(img, 4, 5), px(img, 5, 5)]).toEqual([150, 150, 150, 150]);
		expect([px(img, 3, 4), px(img, 4, 3), px(img, 0, 0)]).toEqual([0, 200, 0]);
	});

	test("a rect hanging off the left/top edge does not wrap onto the previous row", () => {
		// x = -4: unclipped, row y would index into the end of row y-1 and corrupt the right edge of the image
		const img = grey(6, 6, (x, y) => (x === 5 ? 250 : y * 10));
		const before = Uint8ClampedArray.from(img.data);
		applyBlur(img, { x: -4, y: 2, w: 6, h: 2 }, "pixelate", 2);
		for (let y = 0; y < 6; y++) {
			for (let x = 0; x < 6; x++) {
				const i = (y * 6 + x) * 4;
				const touched = x < 2 && y >= 2 && y < 4;
				if (!touched) expect(img.data[i]).toBe(before[i]);
			}
		}
	});

	test("a rect fully outside the image is a no-op", () => {
		const img = grey(6, 6, (x, y) => x + y);
		const before = Uint8ClampedArray.from(img.data);
		applyBlur(img, { x: -10, y: -10, w: 5, h: 5 }, "blur", 2);
		applyBlur(img, { x: 6, y: 0, w: 5, h: 5 }, "pixelate", 2);
		expect(img.data).toEqual(before);
	});

	test("an empty rect is a no-op", () => {
		const img = checker();
		const before = Uint8ClampedArray.from(img.data);
		applyBlur(img, { x: 1, y: 1, w: 0, h: 3 }, "pixelate", 2);
		expect(img.data).toEqual(before);
	});
});

describe("blurPatch", () => {
	test("returns the blurred region only and leaves the base untouched", () => {
		const base = grey(8, 8, (x, y) => ((x + y) % 2 ? 0 : 255));
		const before = Uint8ClampedArray.from(base.data);
		const patch = blurPatch(base, {
			type: "blur",
			id: 1,
			rect: { x: 2, y: 2, w: 4, h: 4 },
			mode: "pixelate",
			amount: 2,
		});
		expect(base.data).toEqual(before);
		expect(patch).toMatchObject({ x: 2, y: 2, width: 4, height: 4 });
		expect(patch.data).toHaveLength(4 * 4 * 4);
		expect(px(patch, 0, 0)).toBe(128);
	});

	test("clips to the image and returns null when nothing is left", () => {
		const base = grey(8, 8, () => 10);
		const item = (rect) => ({ type: "blur", id: 1, rect, mode: "pixelate", amount: 2 });
		expect(blurPatch(base, item({ x: 6, y: 6, w: 10, h: 10 }))).toMatchObject({
			x: 6,
			y: 6,
			width: 2,
			height: 2,
		});
		expect(blurPatch(base, item({ x: 20, y: 20, w: 5, h: 5 }))).toBeNull();
	});
});

describe("paintItems", () => {
	function fakeCtx() {
		const calls = [];
		const ctx = new Proxy(
			{ calls },
			{
				get: (t, k) => (k === "calls" ? calls : (...a) => void calls.push([String(k), ...a])),
				set: (t, k, v) => (calls.push(["set:" + String(k), v]), true),
			},
		);
		return ctx;
	}
	const opts = (base) => ({ base, toImageData: (p) => p });
	const names = (ctx) => ctx.calls.map((c) => c[0]);

	test("a blur item replaces the pixels with real blurred data (putImageData), not a translucent fill", () => {
		const base = grey(8, 8, (x, y) => ((x + y) % 2 ? 0 : 255));
		const ctx = fakeCtx();
		paintItems(
			ctx,
			[{ type: "blur", id: 1, rect: { x: 0, y: 0, w: 4, h: 4 }, mode: "pixelate", amount: 2 }],
			opts(base),
		);
		const put = ctx.calls.find((c) => c[0] === "putImageData");
		expect(put).toBeDefined();
		expect(put[2]).toBe(0);
		expect(put[3]).toBe(0);
		expect(px(put[1], 1, 1)).toBe(128);
		expect(names(ctx)).not.toContain("fillRect");
	});

	test("items are painted in z-order", () => {
		const base = grey(8, 8, () => 0);
		const ctx = fakeCtx();
		paintItems(
			ctx,
			[
				{ type: "rect", id: 1, rect: { x: 0, y: 0, w: 4, h: 4 }, color: "#111111", width: 2, fill: null },
				{ type: "blur", id: 2, rect: { x: 0, y: 0, w: 4, h: 4 }, mode: "pixelate", amount: 2 },
				{ type: "rect", id: 3, rect: { x: 1, y: 1, w: 2, h: 2 }, color: "#222222", width: 2, fill: null },
			],
			opts(base),
		);
		const order = ctx.calls
			.filter((c) => c[0] === "set:strokeStyle" || c[0] === "putImageData")
			.map((c) => c[1]);
		expect(order[0]).toBe("#111111");
		expect(typeof order[1]).toBe("object"); // the blur patch lands between the two rects
		expect(order[2]).toBe("#222222");
	});

	test("an arrow draws a shaft and a head with finite coordinates, even with zero length", () => {
		const ctx = fakeCtx();
		paintItems(
			ctx,
			[{ type: "arrow", id: 1, from: { x: 5, y: 5 }, to: { x: 5, y: 5 }, color: "#ef4444", width: 4 }],
			opts(grey(8, 8, () => 0)),
		);
		for (const c of ctx.calls)
			for (const a of c.slice(1)) if (typeof a === "number") expect(Number.isFinite(a)).toBe(true);
		expect(names(ctx)).toContain("stroke");
		expect(names(ctx)).toContain("fill");
	});

	test("text is drawn with fillText at the item origin", () => {
		const ctx = fakeCtx();
		paintItems(
			ctx,
			[{ type: "text", id: 1, at: { x: 12, y: 34 }, text: "hi", color: "#fff", size: 20 }],
			opts(grey(8, 8, () => 0)),
		);
		const t = ctx.calls.find((c) => c[0] === "fillText");
		expect(t.slice(1)).toEqual(["hi", 12, 34]);
	});
});

// ---------------------------------------------------------------------------------------------------------------
// Counter, marker, spotlight, text style, beautify
// ---------------------------------------------------------------------------------------------------------------
function recCtx() {
	const calls = [];
	return new Proxy(
		{ calls },
		{
			get: (t, k) => {
				if (k === "calls") return calls;
				// A gradient is an object the painter keeps adding stops to, not a fire-and-forget call.
				if (k === "createLinearGradient")
					return (...a) => {
						calls.push([String(k), ...a]);
						const g = { stops: [] };
						// Non-enumerable, so two runs of the same painting compare equal by their stops.
						Object.defineProperty(g, "addColorStop", {
							value: (...s) => void g.stops.push(s),
							enumerable: false,
						});
						return g;
					};
				return (...a) => void calls.push([String(k), ...a]);
			},
			set: (t, k, v) => (calls.push(["set:" + String(k), v]), true),
		},
	);
}
const setOf = (ctx, k) => ctx.calls.filter((c) => c[0] === "set:" + k).map((c) => c[1]);
const base8 = () => grey(8, 8, () => 0);
const o8 = () => ({ base: base8(), toImageData: (p) => p });

describe("counterRadius", () => {
	test("grows with the stroke width, whole pixels, never below a readable disc", () => {
		const r = [2, 4, 6, 10].map(counterRadius);
		expect(r).toEqual([...r].sort((a, b) => a - b));
		expect(new Set(r).size).toBe(4);
		expect(counterRadius(0)).toBeGreaterThanOrEqual(10);
		expect(counterRadius(-5)).toBe(counterRadius(0));
		for (const v of r) expect(Number.isInteger(v)).toBe(true);
	});
});

describe("markerBounds", () => {
	test("the box of the points grown by half the width on every side", () => {
		expect(
			markerBounds(
				[
					{ x: 10, y: 20 },
					{ x: 50, y: 30 },
				],
				10,
			),
		).toEqual({ x: 5, y: 15, w: 50, h: 20 });
	});
	test("one point is a disc of the width; no points is an empty box at the origin", () => {
		expect(markerBounds([{ x: 10, y: 10 }], 6)).toEqual({ x: 7, y: 7, w: 6, h: 6 });
		expect(markerBounds([], 6)).toEqual({ x: 0, y: 0, w: 0, h: 0 });
	});
});

describe("textFont / textBg", () => {
	test("the font string carries weight and size, with a system sans-serif stack", () => {
		expect(textFont(28, "regular")).toMatch(/^400 28px system-ui/);
		expect(textFont(72, "bold")).toMatch(/^700 72px system-ui/);
		expect(textFont(18, "bold")).not.toMatch(/\{|\}/);
	});
	test("bold text is wider in the approximate box, never narrower", () => {
		expect(textBox("hello", 28, "bold").w).toBeGreaterThan(textBox("hello", 28, "regular").w);
		expect(textBox("hello", 28).w).toBe(textBox("hello", 28, "regular").w);
	});
	test("no background paints no box; solid is a square-ish padded box; pill is fully rounded", () => {
		const box = { w: 100, h: 35 };
		expect(textBg("none", { x: 10, y: 10 }, box, 28)).toBeNull();
		const solid = textBg("solid", { x: 10, y: 10 }, box, 28);
		const pill = textBg("pill", { x: 10, y: 10 }, box, 28);
		for (const b of [solid, pill]) {
			expect(b.x).toBeLessThan(10);
			expect(b.y).toBeLessThan(10);
			expect(b.w).toBeGreaterThan(box.w);
			expect(b.h).toBeGreaterThan(box.h);
		}
		expect(solid.r).toBeLessThan(pill.r);
		expect(pill.r).toBe(pill.h / 2);
	});
	test("a readable text colour on the box: dark on light, white on dark", () => {
		expect(textBg("solid", { x: 0, y: 0 }, { w: 10, h: 10 }, 20, "#ffffff").fg).toBe("#111111");
		expect(textBg("solid", { x: 0, y: 0 }, { w: 10, h: 10 }, 20, "#111111").fg).toBe("#ffffff");
		expect(textBg("pill", { x: 0, y: 0 }, { w: 10, h: 10 }, 20, "#facc15").fg).toBe("#111111");
		expect(textBg("pill", { x: 0, y: 0 }, { w: 10, h: 10 }, 20, "#3b82f6").fg).toBe("#ffffff");
	});
});

describe("spotlightHoles", () => {
	const spot = (id, rect, shape = "rect") => ({ id, type: "spotlight", rect, shape });
	test("one hole per spotlight, in z-order, ignoring other items and empty rects", () => {
		const items = [
			spot(1, { x: 1, y: 2, w: 30, h: 20 }),
			{ id: 2, type: "rect", rect: { x: 0, y: 0, w: 5, h: 5 }, color: "#fff", width: 2, fill: null },
			spot(3, { x: 50, y: 50, w: 10, h: 10 }, "ellipse"),
			spot(4, { x: 70, y: 70, w: 0, h: 10 }),
		];
		expect(spotlightHoles(items)).toEqual([
			{ rect: { x: 1, y: 2, w: 30, h: 20 }, shape: "rect" },
			{ rect: { x: 50, y: 50, w: 10, h: 10 }, shape: "ellipse" },
		]);
		expect(spotlightHoles([])).toEqual([]);
	});
});

describe("paintItems: new kinds", () => {
	test("a counter paints a filled disc and its number, centred, with finite coordinates", () => {
		const ctx = recCtx();
		paintItems(ctx, [{ id: 7, type: "counter", at: { x: 4, y: 5 }, color: "#3b82f6", width: 4 }], {
			...o8(),
			numbers: new Map([[7, 3]]),
		});
		const t = ctx.calls.find((c) => c[0] === "fillText");
		expect(t.slice(1, 4)).toEqual(["3", 4, 5]);
		expect(setOf(ctx, "textAlign")).toContain("center");
		expect(setOf(ctx, "textBaseline")).toContain("middle");
		const arc = ctx.calls.find((c) => c[0] === "arc");
		expect(arc.slice(1, 4)).toEqual([4, 5, counterRadius(4)]);
		expect(setOf(ctx, "fillStyle")[0]).toBe("#3b82f6");
		for (const c of ctx.calls)
			for (const a of c.slice(1)) if (typeof a === "number") expect(Number.isFinite(a)).toBe(true);
	});
	test("a counter without a number entry still draws the disc (no 'undefined' text)", () => {
		const ctx = recCtx();
		paintItems(ctx, [{ id: 7, type: "counter", at: { x: 4, y: 5 }, color: "#3b82f6", width: 4 }], o8());
		expect(ctx.calls.some((c) => c[0] === "fillText" && c[1] === "undefined")).toBe(false);
	});

	test("a spotlight dims everything outside with ONE even-odd fill: full image + one hole per spotlight", () => {
		const ctx = recCtx();
		const spot = (id, rect, shape) => ({ id, type: "spotlight", rect, shape });
		paintItems(
			ctx,
			[spot(1, { x: 1, y: 1, w: 3, h: 3 }, "rect"), spot(2, { x: 4, y: 4, w: 3, h: 3 }, "ellipse")],
			{
				...o8(),
				size: { w: 8, h: 8 },
			},
		);
		const fills = ctx.calls.filter((c) => c[0] === "fill");
		expect(fills).toHaveLength(1);
		expect(fills[0][1]).toBe("evenodd");
		expect(ctx.calls.filter((c) => c[0] === "rect")[0].slice(1)).toEqual([0, 0, 8, 8]);
		expect(ctx.calls.filter((c) => c[0] === "rect")).toHaveLength(2); // the image + the rect hole
		expect(ctx.calls.filter((c) => c[0] === "ellipse")).toHaveLength(1);
		expect(setOf(ctx, "fillStyle")[0]).toMatch(/^rgba\(0, ?0, ?0, ?0\.\d+\)$/);
	});
	test("several spotlights are one dim layer, painted at the z position of the FIRST spotlight", () => {
		const ctx = recCtx();
		paintItems(
			ctx,
			[
				{ id: 1, type: "rect", rect: { x: 0, y: 0, w: 2, h: 2 }, color: "#111111", width: 2, fill: null },
				{ id: 2, type: "spotlight", rect: { x: 1, y: 1, w: 3, h: 3 }, shape: "rect" },
				{ id: 3, type: "rect", rect: { x: 0, y: 0, w: 2, h: 2 }, color: "#222222", width: 2, fill: null },
				{ id: 4, type: "spotlight", rect: { x: 4, y: 4, w: 3, h: 3 }, shape: "rect" },
			],
			{ ...o8(), size: { w: 8, h: 8 } },
		);
		const order = ctx.calls
			.filter((c) => c[0] === "set:strokeStyle" || c[0] === "fill")
			.map((c) => (c[0] === "fill" ? "dim" : c[1]));
		expect(order).toEqual(["#111111", "dim", "#222222"]);
	});
	test("without an image size the spotlight cannot dim (nothing is drawn rather than a wrong layer)", () => {
		const ctx = recCtx();
		paintItems(ctx, [{ id: 1, type: "spotlight", rect: { x: 1, y: 1, w: 3, h: 3 }, shape: "rect" }], o8());
		expect(ctx.calls.filter((c) => c[0] === "fill")).toHaveLength(0);
	});

	test("a marker is one multiply stroke at 40% alpha, round joins, over all its points", () => {
		const ctx = recCtx();
		paintItems(
			ctx,
			[
				{
					id: 1,
					type: "marker",
					points: [
						{ x: 1, y: 1 },
						{ x: 4, y: 2 },
						{ x: 6, y: 6 },
					],
					color: "#facc15",
					width: 10,
				},
			],
			o8(),
		);
		expect(setOf(ctx, "globalCompositeOperation")).toContain("multiply");
		expect(setOf(ctx, "globalAlpha")).toContain(0.4);
		expect(setOf(ctx, "lineWidth")).toContain(10);
		expect(setOf(ctx, "lineCap")).toContain("round");
		expect(setOf(ctx, "strokeStyle")).toContain("#facc15");
		expect(ctx.calls.filter((c) => c[0] === "moveTo")).toHaveLength(1);
		expect(ctx.calls.filter((c) => c[0] === "lineTo")).toHaveLength(2);
		expect(ctx.calls.filter((c) => c[0] === "stroke")).toHaveLength(1);
		// state is restored so the alpha does not leak into the next item
		expect(ctx.calls.filter((c) => c[0] === "save")).toHaveLength(
			ctx.calls.filter((c) => c[0] === "restore").length,
		);
	});
	test("a one-point marker is a dot, not an empty path", () => {
		const ctx = recCtx();
		paintItems(ctx, [{ id: 1, type: "marker", points: [{ x: 3, y: 3 }], color: "#facc15", width: 10 }], o8());
		expect(ctx.calls.some((c) => c[0] === "arc" || c[0] === "lineTo")).toBe(true);
	});

	test("text uses the style font, and a pill/solid background is drawn BEFORE the glyphs", () => {
		const ctx = recCtx();
		paintItems(
			ctx,
			[
				{
					id: 1,
					type: "text",
					at: { x: 2, y: 2 },
					text: "hi",
					color: "#ffffff",
					size: 28,
					weight: "bold",
					bg: "pill",
				},
			],
			o8(),
		);
		expect(setOf(ctx, "font")[0]).toBe(textFont(28, "bold"));
		const names = ctx.calls.map((c) => c[0]);
		expect(names.indexOf("fill")).toBeGreaterThan(-1);
		expect(names.indexOf("fill")).toBeLessThan(names.indexOf("fillText"));
		// the text colour is the chosen colour for the box, the glyphs use the readable contrast colour
		expect(setOf(ctx, "fillStyle")[0]).toBe("#ffffff");
		expect(setOf(ctx, "fillStyle").at(-1)).toBe("#111111");
	});
	test("text without a background paints only glyphs, in the chosen colour; old items without style still draw", () => {
		const ctx = recCtx();
		paintItems(
			ctx,
			[
				{
					id: 1,
					type: "text",
					at: { x: 2, y: 2 },
					text: "hi",
					color: "#ef4444",
					size: 20,
					weight: "regular",
					bg: "none",
				},
				{ id: 2, type: "text", at: { x: 2, y: 2 }, text: "legacy", color: "#ef4444", size: 20 },
			],
			o8(),
		);
		expect(ctx.calls.filter((c) => c[0] === "fill")).toHaveLength(0);
		expect(ctx.calls.filter((c) => c[0] === "fillText")).toHaveLength(2);
		expect(setOf(ctx, "fillStyle")).toEqual(["#ef4444", "#ef4444"]);
	});
});

describe("beautifyLayout", () => {
	const off = { ...BEAUTIFY_DEFAULT, on: false };
	const on = (o) => ({ ...BEAUTIFY_DEFAULT, on: true, ...o });

	test("presets: paddings 0/32/64/96, radii 0/12/24, 4 gradients", () => {
		expect(BEAUTIFY_PADDINGS).toEqual([0, 32, 64, 96]);
		expect(BEAUTIFY_RADII).toEqual([0, 12, 24]);
		expect(BEAUTIFY_GRADIENTS).toHaveLength(4);
		for (const g of BEAUTIFY_GRADIENTS) {
			expect(g.stops.length).toBeGreaterThanOrEqual(2);
			expect(g.name.length).toBeGreaterThan(0);
		}
	});
	test("off: the output is the image itself, at the origin, no radius", () => {
		expect(beautifyLayout({ w: 300, h: 200 }, off)).toEqual({
			out: { w: 300, h: 200 },
			image: { x: 0, y: 0, w: 300, h: 200 },
			radius: 0,
		});
	});
	test("output size = image + 2*padding on both axes", () => {
		for (const pad of BEAUTIFY_PADDINGS) {
			const l = beautifyLayout({ w: 300, h: 200 }, on({ padding: pad }));
			expect(l.out).toEqual({ w: 300 + 2 * pad, h: 200 + 2 * pad });
			expect(l.image).toEqual({ x: pad, y: pad, w: 300, h: 200 });
		}
	});
	test("the radius is clamped to half the shorter side of the image", () => {
		expect(beautifyLayout({ w: 300, h: 200 }, on({ radius: 24 })).radius).toBe(24);
		expect(beautifyLayout({ w: 300, h: 30 }, on({ radius: 24 })).radius).toBe(15);
		expect(beautifyLayout({ w: 10, h: 10 }, on({ radius: 24 })).radius).toBe(5);
		expect(beautifyLayout({ w: 300, h: 200 }, on({ radius: -4 })).radius).toBe(0);
	});
	test("a shadow with no padding to show it in grows nothing; the shadow never changes the output size", () => {
		const a = beautifyLayout({ w: 300, h: 200 }, on({ padding: 32, shadow: true }));
		const b = beautifyLayout({ w: 300, h: 200 }, on({ padding: 32, shadow: false }));
		expect(a.out).toEqual(b.out);
	});
	test("an unknown padding or NaN falls back to the nearest preset, never a NaN size", () => {
		const l = beautifyLayout({ w: 300, h: 200 }, on({ padding: Number.NaN, radius: Number.NaN }));
		expect(Number.isFinite(l.out.w) && Number.isFinite(l.out.h)).toBe(true);
		expect(l.radius).toBe(0);
		expect(beautifyLayout({ w: 300, h: 200 }, on({ padding: 70 })).image.x).toBe(64);
	});
	test("crop THEN beautify: the layout is computed from the cropped export size", () => {
		const size = exportSize({ width: 800, height: 600, crop: { x: 10, y: 10, w: 300, h: 150 } });
		expect(beautifyLayout(size, on({ padding: 64 })).out).toEqual({ w: 428, h: 278 });
	});
});

describe("paintBeautified", () => {
	const on = (o) => ({ ...BEAUTIFY_DEFAULT, on: true, ...o });
	const img = { w: 40, h: 20 };
	const run = (b) => {
		const ctx = recCtx();
		const src = { tag: "src" };
		paintBeautified(ctx, src, img, b);
		return ctx;
	};

	test("the background fills the whole output first, the image is drawn last at its padded position", () => {
		const ctx = run(
			on({ padding: 32, radius: 12, shadow: false, background: { kind: "solid", color: "#123456" } }),
		);
		const names = ctx.calls.map((c) => c[0]);
		expect(names.indexOf("fillRect")).toBeLessThan(names.indexOf("drawImage"));
		const bg = ctx.calls.find((c) => c[0] === "fillRect");
		expect(bg.slice(1)).toEqual([0, 0, 104, 84]);
		expect(setOf(ctx, "fillStyle")[0]).toBe("#123456");
		const draw = ctx.calls.find((c) => c[0] === "drawImage");
		expect(draw.slice(2)).toEqual([32, 32, 40, 20]);
	});
	test("rounded corners clip the image to a rounded rect of the clamped radius", () => {
		const ctx = run(on({ padding: 32, radius: 24 }));
		const names = ctx.calls.map((c) => c[0]);
		expect(names).toContain("clip");
		expect(names.indexOf("clip")).toBeLessThan(names.indexOf("drawImage"));
		const rr = ctx.calls.find((c) => c[0] === "roundRect");
		expect(rr.slice(1, 6)).toEqual([32, 32, 40, 20, 10]); // 24 clamped to half of the 20px side
	});
	test("radius 0 does not clip at all", () => {
		expect(run(on({ padding: 32, radius: 0 })).calls.map((c) => c[0])).not.toContain("clip");
	});
	test("a gradient background builds a linear gradient with every stop", () => {
		const g = BEAUTIFY_GRADIENTS[0];
		let stops = 0;
		const ctx = new Proxy(
			{},
			{
				get: (t, k) =>
					k === "createLinearGradient" ? () => ({ addColorStop: () => void stops++ }) : () => {},
				set: () => true,
			},
		);
		paintBeautified(ctx, {}, img, on({ background: { kind: "gradient", index: 0 } }));
		expect(stops).toBe(g.stops.length);
	});
	test("the shadow is a soft blur with an offset, applied to the image only, and reset afterwards", () => {
		const ctx = run(on({ padding: 64, shadow: true }));
		expect(setOf(ctx, "shadowBlur").some((v) => v > 0)).toBe(true);
		expect(setOf(ctx, "shadowColor").some((v) => /rgba/.test(v))).toBe(true);
		expect(ctx.calls.filter((c) => c[0] === "save")).toHaveLength(
			ctx.calls.filter((c) => c[0] === "restore").length,
		);
		expect(setOf(run(on({ shadow: false })), "shadowBlur")).toEqual([]);
	});
	test("beautify off draws just the image at the origin", () => {
		const ctx = run({ ...BEAUTIFY_DEFAULT, on: false });
		expect(ctx.calls.filter((c) => c[0] === "fillRect")).toHaveLength(0);
		expect(ctx.calls.find((c) => c[0] === "drawImage").slice(2)).toEqual([0, 0, 40, 20]);
	});
});

describe("paintBackdrop (the live preview behind the editor canvas)", () => {
	const on = (o) => ({ ...BEAUTIFY_DEFAULT, on: true, ...o });
	test("paints the background and the shadow but never the image", () => {
		const ctx = recCtx();
		paintBackdrop(
			ctx,
			{ w: 40, h: 20 },
			on({ padding: 32, shadow: true, background: { kind: "solid", color: "#123456" } }),
		);
		expect(ctx.calls.find((c) => c[0] === "fillRect").slice(1)).toEqual([0, 0, 104, 84]);
		expect(ctx.calls.some((c) => c[0] === "drawImage")).toBe(false);
		expect(setOf(ctx, "shadowBlur").some((v) => v > 0)).toBe(true);
	});
	test("with beautify off it paints nothing", () => {
		const ctx = recCtx();
		paintBackdrop(ctx, { w: 40, h: 20 }, { ...BEAUTIFY_DEFAULT, on: false });
		expect(ctx.calls).toHaveLength(0);
	});
	test("the export is the backdrop plus the image, in that order (preview and export cannot drift apart)", () => {
		const a = recCtx();
		paintBackdrop(a, { w: 40, h: 20 }, on({ padding: 32, radius: 12 }));
		const b = recCtx();
		paintBeautified(b, {}, { w: 40, h: 20 }, on({ padding: 32, radius: 12 }));
		expect(b.calls.slice(0, a.calls.length)).toEqual(a.calls);
		expect(b.calls.at(-1)[0]).toBe("restore");
		expect(b.calls.some((c) => c[0] === "drawImage")).toBe(true);
	});
});
