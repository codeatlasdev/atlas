// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { carve, clipPath, disjoint, holeCount, resetHoles, setHole } from "./ground.ts";

// The page is transparent exactly over each native slot: the grounds that paint under the slot (the window's
// `.window-ground`, the space layer's `.ground`, the slot's own fill) carry a hole there, written in the SAME task as
// the slot change (docs/desktop.md "Native views"). A hole that lags the slot shows stale pixels or the window
// material; one that outlives the slot's visibility leaves a see-through rectangle.

const rect = (x: number, y: number, w: number, h: number) => new DOMRect(x, y, w, h);

function el(cls: string, at: DOMRect, parent: Element = document.body) {
	const d = document.createElement("div");
	d.className = cls;
	d.getBoundingClientRect = () => at;
	parent.append(d);
	return d;
}

/** window ground + one layer (with its ground) + a slot inside the layer. */
function scene(slotAt = rect(300, 120, 500, 300)) {
	document.documentElement.classList.add("ground-holes");
	const windowGround = el("window-ground", rect(0, 0, 1280, 820));
	const layer = el("space-layer", rect(260, 60, 900, 700));
	const ground = el("ground", rect(260, 60, 900, 700), layer);
	const slot = el("area", slotAt, layer);
	return { windowGround, layer, ground, slot };
}

beforeEach(() => {
	resetHoles();
	document.body.innerHTML = "";
	document.documentElement.className = "";
});
afterEach(() => {
	document.documentElement.className = "";
});

const path = (e: Element) => e.style.clipPath;

test("a slot hole is carved, in each ground's own coordinates, out of the window ground, the layer ground and the slot", () => {
	const { windowGround, ground, slot } = scene();
	// the native view's box: the slot minus a 2 px strip on top (the loading bar belongs to the page)
	setHole(slot, { left: 300, top: 122, right: 800, bottom: 420 });
	// window ground: window coordinates
	expect(path(windowGround)).toBe(clipPath([{ x: 300, y: 122, w: 500, h: 298 }], 1280, 820));
	// layer ground: relative to the layer (260, 60)
	expect(path(ground)).toBe(clipPath([{ x: 40, y: 62, w: 500, h: 298 }], 900, 700));
	// the slot's own fill: relative to the slot (300, 120): padding and the strip stay painted
	expect(path(slot)).toBe(clipPath([{ x: 0, y: 2, w: 500, h: 298 }], 500, 300));
});

test("clearing a hole (the view is hidden or parked) paints the slot again everywhere, in the same call", () => {
	const { windowGround, ground, slot } = scene();
	setHole(slot, { left: 300, top: 122, right: 800, bottom: 420 });
	setHole(slot, null);
	expect(path(windowGround)).toBe("");
	expect(path(ground)).toBe("");
	expect(path(slot)).toBe("");
	expect(holeCount()).toBe(0);
});

test("two slots in one layer carve two holes; hiding one leaves the other", () => {
	const { windowGround, ground, layer, slot } = scene();
	const other = el("area", rect(800, 120, 300, 300), layer);
	setHole(slot, { left: 300, top: 120, right: 800, bottom: 420 });
	setHole(other, { left: 800, top: 120, right: 1100, bottom: 420 });
	// the two panes touch at x = 800: one merged run, 800 wide
	expect(path(ground)).toBe(clipPath([{ x: 40, y: 60, w: 800, h: 300 }], 900, 700));
	setHole(other, null);
	expect(path(ground)).toBe(clipPath([{ x: 40, y: 60, w: 500, h: 300 }], 900, 700));
	expect(path(windowGround)).toBe(clipPath([{ x: 300, y: 120, w: 500, h: 300 }], 1280, 820));
});

test("a hole in another layer does not carve this layer's ground", () => {
	const a = scene();
	const layerB = el("space-layer", rect(1160, 60, 900, 700)); // the parked neighbour, one page to the right
	const groundB = el("ground", rect(1160, 60, 900, 700), layerB);
	const slotB = el("area", rect(1200, 120, 500, 300), layerB);
	setHole(slotB, { left: 1200, top: 120, right: 1700, bottom: 420 });
	expect(path(a.ground)).toBe("");
	// relative to its OWN layer, so a parked layer's hole is right the moment it slides in
	expect(path(groundB)).toBe(clipPath([{ x: 40, y: 60, w: 500, h: 300 }], 900, 700));
});

test("a layer that moves (a swipe transform) keeps its hole right: the rects are relative, both read with the transform", () => {
	const { ground, layer, slot } = scene();
	setHole(slot, { left: 300, top: 120, right: 800, bottom: 420 });
	const before = path(ground);
	// the layer slides 200 px left: its ground and its slot move together
	layer.getBoundingClientRect = () => rect(60, 60, 900, 700);
	ground.getBoundingClientRect = () => rect(60, 60, 900, 700);
	slot.getBoundingClientRect = () => rect(100, 120, 500, 300);
	setHole(slot, { left: 100, top: 120, right: 600, bottom: 420 });
	expect(path(ground)).toBe(before);
});

test("an unchanged hole writes nothing (no style write, no repaint)", () => {
	const { ground, slot } = scene();
	const box = { left: 300, top: 120, right: 800, bottom: 420 };
	setHole(slot, box);
	let writes = 0;
	const real = ground.style.setProperty.bind(ground.style);
	ground.style.setProperty = (...a) => (writes++, real(...a));
	setHole(slot, { ...box });
	setHole(slot, { ...box });
	expect(writes).toBe(0);
	setHole(slot, { ...box, right: 801 });
	expect(writes).toBe(1);
});

test("without the engine class (Linux/Windows, or before the shell started) nothing is carved: the page stays opaque", () => {
	const { windowGround, ground, slot } = scene();
	document.documentElement.classList.remove("ground-holes");
	setHole(slot, { left: 300, top: 120, right: 800, bottom: 420 });
	expect(path(windowGround)).toBe("");
	expect(path(ground)).toBe("");
	expect(path(slot)).toBe("");
});

test("a hole outside its ground is clipped to it; an empty or broken one is dropped", () => {
	const { windowGround, slot } = scene();
	setHole(slot, { left: -50, top: 700, right: 100, bottom: 900 });
	expect(path(windowGround)).toBe(clipPath([{ x: 0, y: 700, w: 100, h: 120 }], 1280, 820));
	setHole(slot, { left: 10, top: 10, right: 10, bottom: 50 });
	expect(path(windowGround)).toBe("");
	setHole(slot, { left: Number.NaN, top: 0, right: 5, bottom: 5 });
	expect(path(windowGround)).toBe("");
});

// Incident (2026-10-26, Settings over a browser tab): a rect reported twice made the even-odd clip fill its own hole
// back in, so the page stayed on top. Same class here: the grounds are clipped even-odd.
test("overlapping and duplicate holes become disjoint rects covering the same area", () => {
	const r = disjoint([
		{ x: 0, y: 0, w: 100, h: 100 },
		{ x: 0, y: 0, w: 100, h: 100 },
		{ x: 50, y: 50, w: 100, h: 100 },
	]);
	const area = r.reduce((s, c) => s + c.w * c.h, 0);
	expect(area).toBe(100 * 100 * 2 - 50 * 50);
	for (const [i, a] of r.entries())
		for (const b of r.slice(i + 1)) {
			const ox = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
			const oy = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
			expect(ox <= 0 || oy <= 0).toBe(true);
		}
});

test("the clip path is the whole ground minus the holes, even-odd, in px of the ground's own box", () => {
	expect(clipPath([{ x: 10, y: 20, w: 30, h: 40 }], 100, 200)).toBe(
		'path(evenodd, "M0 0H100V200H0ZM10 20H40V60H10Z")',
	);
	expect(clipPath([], 100, 200)).toBe("");
});

test("carve writes one ground from the holes inside it, by rect, with no DOM beyond the element", () => {
	const g = el("ground", rect(100, 100, 400, 300));
	carve(g, [{ left: 150, top: 150, right: 250, bottom: 250 }]);
	expect(path(g)).toBe(clipPath([{ x: 50, y: 50, w: 100, h: 100 }], 400, 300));
	carve(g, []);
	expect(path(g)).toBe("");
});
