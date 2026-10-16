// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { overlayOpen, watchSlot } from "./nativeSlot.ts";

// Incident (desktop flicker): the native terminal/browser view is positioned over a DOM slot by an IPC call that
// each observer fired on its own, so one layout change (a pane split, a dialog opening, a space commit) pushed
// several `term_bounds` in one frame, each one reading layout in the middle of Svelte's writes. The native view
// then lagged the DOM by a frame or two and flashed. The watcher must coalesce every trigger into ONE sync per frame.

type Cb = () => void;
const frames: Cb[] = [];
let resizeCb: Cb | undefined;
const realRaf = globalThis.requestAnimationFrame;
const realCancel = globalThis.cancelAnimationFrame;
const realRO = globalThis.ResizeObserver;

beforeEach(() => {
	frames.length = 0;
	globalThis.requestAnimationFrame = (f: Cb) => frames.push(f);
	globalThis.cancelAnimationFrame = (id: number) => void (frames[id - 1] = () => {});
	globalThis.ResizeObserver = class {
		constructor(cb: Cb) {
			resizeCb = cb;
		}
		observe() {}
		disconnect() {}
	};
});
afterEach(() => {
	globalThis.requestAnimationFrame = realRaf;
	globalThis.cancelAnimationFrame = realCancel;
	globalThis.ResizeObserver = realRO;
	document.body.innerHTML = "";
});

const flush = () => frames.splice(0).forEach((f) => f());
const tick = () => new Promise<void>((r) => queueMicrotask(r));

test("a resize, a window resize and a dialog opening in the same frame make one sync, not three", async () => {
	let calls = 0;
	const stop = watchSlot(document.createElement("div"), () => calls++);
	resizeCb!();
	window.dispatchEvent(new Event("resize"));
	const dlg = document.createElement("div");
	dlg.setAttribute("role", "dialog");
	document.body.append(dlg);
	await tick(); // MutationObserver records are delivered in a microtask
	expect(calls).toBe(0); // nothing reads layout or crosses IPC before the frame
	flush();
	expect(calls).toBe(1);
	stop();
});

test("sync reads layout inside the frame callback, after the writes of the frame", async () => {
	const order: string[] = [];
	const stop = watchSlot(document.createElement("div"), () => order.push("sync"));
	resizeCb!();
	order.push("writes");
	flush();
	expect(order).toEqual(["writes", "sync"]);
	stop();
});

test("a later change schedules a new frame; teardown cancels a pending one", async () => {
	let calls = 0;
	const stop = watchSlot(document.createElement("div"), () => calls++);
	resizeCb!();
	flush();
	resizeCb!();
	flush();
	expect(calls).toBe(2);
	resizeCb!();
	stop();
	flush();
	expect(calls).toBe(2);
});

// Incident (desktop flicker): ANY open dialog parked every native terminal, even a small popover far from it (a
// "Time range" chip in the logs tab, a dialog over the sidebar), so the terminal vanished and reappeared on each open
// and close. Only a dialog that overlaps the slot can cover the native view; the rest must leave it alone.
const rect = (x: number, y: number, w: number, h: number) =>
	({ left: x, top: y, right: x + w, bottom: y + h, width: w, height: h }) as DOMRect;

function dialog(at: DOMRect, open = true) {
	const d = document.createElement("div");
	d.setAttribute("role", "dialog");
	if (open) d.dataset.state = "open";
	d.getBoundingClientRect = () => at;
	document.body.append(d);
	return d;
}

const SLOT = rect(200, 100, 600, 400);

test("an open dialog over the slot parks the native view", () => {
	dialog(rect(300, 150, 200, 100));
	expect(overlayOpen(SLOT)).toBe(true);
});

test("an open dialog elsewhere (sidebar, far corner) leaves the native view alone", () => {
	dialog(rect(0, 0, 150, 300));
	expect(overlayOpen(SLOT)).toBe(false);
});

test("a dialog that only touches the slot's edge does not overlap it", () => {
	dialog(rect(800, 100, 100, 100));
	expect(overlayOpen(SLOT)).toBe(false);
});

test("a closed dialog never parks", () => {
	dialog(rect(300, 150, 200, 100), false);
	expect(overlayOpen(SLOT)).toBe(false);
});

test("a full-window modal (palette scrim, first run) parks every slot", () => {
	dialog(rect(0, 0, 1280, 800));
	expect(overlayOpen(SLOT)).toBe(true);
});

// Regression (#43): the collapsed-sidebar hover peek is an HTML panel (role=complementary, no data-state), so it
// neither parked the native browser/terminal nor triggered a re-sync: it slid in BEHIND the native view.
// An open peek must count as an overlay for the slot it overlaps, and its open/close must schedule ONE re-sync.
function peekPanel(at: DOMRect, open = true) {
	const p = document.createElement("div");
	p.className = "peek-panel left";
	p.setAttribute("role", "complementary");
	if (open) p.dataset.state = "open";
	// The panel is transform-animated: its layout box (what the final rect is made of) ignores the transform.
	Object.defineProperty(p, "offsetLeft", { value: at.left });
	Object.defineProperty(p, "offsetTop", { value: at.top });
	Object.defineProperty(p, "offsetWidth", { value: at.width });
	Object.defineProperty(p, "offsetHeight", { value: at.height });
	p.getBoundingClientRect = () => rect(-9999, at.top, at.width, at.height); // parked off-window mid-slide
	document.body.append(p);
	return p;
}

test("an open edge peek over the slot parks the native view, even mid-slide", () => {
	peekPanel(rect(190, 40, 260, 700));
	expect(overlayOpen(SLOT)).toBe(true);
});

test("a closed edge peek never parks", () => {
	peekPanel(rect(190, 40, 260, 700), false);
	expect(overlayOpen(SLOT)).toBe(false);
});

test("an open peek that does not overlap the slot (slot already beside it) leaves the native view alone", () => {
	peekPanel(rect(0, 40, 150, 700));
	expect(overlayOpen(SLOT)).toBe(false);
});

test("opening and closing a peek schedules exactly one re-sync each, never more", async () => {
	let calls = 0;
	const stop = watchSlot(document.createElement("div"), () => calls++);
	const p = peekPanel(rect(190, 40, 260, 700), false);
	await tick();
	flush();
	calls = 0;
	p.dataset.state = "open";
	await tick();
	flush();
	expect(calls).toBe(1);
	p.dataset.state = "closed";
	await tick();
	flush();
	expect(calls).toBe(2);
	flush();
	expect(calls).toBe(2); // no further syncs without a change
	stop();
});

// Regression (#69): a native view cannot follow the CSS transform of a tab swipe. A slot inside a pane the tab
// pager marked `data-swiping` is covered (parked), and lifting the mark makes `watchSlot` re-sync ONCE, so the view
// is placed at the untransformed rect when the swipe rests.
function swipingPane() {
	const pane = document.createElement("div");
	pane.className = "pane";
	pane.setAttribute("data-swiping", "");
	const slot = document.createElement("div");
	slot.getBoundingClientRect = () => rect(600, 100, 600, 400); // includes the in-flight translate3d
	pane.append(slot);
	document.body.append(pane);
	return { pane, slot };
}

test("a slot inside a swiping pane counts as covered, wherever the transform put it", () => {
	const { slot } = swipingPane();
	expect(overlayOpen(slot.getBoundingClientRect(), slot)).toBe(true);
});

test("a slot in a pane that is not swiping is not covered", () => {
	const { pane, slot } = swipingPane();
	pane.removeAttribute("data-swiping");
	expect(overlayOpen(slot.getBoundingClientRect(), slot)).toBe(false);
});

test("lifting the swipe mark re-syncs the slot once; setting it does too (to park)", async () => {
	let calls = 0;
	const { pane, slot } = swipingPane();
	const stop = watchSlot(slot, () => calls++);
	await tick();
	flush();
	calls = 0;
	pane.removeAttribute("data-swiping");
	await tick();
	flush();
	expect(calls).toBe(1);
	pane.setAttribute("data-swiping", "");
	await tick();
	flush();
	expect(calls).toBe(2);
	stop();
});
