import { expect, test } from "bun:test";
import { SINK_REST, sinkBox } from "./ime.ts";

const pad = { padding_x: 10, padding_y: 6 };

// The candidate window opens at the focused field's caret: the sink must sit on the cursor's cell, which is padded
// from the slot's corner and counted in cells of the renderer's own cell size.
test("the sink goes on the cursor's cell, inset by the terminal padding", () => {
	expect(sinkBox({ cell_w: 8, cell_h: 17, cursor: { col: 5, row: 3 } }, pad)).toEqual({
		left: 10 + 5 * 8,
		top: 6 + 3 * 17,
		width: 1,
		height: 17,
	});
});

test("the first cell is the padding itself", () => {
	expect(sinkBox({ cell_w: 8, cell_h: 17, cursor: { col: 0, row: 0 } }, pad)).toMatchObject({ left: 10, top: 6 });
});

// Before the first frame the renderer has measured no cell (the stats say 0): placing the sink at NaN or at the
// corner would open the candidates in the wrong place, so it stays where it is.
test("an unknown cell size or cursor leaves the sink alone", () => {
	expect(sinkBox(undefined, pad)).toBeNull();
	expect(sinkBox({ cell_w: 0, cell_h: 0, cursor: { col: 1, row: 1 } }, pad)).toBeNull();
	expect(sinkBox({ cell_w: 8, cell_h: 17 }, pad)).toBeNull();
	expect(sinkBox({ cell_w: Number.NaN, cell_h: 17, cursor: { col: 1, row: 1 } }, pad)).toBeNull();
});

test("the sink at rest has no area (no caret to blink)", () => {
	expect(SINK_REST.width * SINK_REST.height).toBe(0);
});
