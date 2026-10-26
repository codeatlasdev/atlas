// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { closeHole, openHole, resetHoles } from "./holeLayers.ts";
import { contentRect } from "./fonts.ts";
import { maskStyle, overlap, sameHoles } from "./holes.ts";

// The terminal is a native view UNDER the page: the page cuts its rect out of the layers that paint over it
// (`.ground`, `.card`) and nothing else. A dialog, menu or peek paints above those layers, so it covers the terminal
// with no native call. These are the rules that make that true.

const hole = (x: number, y: number, w: number, h: number) => ({ x, y, w, h });

test("no hole means no mask at all", () => {
	expect(maskStyle([])["mask-image"]).toBe("none");
	expect(maskStyle([hole(0, 0, 0, 10)])["mask-image"]).toBe("none");
	expect(maskStyle([hole(NaN, 0, 10, 10)])["mask-image"]).toBe("none");
});

test("each hole adds one excluded layer sized and placed at the rect", () => {
	const m = maskStyle([hole(10, 20, 300, 200), hole(400, 20, 100.456, 50)]);
	expect(m["mask-size"]).toBe("100% 100%, 300px 200px, 100.46px 50px");
	expect(m["mask-position"]).toBe("0 0, 10px 20px, 400px 20px");
	expect(m["mask-composite"]).toBe("exclude, exclude, exclude");
	expect(m["-webkit-mask-composite"]).toBe("xor, xor, xor");
	expect(m["mask-repeat"]).toBe("no-repeat");
});

test("holes are the same set whatever their order, and a moved rect is a change", () => {
	expect(sameHoles([hole(0, 0, 1, 1), hole(2, 2, 3, 3)], [hole(2, 2, 3, 3), hole(0, 0, 1, 1)])).toBe(true);
	expect(sameHoles([hole(0, 0, 1, 1)], [hole(0, 0, 1, 2)])).toBe(false);
	expect(sameHoles([], [hole(0, 0, 1, 1)])).toBe(false);
});

test("overlap is strict: touching edges are not an overlap", () => {
	expect(overlap(hole(0, 0, 10, 10), hole(10, 0, 10, 10))).toBe(false);
	expect(overlap(hole(0, 0, 10, 10), hole(9, 9, 10, 10))).toBe(true);
});

// contentRect is what both the native view and the hole use: the padding band is the slot's border.
test("contentRect is the slot inside its padding border, the same box for the view and the hole", () => {
	const el = document.createElement("div");
	el.getBoundingClientRect = () => ({ left: 100, top: 50, width: 400, height: 300 });
	Object.defineProperty(el, "clientLeft", { value: 8 });
	Object.defineProperty(el, "clientTop", { value: 4 });
	Object.defineProperty(el, "clientWidth", { value: 384 });
	Object.defineProperty(el, "clientHeight", { value: 292 });
	expect(contentRect(el)).toEqual({ left: 108, top: 54, width: 384, height: 292 });
});

// holeLayers: the DOM half. Real elements, real mask writes.
type Cb = () => void;
const frames: Cb[] = [];
const realRaf = globalThis.requestAnimationFrame;
beforeEach(() => {
	frames.length = 0;
	globalThis.requestAnimationFrame = (f: Cb) => frames.push(f);
	document.body.innerHTML = '<div class="ground"></div><div class="card"></div>';
	for (const el of document.querySelectorAll(".ground, .card"))
		el.getBoundingClientRect = () => ({ left: 0, top: 0, width: 1000, height: 800 });
});
afterEach(() => {
	globalThis.requestAnimationFrame = realRaf;
	resetHoles();
	document.body.innerHTML = "";
});
const flush = () => frames.splice(0).forEach((f) => f());

test("opening a hole cuts it out of the ground and the card in the next frame, once", () => {
	openHole("t", () => ({ left: 10, top: 20, width: 300, height: 200 }));
	openHole("u", () => ({ left: 400, top: 20, width: 100, height: 50 }));
	expect(frames.length).toBe(1);
	flush();
	for (const el of document.querySelectorAll(".ground, .card"))
		expect(el.style.getPropertyValue("mask-size")).toBe("100% 100%, 300px 200px, 100px 50px");
});

test("closing the last hole removes the mask: the page paints that rect again", () => {
	openHole("t", () => ({ left: 10, top: 20, width: 300, height: 200 }));
	flush();
	closeHole("t");
	flush();
	expect(document.querySelector(".card").style.getPropertyValue("mask-image")).toBe("none");
});

test("an unchanged set does not rewrite the mask", () => {
	openHole("t", () => ({ left: 10, top: 20, width: 300, height: 200 }));
	flush();
	const card = document.querySelector(".card");
	card.style.setProperty("mask-size", "sentinel");
	openHole("t", () => ({ left: 10, top: 20, width: 300, height: 200 }));
	flush();
	expect(card.style.getPropertyValue("mask-size")).toBe("sentinel");
});

test("a layer inside a offset parent gets rects in its own coordinates", () => {
	document.querySelector(".card").getBoundingClientRect = () => ({ left: 100, top: 40, width: 800, height: 700 });
	openHole("t", () => ({ left: 110, top: 60, width: 300, height: 200 }));
	flush();
	expect(document.querySelector(".card").style.getPropertyValue("mask-position")).toBe("0 0, 10px 20px");
	expect(document.querySelector(".ground").style.getPropertyValue("mask-position")).toBe("0 0, 110px 60px");
});

// Blank-terminal audit: a hole whose slot is gone (unmounted, zero rect) must never leave the page cut.
test("a hole whose rect function returns null contributes nothing", () => {
	openHole("t", () => null);
	flush();
	expect(document.querySelector(".card").style.getPropertyValue("mask-image")).toBe("none");
});
