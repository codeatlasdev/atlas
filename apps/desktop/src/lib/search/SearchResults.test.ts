// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import { stubLayout } from "../../../test/viewport.ts";

// Regression #308: 2000 matches + 200 file headers were 2200 rows in the DOM, each with an inline file icon.
// The list now windows its rows (VirtualList); keyboard navigation is by row index and must keep working on rows
// that are not in the DOM yet.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
globalThis.ResizeObserver ??= class {
	observe() {}
	unobserve() {}
	disconnect() {}
};
// happy-dom does no layout: give the list a 600px viewport like a real panel (row heights stay the estimate).
let restoreLayout;
beforeEach(() => (restoreLayout = stubLayout(600, 22)));
afterEach(() => restoreLayout());
globalThis.requestAnimationFrame = (f) => setTimeout(() => f(0), 0);

const { flushSync, mount, tick, unmount } = await import("svelte");
const { default: SearchResults } = await import("./SearchResults.svelte");

let done;
afterEach(() => done?.());

/** `files` files with `per` matches each: the shape `parse.ts` returns at its caps (200 files, 2000 matches). */
const results = (files = 200, per = 10) =>
	Array.from({ length: files }, (_, f) => ({
		path: `src/pkg${f % 9}/file${f}.ts`,
		matches: Array.from({ length: per }, (_, m) => ({
			line: m + 1,
			start: 2,
			end: 5,
			text: `  const foo${m} = bar(${f});`,
		})),
	}));

function setup(files = results()) {
	const target = document.createElement("div");
	document.body.append(target);
	let left = 0;
	const s = { phase: "done", query: "foo", files, truncated: true };
	const props = { worktreeId: "w", s, onleaveup: () => left++ };
	const api = flushSync(() => mount(SearchResults, { target, props }));
	const app = api;
	done = () => (unmount(app), target.remove());
	const list = target.querySelector('[role="tree"]');
	return { target, list, api, left: () => left };
}
const items = (t) => [...t.querySelectorAll('[role="treeitem"]')];
const key = (list, k, init = {}) => {
	list.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...init }));
	flushSync();
};
const current = (t) => t.querySelector('[role="treeitem"][aria-selected="true"]');

test("2200 result rows keep fewer than 150 in the DOM", async () => {
	const { target } = setup();
	await tick();
	expect(items(target).length).toBeGreaterThan(10);
	expect(items(target).length).toBeLessThan(150);
});

test("arrow-down reaches a row that was not rendered and the list scrolls to it", async () => {
	const { target, list, api } = setup();
	await tick();
	flushSync(() => api.enter());
	for (let i = 0; i < 600; i++) key(list, "ArrowDown");
	await tick();
	await tick();
	const cur = current(target);
	expect(cur).not.toBeNull();
	expect(cur.dataset.row).toBe("600");
	expect(items(target).length).toBeLessThan(150);
});

test("arrow-down stops on the last row, arrow-up from the first row hands focus back to the query", async () => {
	const { target, list, api, left } = setup(results(3, 2));
	await tick();
	flushSync(() => api.enter());
	for (let i = 0; i < 20; i++) key(list, "ArrowDown");
	await tick();
	expect(current(target).dataset.row).toBe("8"); // 3 headers + 6 matches = 9 rows
	for (let i = 0; i < 20; i++) key(list, "ArrowUp");
	expect(left()).toBeGreaterThan(0);
});

test("folding a file removes its matches from the list and the rows below move up", async () => {
	const { target, list, api } = setup(results(3, 2));
	await tick();
	flushSync(() => api.enter());
	expect(items(target)).toHaveLength(9);
	key(list, "ArrowLeft"); // on the first file header: fold it
	await tick();
	expect(items(target)).toHaveLength(7);
	key(list, "ArrowRight");
	await tick();
	expect(items(target)).toHaveLength(9);
});
