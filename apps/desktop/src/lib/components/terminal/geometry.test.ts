// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { cellAt, cellFromStats, shellQuote, wheelLines } from "./geometry.ts";

test("cellAt floors to the cell and keeps the fraction for the selection side", () => {
	const hit = cellAt(125, 52, { left: 10, top: 4 }, { w: 10, h: 20 });
	expect(hit.col).toBe(11);
	expect(hit.row).toBe(2);
	expect(hit.frac).toBeCloseTo(0.5);
});

test("cellAt clamps the margin above/left of the origin to cell 0", () => {
	expect(cellAt(0, 0, { left: 10, top: 10 }, { w: 8, h: 16 })).toMatchObject({ col: 0, row: 0 });
});

test("cellFromStats rejects a renderer that has no cell size yet", () => {
	expect(cellFromStats(undefined)).toBeNull();
	expect(cellFromStats({ cell_w: 0, cell_h: 16 })).toBeNull();
	expect(cellFromStats({ cell_w: 8, cell_h: 16 })).toEqual({ w: 8, h: 16 });
});

// Layout zoom: the renderer reports its cell in window points, the pointer arrives in CSS px. At 150% a cell
// of 12 pt is 8 CSS px wide, so a click 80 CSS px in is column 10, not column 6.
test("cellFromStats reports the cell in the page's CSS px under layout zoom", () => {
	expect(cellFromStats({ cell_w: 12, cell_h: 24 }, 1.5)).toEqual({ w: 8, h: 16 });
	expect(cellFromStats({ cell_w: 8, cell_h: 16 })).toEqual({ w: 8, h: 16 });
	const cell = cellFromStats({ cell_w: 12, cell_h: 24 }, 1.5);
	expect(cellAt(80, 0, { left: 0, top: 0 }, cell)).toMatchObject({ col: 10, row: 0 });
});

test("wheelLines scrolls up for negative deltas, falling back to 16px per line", () => {
	expect(wheelLines(-32, null)).toBe(2);
	expect(wheelLines(40, { w: 8, h: 20 })).toBe(-2);
	expect(wheelLines(3, { w: 8, h: 20 }) === 0).toBe(true);
});

test("shellQuote escapes embedded single quotes", () => {
	expect(shellQuote("/a b/it's")).toBe("'/a b/it'\\''s' ");
});
