// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, jest, mock, test } from "bun:test";
import { createRawSnippet, flushSync, mount, unmount } from "svelte";

// Regression (#70): two-finger swipe over the hover-peeked (collapsed) sidebar paged TABS instead of projects. The
// peek panel is `inert` while closed, and `rectOf` drops anything inside `[inert]`, so the last zone push had no
// `space` zone; opening the peek only flips a class/attribute (no resize), so nothing re-pushed and the engine kept
// hit-testing the stale list: the point fell in the `tab` zone of the content behind the peek.
// Contract: opening and closing a peek re-pushes the zones, and `space` is in the pushed list only while it is open.

interface Pushed {
	zones: { zone: string }[];
}
const pushes: Pushed[] = [];
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: Pushed) => {
		if (cmd === "swipe_native") return true;
		if (cmd === "swipe_zones") pushes.push(args);
		return null;
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

// Dynamic: the Tauri mocks above must be installed before these modules first import `@tauri-apps/api` (static
// imports are hoisted above `mock.module`).
const { initNativeSwipe, registerZone } = await import("./nativeSwipe.ts");
const { default: EdgePeek } = await import("./EdgePeek.svelte");

const box = (el: HTMLElement, x: number, w: number) => {
	el.getBoundingClientRect = () => ({
		x,
		y: 0,
		left: x,
		top: 0,
		right: x + w,
		bottom: 700,
		width: w,
		height: 700,
	});
};
const zonesOf = () => pushes.at(-1)?.zones.map((z) => z.zone) ?? [];
/** Run the timers (open delay, close grace, 120 ms trailing push) and the promises they resolve. */
const run = async (ms: number) => {
	jest.advanceTimersByTime(ms);
	for (let i = 0; i < 5; i++) await Promise.resolve();
};

let cmp: ReturnType<typeof mount> | undefined;
let regs: { destroy(): void }[] = [];
beforeEach(() => {
	jest.useFakeTimers();
	pushes.length = 0;
	// `initNativeSwipe` only turns native on inside Tauri; rAF runs on the fake clock (16 ms frames).
	window.__TAURI_INTERNALS__ = {};
	globalThis.requestAnimationFrame = (f) => setTimeout(() => f(performance.now()), 16);
	globalThis.cancelAnimationFrame = (id) => clearTimeout(id);
	Object.defineProperty(window, "innerWidth", { value: 1200, configurable: true });
	Object.defineProperty(window, "innerHeight", { value: 800, configurable: true });
});
afterEach(() => {
	for (const r of regs) r.destroy();
	regs = [];
	if (cmp) unmount(cmp);
	cmp = undefined;
	document.body.innerHTML = "";
	jest.useRealTimers();
});

/** The app's shape: the sidebar (`space` zone) lives inside the peek panel, the tab group sits behind it. */
async function app(edge: "left" | "right" = "left") {
	await initNativeSwipe();
	const children = createRawSnippet(() => ({ render: () => `<aside class="sidebar"></aside>` }));
	cmp = mount(EdgePeek, { target: document.body, props: { edge, size: 260, label: "Sidebar", children } });
	flushSync();
	const panel = document.querySelector<HTMLElement>(".peek-panel")!;
	const aside = panel.querySelector<HTMLElement>(".sidebar")!;
	box(aside, edge === "left" ? 0 : 940, 260);
	const tabs = document.createElement("div");
	box(tabs, 0, 1200);
	document.body.append(tabs);
	const spec = { can: () => true, onFrame() {}, onCommit() {} };
	regs.push(
		registerZone(aside, { zone: "space", ...spec }),
		registerZone(tabs, { zone: "tab", id: "g", ...spec }),
	);
	await run(160); // the registration's own trailing push
	return panel;
}

/** What the pointer handler does on dwell: a move into the hot zone (rAF-coalesced), then the open delay. */
async function hover(x: number) {
	window.dispatchEvent(new PointerEvent("pointermove", { clientX: x, buttons: 0 }));
	await run(20); // requestAnimationFrame
	await run(100); // OPEN_DELAY
}

test("a closed (inert) peek has no space zone; the tab zone is all the engine knows", async () => {
	await app();
	expect(zonesOf()).toEqual(["tab"]);
});

test("opening the peek re-pushes the zones and `space` is in the list, innermost", async () => {
	const panel = await app();
	const before = pushes.length;
	await hover(2);
	expect(panel.dataset.state).toBe("open");
	await run(200);
	expect(pushes.length).toBeGreaterThan(before);
	expect(zonesOf()).toEqual(["tab", "space"]);
});

test("closing the peek re-pushes again and the space zone is gone", async () => {
	const panel = await app();
	await hover(2);
	await run(200);
	expect(zonesOf()).toContain("space");
	document.documentElement.dispatchEvent(new Event("pointerleave"));
	await run(300); // CLOSE_DELAY
	expect(panel.dataset.state).toBe("closed");
	await run(200);
	expect(zonesOf()).toEqual(["tab"]);
});

test("the right dock peek re-pushes too", async () => {
	const panel = await app("right");
	const before = pushes.length;
	await hover(1199);
	expect(panel.dataset.state).toBe("open");
	await run(200);
	expect(pushes.length).toBeGreaterThan(before);
	expect(zonesOf()).toContain("space");
});
