// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";

// A transcript that grows for hours, a 20k-line diff and a 5k-line build log used to put every row in the DOM
// (DiffView/Transcript/ActionOutputLog). The list keeps the node count flat: only the rows near the viewport exist.
globalThis.ResizeObserver ??= class {
	observe() {}
	unobserve() {}
	disconnect() {}
};
globalThis.requestAnimationFrame = (f) => setTimeout(() => f(0), 0);

const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: Harness } = await import("../../../test/VirtualListHarness.svelte");

let done: (() => void) | undefined;
afterEach(() => done?.());

function setup(props = {}) {
	const target = document.createElement("div");
	document.body.append(target);
	let h;
	const app = flushSync(() => mount(Harness, { target, props: { expose: (x) => (h = x), ...props } }));
	done = () => (unmount(app), target.remove());
	return { target, h };
}
const rows = (t) => t.querySelectorAll(".vl-row").length;

test("20k items render a bounded number of rows", async () => {
	const { target, h } = setup();
	flushSync(() => h.set(20_000));
	await tick();
	expect(rows(target)).toBeGreaterThan(0);
	expect(rows(target)).toBeLessThan(60);
	expect(target.querySelector(".vl-space").style.height).toBe(`${20_000 * 20}px`);
});

test("the node count stays flat when the list grows", async () => {
	const { target, h } = setup();
	flushSync(() => h.set(1_000));
	await tick();
	const small = rows(target);
	flushSync(() => h.set(50_000));
	await tick();
	expect(rows(target)).toBe(small);
});

test("scrolling moves the window and keeps the offset in step with the first rendered row", async () => {
	const { target, h } = setup();
	flushSync(() => h.set(5_000));
	await tick();
	const el = h.el();
	el.scrollTop = 20 * 2_000;
	el.dispatchEvent(new Event("scroll"));
	flushSync();
	await tick();
	const first = target.querySelector(".vl-row .r").textContent;
	const n = Number(first.slice(1));
	expect(n).toBeGreaterThan(1_900);
	expect(n).toBeLessThanOrEqual(2_000);
	expect(target.querySelector(".vl-win").style.transform).toBe(`translateY(${n * 20}px)`);
});

test("stickBottom: appending while at the end keeps the last row rendered", async () => {
	const { target, h } = setup({ stickBottom: true });
	flushSync(() => h.set(100));
	await tick();
	await tick();
	flushSync(() => h.set(200));
	await tick();
	await tick();
	const texts = [...target.querySelectorAll(".vl-row .r")].map((e) => e.textContent);
	expect(texts.at(-1)).toBe("r199");
});

test("a trimmed list drops rows that left instead of leaving them in the DOM", async () => {
	const { target, h } = setup();
	flushSync(() => h.set(10_000));
	await tick();
	flushSync(() => h.set(3));
	await tick();
	expect(rows(target)).toBe(3);
});

// ChatTab used to read `scrollHeight` (a forced layout) once per streamed message to decide whether to keep following.
// The list tracks that from scroll events, so asking costs nothing and appending never reads layout.
test("isFollowing is a flag: reading it and appending never touch scrollHeight", async () => {
	const { target, h } = setup({ stickBottom: true });
	flushSync(() => h.set(10));
	await tick();
	await tick();
	const el = h.el();
	let reads = 0;
	Object.defineProperty(el, "scrollHeight", {
		configurable: true,
		get() {
			reads++;
			return 200;
		},
	});
	expect(h.list().isFollowing()).toBe(true);
	reads = 0;
	h.list().isFollowing();
	expect(reads).toBe(0);
});

test("scrolling up releases follow; scrolling back to the end re-pins it", async () => {
	const { h } = setup({ stickBottom: true });
	flushSync(() => h.set(500));
	await tick();
	await tick();
	const el = h.el();
	Object.defineProperty(el, "clientHeight", { configurable: true, value: 400 });
	Object.defineProperty(el, "scrollHeight", { configurable: true, value: 10_000 });
	el.scrollTop = 9_600; // at the end
	el.dispatchEvent(new Event("scroll"));
	await tick();
	el.scrollTop = 5_000;
	el.dispatchEvent(new Event("scroll"));
	await tick();
	expect(h.list().isFollowing()).toBe(false);
	el.scrollTop = 9_600;
	el.dispatchEvent(new Event("scroll"));
	await tick();
	expect(h.list().isFollowing()).toBe(true);
});
