// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { watchSlot } from "./slot.ts";

// A slot's native view and its hole follow its layout. A ResizeObserver callback runs after layout and before paint
// of the SAME frame, so syncing right there lands the hole in the frame whose layout moved it (`ground.ts`); the IPC
// behind the sync is the surface's own coalesced chain. Nothing here loops: overlays are DOM above the view and no
// longer move it (hit.ts), so the old per-frame settling window is gone.

type Cb = () => void;
let resizeCb: Cb | undefined;
const realRO = globalThis.ResizeObserver;
beforeEach(() => {
	globalThis.ResizeObserver = class {
		constructor(cb: Cb) {
			resizeCb = cb;
		}
		observe() {}
		unobserve() {}
		disconnect() {}
	};
});
afterEach(() => {
	globalThis.ResizeObserver = realRO;
	document.body.innerHTML = "";
});
const tick = () => new Promise<void>((r) => queueMicrotask(r));

test("a slot resize syncs in the same task: the hole lands in the frame whose layout moved it", () => {
	let calls = 0;
	const stop = watchSlot(document.createElement("div"), () => calls++);
	resizeCb!();
	expect(calls).toBe(1);
	stop();
});

test("a window resize syncs in the same task (zoom and the sidebar glide change layout without resizing the slot)", () => {
	let calls = 0;
	const stop = watchSlot(document.createElement("div"), () => calls++);
	window.dispatchEvent(new Event("resize"));
	expect(calls).toBe(1);
	stop();
});

test("a dialog opening or closing does NOT sync: overlays are DOM above the view and never move it", async () => {
	let calls = 0;
	const stop = watchSlot(document.createElement("div"), () => calls++);
	const dlg = document.createElement("div");
	dlg.setAttribute("role", "dialog");
	dlg.dataset.state = "open";
	document.body.append(dlg);
	await tick();
	dlg.dataset.state = "closed";
	await tick();
	expect(calls).toBe(0);
	stop();
});

test("teardown stops syncing: the observers are disconnected and the window listener is gone", async () => {
	let calls = 0;
	let disconnected = 0;
	globalThis.ResizeObserver = class {
		observe() {}
		disconnect() {
			disconnected++;
		}
	};
	const { pane, slot } = swipingPane(false);
	const stop = watchSlot(slot, () => calls++);
	stop();
	window.dispatchEvent(new Event("resize"));
	pane.setAttribute("data-swiping", "");
	await tick();
	expect(disconnected).toBe(1);
	expect(calls).toBe(0);
});

// Regression (#69): a native view cannot follow the CSS transform of a tab swipe. A slot inside a pane the tab pager
// marked `data-swiping` is parked (the surface reads the mark), and flipping the mark re-syncs ONCE each way.
function swipingPane(on = true) {
	const pane = document.createElement("div");
	if (on) pane.setAttribute("data-swiping", "");
	const slot = document.createElement("div");
	pane.append(slot);
	document.body.append(pane);
	return { pane, slot };
}

test("setting the swipe mark and lifting it each re-sync the slot once", async () => {
	let calls = 0;
	const { pane, slot } = swipingPane(false);
	const stop = watchSlot(slot, () => calls++);
	pane.setAttribute("data-swiping", "");
	await tick();
	expect(calls).toBe(1); // to park
	pane.removeAttribute("data-swiping");
	await tick();
	expect(calls).toBe(2); // back at the untransformed rect
	stop();
});

test("other attribute or DOM churn in the page does not sync the slot", async () => {
	let calls = 0;
	const { slot } = swipingPane(false);
	const stop = watchSlot(slot, () => calls++);
	const other = document.createElement("div");
	document.body.append(other);
	other.setAttribute("class", "x");
	other.dataset.state = "open";
	await tick();
	expect(calls).toBe(0);
	stop();
});
