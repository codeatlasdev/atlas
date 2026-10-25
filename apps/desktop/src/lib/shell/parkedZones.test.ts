// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, jest, mock, test } from "bun:test";

// Regression (#121): a parked pager layer used to be `inert`, and `rectOf` dropped every swipe zone inside `[inert]`. Without
// `inert` (it restyled the whole layer per role flip) a parked layer that `warm` brings into view for one snapshot frame, or a
// neighbour mid-slide, would register its tab strip and scrollers as live zones next to the current layer's. The role
// (`data-role="near"`) is what keeps them out now.

const pushes: { zones: { zone: string; id?: string }[]; scrollers: unknown[] }[] = [];
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: (typeof pushes)[number]) => {
		if (cmd === "swipe_native") return true;
		if (cmd === "swipe_zones") pushes.push(args);
		return null;
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
const { initNativeSwipe, registerZone } = await import("./nativeSwipe.ts");

const inView = (el: HTMLElement) => {
	el.getBoundingClientRect = () => ({
		x: 0,
		y: 0,
		left: 0,
		top: 0,
		right: 600,
		bottom: 400,
		width: 600,
		height: 400,
	});
};
const run = async (ms: number) => {
	jest.advanceTimersByTime(ms);
	for (let i = 0; i < 5; i++) await Promise.resolve();
};
const ids = () => pushes.at(-1)?.zones.map((z) => z.id) ?? [];

let regs: { destroy(): void }[] = [];
beforeEach(() => {
	jest.useFakeTimers();
	pushes.length = 0;
	window.__TAURI_INTERNALS__ = {};
	globalThis.requestAnimationFrame = (f) => setTimeout(() => f(performance.now()), 16);
	globalThis.cancelAnimationFrame = (id) => clearTimeout(id);
	Object.defineProperty(window, "innerWidth", { value: 1200, configurable: true });
	Object.defineProperty(window, "innerHeight", { value: 800, configurable: true });
});
afterEach(() => {
	for (const r of regs) r.destroy();
	regs = [];
	document.body.innerHTML = "";
	jest.useRealTimers();
});

function layer(role: "current" | "near", id: string) {
	const l = document.createElement("div");
	l.className = "space-layer";
	l.dataset.role = role;
	const strip = document.createElement("div");
	inView(strip);
	l.append(strip);
	document.body.append(l);
	regs.push(registerZone(strip, { zone: "tab", id, can: () => true, onFrame() {}, onCommit() {} }));
	return l;
}

test("a tab zone inside a parked layer is not pushed, even when the layer is in view; the current layer's is", async () => {
	await initNativeSwipe();
	layer("current", "cur");
	layer("near", "parked");
	await run(200);
	expect(ids()).toEqual(["cur"]);
});

test("a layer that becomes current registers its zones with no change to its subtree", async () => {
	await initNativeSwipe();
	layer("current", "a");
	const b = layer("near", "b");
	await run(200);
	expect(ids()).toEqual(["a"]);
	b.dataset.role = "current";
	window.dispatchEvent(new Event("resize"));
	await run(300);
	expect(ids()).toContain("b");
});

// Incident (#121): after a switch the trailing zone push ran `scrollerRects()`, which visited every element of `main.content`
// in ALL layers (218k with five 400-line diffs: a 480 ms task) to read `scrollWidth` and `getComputedStyle`, and only then
// dropped the ones in parked layers. A parked layer can never contribute a rect, so the scan must not touch it.
test("scrollerRects does not measure elements of a parked layer (it would cost O(every layer) per switch)", async () => {
	const { scrollerRects } = await import("./nativeSwipe.ts");
	// the app's nesting: main.content > .space-view > .pager > layers > body > content (not layers directly under main)
	const main = document.createElement("main");
	main.className = "content";
	const pager = document.createElement("div");
	pager.className = "space-view";
	main.append(pager);
	document.body.append(main);
	const reads: string[] = [];
	const mk = (role: "current" | "near", n: number) => {
		const l = document.createElement("div");
		l.className = "space-layer";
		l.dataset.role = role;
		const body = document.createElement("div");
		body.className = "space-body";
		l.append(body);
		for (let i = 0; i < n; i++) {
			const e = document.createElement("div");
			Object.defineProperty(e, "scrollWidth", {
				get() {
					reads.push(role);
					return 0;
				},
			});
			const wrap = document.createElement("section");
			wrap.append(e);
			body.append(wrap);
		}
		pager.append(l);
	};
	mk("current", 3);
	mk("near", 50);
	mk("near", 50);
	scrollerRects();
	expect(reads.filter((r) => r === "near")).toEqual([]);
	expect(reads.filter((r) => r === "current").length).toBe(3);
});
