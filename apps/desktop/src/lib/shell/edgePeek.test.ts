// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, jest, test } from "bun:test";
import { createRawSnippet, flushSync, mount, unmount } from "svelte";
import EdgePeek from "./EdgePeek.svelte";
import Harness from "../../../test/EdgePeekDockHarness.svelte";
import { docks, peek, toggleDock } from "#lib/stores/docks.svelte.ts";
import { shortcutFor } from "#lib/keymap.svelte.ts";
import { CLOSE_DELAY, FLICK_SPEED, HOT_ZONE, OPEN_DELAY } from "./peek.ts";

// Incident #503 (1.5.0-alpha.26): the right panel opened "sometimes". The real EdgePeek, the real timers (faked), the
// pointer events a window delivers. The window is 1200 wide, the panel 340.
const W = 1200;
const SIZE = 340;

let cmp: ReturnType<typeof mount> | undefined;
const children = createRawSnippet(() => ({ render: () => `<p>panel</p>` }));

/** Run the timers (open delay, close grace) and the frame the pointer handler coalesces into. */
const run = (ms: number) => jest.advanceTimersByTime(ms);
/** One pointer position, as the window reports it; the frame it is handled in runs with it. */
const move = (x: number, init = {}) => {
	window.dispatchEvent(new PointerEvent("pointermove", { clientX: x, buttons: 0, ...init }));
	run(20);
};
/** Come from the middle of the page and rest at `x` (a slow approach: no flick). */
const approach = (x: number) => {
	move(600);
	run(400);
	move(x);
};
const panel = () => document.querySelector<HTMLElement>(".peek-panel")!;
const isOpen = () => panel().dataset.state === "open";
const leaveWindow = () => document.documentElement.dispatchEvent(new Event("pointerleave"));

function mountPeek(active = true) {
	cmp = mount(EdgePeek, {
		target: document.body,
		props: { edge: "right", size: SIZE, label: "R", active, children },
	});
	flushSync();
}

beforeEach(() => {
	jest.useFakeTimers();
	globalThis.requestAnimationFrame = (f) => setTimeout(() => f(performance.now()), 16);
	globalThis.cancelAnimationFrame = (id) => clearTimeout(id);
	Object.defineProperty(window, "innerWidth", { value: W, configurable: true });
	peek.right = false;
	docks.right = false;
});
afterEach(() => {
	if (cmp) unmount(cmp);
	cmp = undefined;
	document.body.innerHTML = "";
	// A fake timer still pending when the real ones come back (the trailing zone push after a close) leaves bun's
	// clock broken for every file that runs after this one: fire what is left first.
	jest.runOnlyPendingTimers();
	jest.useRealTimers();
});

test("a dwell at the edge opens after the open delay, not before", () => {
	mountPeek();
	approach(W - 2);
	run(OPEN_DELAY - 30);
	expect(isOpen()).toBe(false);
	run(60);
	expect(isOpen()).toBe(true);
	expect(peek.right).toBe(true);
});

test("a flick into the edge opens at once", () => {
	mountPeek();
	move(W - 400);
	// 400 px in 20 ms = 20 px/ms, far above FLICK_SPEED.
	run(20);
	move(W - 1);
	expect(FLICK_SPEED).toBeLessThan(20);
	expect(isOpen()).toBe(true);
});

// The pointer at the window's last pixels is over the OS resize border, which the page does not own: WebKit reports it as
// a pointerleave. That used to cancel the dwell (leave-window with the panel closed = cancel), and the region stayed "hot",
// so nothing re-armed it: the pointer was at the edge and the panel never came.
test("pressing against the edge (the page reports a pointerleave there) still opens the panel", () => {
	mountPeek();
	approach(W - 2);
	leaveWindow();
	run(OPEN_DELAY + 40);
	expect(isOpen()).toBe(true);
});

test("leaving the window from the middle of the page does not open it", () => {
	mountPeek();
	approach(W - 2);
	move(600); // the pointer went back inward, then out of the window elsewhere
	leaveWindow();
	run(500);
	expect(isOpen()).toBe(false);
});

// The edge strip of an open panel is inside it (HYSTERESIS), but a pointer resting on the border leaves the page: the
// panel closed under the pointer 250 ms later, and the next move reopened it (flicker).
test("an open panel does not close while the pointer rests on the window border", () => {
	mountPeek();
	approach(W - 2);
	run(OPEN_DELAY + 40);
	expect(isOpen()).toBe(true);
	move(W - 6); // over the panel's strip
	leaveWindow();
	run(CLOSE_DELAY * 3);
	expect(isOpen()).toBe(true);
	move(300); // a real departure still closes it after the grace, not before
	run(CLOSE_DELAY - 50);
	expect(isOpen()).toBe(true);
	run(100);
	expect(isOpen()).toBe(false);
});

test("leaving the window from the panel's far side closes it after the grace", () => {
	mountPeek();
	approach(W - 2);
	run(OPEN_DELAY + 40);
	move(300); // out of the panel, into the page
	leaveWindow(); // and out of the window through the top
	run(CLOSE_DELAY + 50);
	expect(isOpen()).toBe(false);
});

// The 6 px strip is mostly the 8 px window padding plus an OS resize band: a few pixels to hit. A pointer that comes to
// rest 10 px from the edge is "at the edge" for a person.
test("the hot zone is wide enough to hit: 10 px opens it, 24 px does not", () => {
	expect(HOT_ZONE).toBeGreaterThanOrEqual(10);
	mountPeek();
	approach(W - 10);
	run(OPEN_DELAY + 40);
	expect(isOpen()).toBe(true);
	move(300);
	run(CLOSE_DELAY + 50);
	expect(isOpen()).toBe(false);
	approach(W - 24);
	run(OPEN_DELAY + 100);
	expect(isOpen()).toBe(false);
});

test("a press during the dwell cancels the open, and the next move at the edge starts a new dwell", () => {
	mountPeek();
	approach(W - 2);
	run(OPEN_DELAY - 30);
	window.dispatchEvent(new PointerEvent("pointerdown", { clientX: W - 2, buttons: 1 }));
	run(200);
	expect(isOpen()).toBe(false);
	window.dispatchEvent(new PointerEvent("pointerup", { clientX: W - 2, buttons: 0 }));
	move(W - 3);
	run(OPEN_DELAY + 40);
	expect(isOpen()).toBe(true);
});

test("a drag (button held) never opens the panel", () => {
	mountPeek();
	move(600, { buttons: 1 });
	run(400);
	move(W - 2, { buttons: 1 });
	run(500);
	expect(isOpen()).toBe(false);
});

test("a dwell cancelled by a modal re-arms: the pointer still at the edge opens it once the modal is gone", () => {
	mountPeek();
	approach(W - 2);
	document.dispatchEvent(
		new CustomEvent("layerchange", { detail: { layers: [{ kind: "modal" }], count: 1, modal: true } }),
	);
	run(300);
	expect(isOpen()).toBe(false);
	document.dispatchEvent(new CustomEvent("layerchange", { detail: { layers: [], count: 0, modal: false } }));
	move(W - 3); // same region as before the modal; the dwell must start again
	run(OPEN_DELAY + 40);
	expect(isOpen()).toBe(true);
});

test("Escape closes the peek", () => {
	mountPeek();
	approach(W - 2);
	run(OPEN_DELAY + 40);
	window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
	expect(isOpen()).toBe(false);
	expect(peek.right).toBe(false);
});

// The explicit way in (button, ⌘R) is `toggleDock("right")`: pinned = a dock, nothing peeks any more.
test("pinning the right sidebar while it peeks closes the peek; a pinned sidebar never peeks; unpinning re-arms", () => {
	cmp = mount(Harness, { target: document.body });
	flushSync();
	approach(W - 2);
	run(OPEN_DELAY + 40);
	expect(isOpen()).toBe(true);
	toggleDock("right"); // the shortcut and the button
	flushSync();
	expect(docks.right).toBe(true);
	expect(peek.right).toBe(false);
	expect(document.querySelector(".edge-zone")).toBeNull();
	approach(W - 2);
	run(OPEN_DELAY + 200);
	expect(peek.right).toBe(false);
	toggleDock("right"); // unpin: hidden again and armed again
	flushSync();
	expect(docks.right).toBe(false);
	expect(document.querySelector(".edge-zone")).not.toBeNull();
	approach(W - 2);
	run(OPEN_DELAY + 40);
	expect(isOpen()).toBe(true);
});

test("the toggle is labelled with the platform's own shortcut", () => {
	expect(shortcutFor("workspace::ToggleRightDock", true)).toBe("⌘R");
	expect(shortcutFor("workspace::ToggleRightDock", false)).toBe("Ctrl+R");
});
