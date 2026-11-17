// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, jest, mock, test } from "bun:test";

// Incident (#498): a horizontal scroll over an editor with a long line paged the tab group. The engine decides who owns
// a gesture when the fingers land, from what the page last measured: which scrollers sit where and which way each one
// can still go. These tests run the page side of that contract on real elements.

const calls: { cmd: string; args?: { zones: unknown[]; scrollers: Edges[] } }[] = [];
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: unknown) => {
		if (cmd === "swipe_native") return true;
		calls.push({ cmd, args });
		return null;
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
// Dynamic: `mock.module` must be in place before nativeSwipe.ts loads (static imports hoist above it).
const { initNativeSwipe, scrollerRects } = await import("./nativeSwipe.ts");
const { edgesOf, scrollsHorizontally } = await import("./scrollable.ts");

interface Edges {
	left: boolean;
	right: boolean;
}

/** An element with the geometry of a scroller: 400 wide showing 400 of `scrollWidth`, scrolled to `scrollLeft`. */
function scroller(cls: string, { scrollLeft = 0, scrollWidth = 1200 } = {}) {
	const el = document.createElement("div");
	el.className = cls;
	el.style.overflowX = "auto";
	for (const [k, v] of Object.entries({ scrollWidth, clientWidth: 400 }))
		Object.defineProperty(el, k, { value: v, configurable: true });
	el.scrollLeft = scrollLeft;
	el.getBoundingClientRect = () => ({
		x: 10,
		y: 10,
		left: 10,
		top: 10,
		right: 410,
		bottom: 210,
		width: 400,
		height: 200,
	});
	return el;
}

beforeEach(() => {
	calls.length = 0;
	jest.useFakeTimers();
	window.__TAURI_INTERNALS__ = {};
	Object.defineProperty(window, "innerWidth", { value: 1200, configurable: true });
	Object.defineProperty(window, "innerHeight", { value: 800, configurable: true });
});
afterEach(() => {
	document.body.innerHTML = "";
	jest.useRealTimers();
});

test("an editor scroller says which way it can still go: from the left edge only right, mid-line both, at the end only left", () => {
	const at = (scrollLeft) => {
		const el = scroller("cm-scroller", { scrollLeft });
		document.body.append(el);
		return scrollerRects()[0];
	};
	expect(at(0)).toMatchObject({ left: false, right: true });
	document.body.innerHTML = "";
	expect(at(300)).toMatchObject({ left: true, right: true });
	document.body.innerHTML = "";
	expect(at(800)).toMatchObject({ left: true, right: false });
});

test("an editor that wraps its lines has nothing to scroll and is no scroller at all", () => {
	document.body.append(scroller("cm-scroller", { scrollWidth: 400 }));
	expect(scrollerRects()).toEqual([]);
});

test("a terminal or a data-gesture=own surface shows no position: it keeps both directions", () => {
	const term = scroller("xterm");
	term.style.overflowX = "hidden";
	const own = scroller("");
	own.dataset.gesture = "own";
	own.style.overflowX = "hidden";
	document.body.append(term, own);
	expect(scrollerRects().map(({ left, right }) => ({ left, right }))).toEqual([
		{ left: true, right: true },
		{ left: true, right: true },
	]);
});

test("an editor in a tab that is not shown (visibility: hidden, still laid out under the active one) is no scroller", () => {
	const pane = document.createElement("div");
	pane.style.visibility = "hidden";
	const ed = scroller("cm-scroller", { scrollLeft: 300 });
	ed.style.visibility = "hidden"; // what inheritance gives it in a browser
	pane.append(ed);
	document.body.append(pane);
	expect(scrollerRects()).toEqual([]);
});

test("the DOM fallback agrees: an editor consumes a wheel delta only while it can move that way", () => {
	const ed = scroller("cm-scroller", { scrollLeft: 0 });
	document.body.append(ed);
	expect(scrollsHorizontally(ed, document.body, 5)).toBe(true); // right: there is more line
	expect(scrollsHorizontally(ed, document.body, -5)).toBe(false); // already at the left edge: the gesture may take it
	ed.scrollLeft = 800;
	expect(scrollsHorizontally(ed, document.body, 5)).toBe(false);
	expect(
		edgesOf({ scrollLeft: 0, scrollWidth: 800, clientWidth: 400, overflowX: "auto", own: false }),
	).toEqual({
		left: false,
		right: true,
	});
});

const run = async (ms) => {
	jest.advanceTimersByTime(ms);
	for (let i = 0; i < 5; i++) await Promise.resolve();
};
const pushes = () => calls.filter((c) => c.cmd === "swipe_zones");
/** Native tracking is on and the page has told the engine its scrollers (the init may have run in another test file). */
const ready = async () => {
	await initNativeSwipe();
	window.dispatchEvent(new WheelEvent("wheel", { deltaX: 30, deltaY: 1 }));
	await run(500);
};

test("a sideways wheel re-measures the scrollers once it is over, even when no rect moved; a vertical one does not", async () => {
	document.body.append(scroller("cm-scroller", { scrollLeft: 300 }));
	await ready();
	const before = pushes().length;
	expect(pushes().at(-1).args.scrollers[0]).toMatchObject({ left: true, right: true });
	window.dispatchEvent(new WheelEvent("wheel", { deltaX: 2, deltaY: 40 }));
	await run(500);
	expect(pushes().length).toBe(before); // vertical: nothing to learn
	window.dispatchEvent(new WheelEvent("wheel", { deltaX: 30, deltaY: 1 }));
	await run(50);
	expect(pushes().length).toBe(before); // not before the burst is over
	await run(100);
	expect(pushes().length).toBe(before + 1);
});

test("a scrollLeft change is a reason to re-measure, a vertical scroll of the same element is not", async () => {
	const ed = scroller("cm-scroller", { scrollLeft: 0 });
	document.body.append(ed);
	await ready();
	const before = pushes().length;
	ed.scrollLeft = 800;
	ed.dispatchEvent(new Event("scroll"));
	await run(200);
	expect(pushes().length).toBe(before + 1);
	expect(pushes().at(-1).args.scrollers[0]).toMatchObject({ left: true, right: false });
	ed.dispatchEvent(new Event("scroll")); // same scrollLeft: a vertical scroll
	await run(200);
	expect(pushes().length).toBe(before + 1);
});

test("losing focus tells the engine to settle a gesture it still counts as down", async () => {
	await ready();
	window.dispatchEvent(new Event("blur"));
	expect(calls.filter((c) => c.cmd === "swipe_abandon").length).toBe(1);
});
