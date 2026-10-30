// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { AUTOSCROLL_MS, autoscroll, hoverDecision, selectionClicks } from "./select.ts";

const grid = { top: 100, bottom: 400 };
const row = 20;

// Dragging a selection past the top edge scrolls up (positive = into history) and past the bottom edge
// scrolls down; inside the grid there is nothing to scroll. Farther out is faster, like every terminal.
test("autoscroll: nothing inside the grid, up above it, down below it", () => {
	expect(autoscroll(250, grid.top, grid.bottom, row)).toBe(0);
	expect(autoscroll(grid.top, grid.top, grid.bottom, row)).toBe(0);
	expect(autoscroll(grid.bottom, grid.top, grid.bottom, row)).toBe(0);
	expect(autoscroll(grid.top - 5, grid.top, grid.bottom, row)).toBeGreaterThan(0);
	expect(autoscroll(grid.bottom + 5, grid.top, grid.bottom, row)).toBeLessThan(0);
});

test("autoscroll: a hair outside is slow, far outside is fast, and it is capped", () => {
	const near = autoscroll(grid.bottom + 2, grid.top, grid.bottom, row);
	const mid = autoscroll(grid.bottom + 3 * row, grid.top, grid.bottom, row);
	const far = autoscroll(grid.bottom + 40 * row, grid.top, grid.bottom, row);
	expect(near).toBe(-1);
	expect(Math.abs(mid)).toBeGreaterThan(Math.abs(near));
	expect(Math.abs(far)).toBeGreaterThanOrEqual(Math.abs(mid));
	expect(Math.abs(far)).toBeLessThanOrEqual(12);
	// Symmetric above.
	expect(autoscroll(grid.top - 3 * row, grid.top, grid.bottom, row)).toBe(-mid);
});

test("autoscroll ticks at a steady, sub-frame-pair rate", () => {
	expect(AUTOSCROLL_MS).toBeGreaterThanOrEqual(16);
	expect(AUTOSCROLL_MS).toBeLessThanOrEqual(60);
});

test("autoscroll: a degenerate grid (row height 0) scrolls nothing instead of dividing by zero", () => {
	expect(autoscroll(999, 0, 100, 0)).toBe(0);
});

// What ⌘-hover shows over a link: a URL (or OSC 8) underlines and shows the hand; a path only when the
// app says it is a file of the worktree (it may have to ask the disk, so the answer can be async); a plain
// word, nothing. The Rust side underlines paths only on `paths: true`, so a wrong answer here underlines
// `plain.words` or hides a real file link.
test("hoverDecision: urls always, paths only when the app vouches, nothing otherwise", async () => {
	const never = async () => false;
	const always = () => true;
	expect(await hoverDecision(null, always)).toEqual({ link: false, paths: false });
	expect(await hoverDecision({ kind: "url", target: "https://a.dev" }, never)).toEqual({
		link: true,
		paths: false,
	});
	expect(await hoverDecision({ kind: "path", target: "src/a.ts:2" }, always)).toEqual({
		link: true,
		paths: true,
	});
	expect(await hoverDecision({ kind: "path", target: "src/a.ts:2" }, async () => true)).toEqual({
		link: true,
		paths: true,
	});
	expect(await hoverDecision({ kind: "path", target: "example.com" }, never)).toEqual({
		link: false,
		paths: false,
	});
});

test("hoverDecision: a failing file check is no link, not a crash", async () => {
	const boom = async () => {
		throw new Error("host down");
	};
	expect(await hoverDecision({ kind: "path", target: "src/a.ts" }, boom)).toEqual({
		link: false,
		paths: false,
	});
});

// Click counts: macOS/WebKit's `detail` keeps counting (4, 5…) on rapid clicks; a fourth click is a
// line selection again, not an unknown kind. Shift/alt never start a word or line selection.
test("selectionClicks: caps the count at triple-click", () => {
	expect(selectionClicks(1)).toBe(1);
	expect(selectionClicks(2)).toBe(2);
	expect(selectionClicks(3)).toBe(3);
	expect(selectionClicks(7)).toBe(3);
	expect(selectionClicks(0)).toBe(1);
});
