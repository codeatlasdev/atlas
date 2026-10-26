// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, test } from "bun:test";
import contract from "./hit.contract.json";
import { applyZoom } from "#lib/zoom.ts";
import { hitPayload, overlayRects, startHitMap } from "./hit.ts";

// The router (`natives/router.rs`) hands an interactive native view the mouse over its slot EXCEPT inside the rects the
// page publishes here: whatever the page paints over a slot and owns input in. It is measured when an overlay opens or
// closes (never per frame), sent only when it changed, and before the pixels: the order a click needs.

const rect = (x: number, y: number, w: number, h: number) =>
	({ left: x, top: y, right: x + w, bottom: y + h, width: w, height: h }) as DOMRect;

function layer(at: DOMRect, role = "dialog", open = true) {
	const d = document.createElement("div");
	d.setAttribute("role", role);
	if (open) d.dataset.state = "open";
	d.getBoundingClientRect = () => at;
	document.body.append(d);
	return d;
}

function modal(open = true) {
	const d = document.createElement("dialog");
	d.setAttribute("role", "dialog");
	if (open) d.dataset.state = "open";
	d.matches = (sel: string) => (sel === ":modal" ? open : Element.prototype.matches.call(d, sel));
	d.getBoundingClientRect = () => rect(300, 198, 300, 200);
	document.body.append(d);
	return d;
}

const realCS = window.getComputedStyle;
beforeEach(() => {
	document.body.innerHTML = "";
	Object.defineProperty(window, "innerWidth", { value: 1280, configurable: true });
	Object.defineProperty(window, "innerHeight", { value: 820, configurable: true });
});
afterEach(() => {
	window.getComputedStyle = realCS;
});

test("nothing over the page: no rects", () => {
	expect(overlayRects()).toEqual([]);
});

test("an open dialog, menu, listbox and tooltip are each a rect at their box; a closed one is none", () => {
	layer(rect(500, 300, 400, 300), "menu");
	layer(rect(10, 10, 50, 40), "listbox");
	layer(rect(70, 70, 20, 10), "tooltip");
	layer(rect(1, 1, 9, 9), "menu", false);
	expect(overlayRects()).toEqual([
		{ x: 500, y: 300, w: 400, h: 300 },
		{ x: 10, y: 10, w: 50, h: 40 },
		{ x: 70, y: 70, w: 20, h: 10 },
	]);
});

test("a modal is the whole window: its scrim takes the pointer everywhere, not only under its panel", () => {
	modal();
	expect(overlayRects()).toContainEqual({ x: 0, y: 0, w: 1280, h: 820 });
});

test("an open edge peek is its layout box even mid-slide; closed it is none", () => {
	const p = document.createElement("div");
	p.className = "peek-panel left";
	p.dataset.state = "open";
	for (const [k, v] of Object.entries({
		offsetLeft: 190,
		offsetTop: 40,
		offsetWidth: 260,
		offsetHeight: 700,
	}))
		Object.defineProperty(p, k, { value: v });
	p.getBoundingClientRect = () => rect(-9999, 40, 260, 700); // parked off-window mid-slide
	document.body.append(p);
	expect(overlayRects()).toEqual([{ x: 190, y: 40, w: 260, h: 700 }]);
	p.dataset.state = "closed";
	expect(overlayRects()).toEqual([]);
});

test("any element marked data-native-cover is a rect: the hook for future overlays", () => {
	const d = document.createElement("div");
	d.setAttribute("data-native-cover", "");
	d.getBoundingClientRect = () => rect(5, 6, 70, 80);
	document.body.append(d);
	expect(overlayRects()).toEqual([{ x: 5, y: 6, w: 70, h: 80 }]);
});

// Contract (kept from the old cover): a menu scaling in from 0.95 is reported at its FINAL box, so a click on the
// part it has not grown into yet is already the page's.
test("a menu mid scale-in is reported at its FINAL box, not at the shrunken bounding rect", () => {
	const d = layer(rect(250, 150, 100 * 0.95, 80 * 0.95), "menu");
	Object.defineProperty(d, "offsetWidth", { value: 100 });
	Object.defineProperty(d, "offsetHeight", { value: 80 });
	window.getComputedStyle = (e, p) =>
		e === d ? { transform: "matrix(0.95, 0, 0, 0.95, 0, 0)", transformOrigin: "0px 0px" } : realCS(e, p);
	expect(overlayRects()).toEqual([{ x: 250, y: 150, w: 100, h: 80 }]);
});

test("a kit floating layer the engine has not placed yet (top-layer origin) is no rect yet", () => {
	const d = layer(rect(0, 0, 100, 80), "menu");
	d.classList.add("ca-floating");
	expect(overlayRects()).toEqual([]);
	d.style.translate = "250px 150px";
	d.getBoundingClientRect = () => rect(250, 150, 100, 80);
	expect(overlayRects()).toEqual([{ x: 250, y: 150, w: 100, h: 80 }]);
});

test("a closing menu keeps its rect until its exit animation ends: it is still painted over the slot", () => {
	const d = layer(rect(250, 150, 100, 80), "menu");
	d.dataset.state = "closed";
	d.getAnimations = () => [{}];
	expect(overlayRects()).toEqual([{ x: 250, y: 150, w: 100, h: 80 }]);
	d.getAnimations = () => [];
	expect(overlayRects()).toEqual([]);
});

test("duplicates (the same overlay found twice) are reported once", () => {
	const d = layer(rect(250, 150, 100, 80), "menu");
	d.setAttribute("data-native-cover", "");
	expect(overlayRects()).toEqual([{ x: 250, y: 150, w: 100, h: 80 }]);
});

test("window points: under layout zoom a CSS px rect is sent scaled (what an NSView frame takes)", () => {
	layer(rect(100, 50, 200, 100), "menu");
	applyZoom(1.25);
	try {
		expect(overlayRects()).toEqual([{ x: 125, y: 62.5, w: 250, h: 125 }]);
	} finally {
		applyZoom(1);
	}
});

// ── Publishing: only when it changes, never per frame, with the exact keys Rust reads.
type Cb = () => void;
const frames: Cb[] = [];
const realRaf = globalThis.requestAnimationFrame;
const realRO = globalThis.ResizeObserver;
beforeEach(() => {
	frames.length = 0;
	globalThis.requestAnimationFrame = (f: Cb) => frames.push(f);
});
afterEach(() => {
	globalThis.requestAnimationFrame = realRaf;
	globalThis.ResizeObserver = realRO;
});
const flush = () => {
	// one animation frame's worth: callbacks queued while flushing run in the next
	const run = frames.splice(0);
	run.forEach((f) => f());
};
const tick = () => new Promise<void>((r) => queueMicrotask(r));

function harness() {
	const calls: { cmd: string; args: { hit: { rects: unknown[] } } }[] = [];
	const invoke = async (cmd: string, args?: unknown) => void calls.push({ cmd, args: args as never });
	const stop = startHitMap(invoke);
	return { calls, stop, hits: () => calls.filter((c) => c.cmd === "natives_hit") };
}

test("the payload has exactly the keys natives::HitMap reads (the very bytes Rust's contract test deserializes)", () => {
	layer(rect(300, 198, 300, 200), "menu");
	modal();
	const sent = JSON.parse(JSON.stringify(hitPayload()));
	// the fixture's two rects: the menu at (300,198) 300x200 and the modal's whole window
	expect(sent).toEqual(contract);
});

test("an overlay opening is sent on the next frame; the same set again is not sent twice", async () => {
	const { hits, stop } = harness();
	flush();
	const first = hits().length;
	layer(rect(500, 300, 400, 300), "menu");
	await tick();
	flush();
	expect(hits().length).toBe(first + 1);
	expect(hits().at(-1).args.hit.rects).toEqual([{ x: 500, y: 300, w: 400, h: 300 }]);
	await tick();
	flush();
	flush();
	expect(hits().length).toBe(first + 1);
	stop();
});

test("closing the overlay sends the empty set, so a click where it was is the native view's again", async () => {
	const { hits, stop } = harness();
	const d = layer(rect(500, 300, 400, 300), "menu");
	await tick();
	flush();
	d.dataset.state = "closed";
	await tick();
	flush();
	expect(hits().at(-1).args.hit.rects).toEqual([]);
	stop();
});

test("a floating layer placed a frame after it opened is re-measured: the rect lands once it has a place", async () => {
	const { hits, stop } = harness();
	const d = layer(rect(0, 0, 100, 80), "menu");
	d.classList.add("ca-floating");
	await tick();
	flush();
	expect(hits().at(-1).args.hit.rects).toEqual([]); // not placed: no rect yet
	d.style.translate = "250px 150px";
	d.getBoundingClientRect = () => rect(250, 150, 100, 80);
	flush(); // the follow-up measures the opening scheduled
	flush();
	flush();
	expect(hits().at(-1).args.hit.rects).toEqual([{ x: 250, y: 150, w: 100, h: 80 }]);
	stop();
});

test("no per-frame loop: with nothing changing no frame is requested and nothing is sent", async () => {
	const { hits, stop } = harness();
	layer(rect(500, 300, 400, 300), "menu");
	await tick();
	flush();
	flush();
	flush();
	const n = hits().length;
	frames.length = 0;
	flush();
	expect(frames.length).toBe(0);
	expect(hits().length).toBe(n);
	stop();
});

test("a modal opening or closing (the kit's layerchange) is measured even when no data-state flipped", async () => {
	const { hits, stop } = harness();
	flush();
	const d = modal();
	document.dispatchEvent(new CustomEvent("layerchange", { detail: { modal: true } }));
	flush();
	expect(hits().at(-1).args.hit.rects).toContainEqual({ x: 0, y: 0, w: 1280, h: 820 });
	d.remove();
	document.dispatchEvent(new CustomEvent("layerchange", { detail: { modal: false } }));
	flush();
	expect(hits().at(-1).args.hit.rects).toEqual([]);
	stop();
});

test("teardown stops listening and cancels what is pending", async () => {
	const { hits, stop } = harness();
	flush();
	const n = hits().length;
	stop();
	layer(rect(500, 300, 400, 300), "menu");
	await tick();
	flush();
	expect(hits().length).toBe(n);
});
