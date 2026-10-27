// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	COLORS,
	DEFAULT_STYLE,
	WIDTHS,
	TEXT_SIZES,
	begin,
	counterNumbers,
	isHex,
	normalizeHex,
	styleOf,
	boundsOf,
	canRedo,
	canUndo,
	deleteSelected,
	distToSegment,
	drag,
	editText,
	end,
	escape,
	hitTest,
	isDirty,
	markSaved,
	newDoc,
	redo,
	restyle,
	select,
	setCrop,
	setText,
	commitText,
	cancelEdit,
	undo,
} from "./annotate.ts";
import { textBox } from "./render.ts";

const W = 200;
const H = 100;
const fresh = () => newDoc(W, H);

/** One whole gesture: begin, drag, end. */
function gesture(doc, tool, a, b, shift = false, style = DEFAULT_STYLE) {
	return end(drag(begin(doc, tool, a, style), b, shift));
}
const arrow = (doc, a, b, shift) => gesture(doc, "arrow", a, b, shift);
const rect = (doc, a, b, shift) => gesture(doc, "rect", a, b, shift);
const ids = (doc) => doc.items.map((i) => i.id);

describe("tool gestures", () => {
	test("arrow: begin/drag/end creates one arrow with the style, selects it and records one undo step", () => {
		const d = arrow(fresh(), { x: 10, y: 10 }, { x: 80, y: 40 });
		expect(d.items).toHaveLength(1);
		const a = d.items[0];
		expect(a).toMatchObject({
			type: "arrow",
			from: { x: 10, y: 10 },
			to: { x: 80, y: 40 },
			color: DEFAULT_STYLE.color,
			width: DEFAULT_STYLE.width,
		});
		expect(d.selected).toBe(a.id);
		expect(d.past).toHaveLength(1);
		expect(d.gesture).toBeNull();
	});

	test("the item is live while dragging but nothing is recorded until end", () => {
		let d = begin(fresh(), "arrow", { x: 10, y: 10 });
		d = drag(d, { x: 60, y: 10 });
		expect(d.items[0].to).toEqual({ x: 60, y: 10 });
		expect(d.past).toHaveLength(0);
		d = end(d);
		expect(d.past).toHaveLength(1);
	});

	test("rect dragged up-left is normalised to a positive w/h, with no fill", () => {
		const d = rect(fresh(), { x: 90, y: 60 }, { x: 30, y: 20 });
		expect(d.items[0]).toMatchObject({ type: "rect", rect: { x: 30, y: 20, w: 60, h: 40 }, fill: null });
	});

	test("highlight and blur gestures carry their own fields", () => {
		let d = gesture(fresh(), "highlight", { x: 5, y: 5 }, { x: 55, y: 25 }, false, {
			...DEFAULT_STYLE,
			color: "#facc15",
		});
		expect(d.items[0]).toMatchObject({
			type: "highlight",
			rect: { x: 5, y: 5, w: 50, h: 20 },
			color: "#facc15",
		});
		d = gesture(d, "blur", { x: 60, y: 30 }, { x: 100, y: 70 }, false, {
			...DEFAULT_STYLE,
			mode: "blur",
			amount: 9,
		});
		expect(d.items[1]).toMatchObject({
			type: "blur",
			rect: { x: 60, y: 30, w: 40, h: 40 },
			mode: "blur",
			amount: 9,
		});
	});

	test("a plain click (no drag) with a drawing tool adds nothing and records nothing", () => {
		for (const tool of ["arrow", "rect", "highlight", "blur"]) {
			const d = end(begin(fresh(), tool, { x: 50, y: 50 }));
			expect(d.items).toHaveLength(0);
			expect(d.past).toHaveLength(0);
		}
	});

	test("a sub-3px drag is discarded like a click", () => {
		const d = rect(fresh(), { x: 50, y: 50 }, { x: 52, y: 90 });
		expect(d.items).toHaveLength(0);
		expect(d.past).toHaveLength(0);
	});

	test("rects are clamped to the image", () => {
		const d = rect(fresh(), { x: 150, y: 50 }, { x: 900, y: -40 });
		expect(d.items[0].rect).toEqual({ x: 150, y: 0, w: 50, h: 50 });
	});

	test("drag/end without a begin leave the document untouched", () => {
		const d = fresh();
		expect(drag(d, { x: 5, y: 5 })).toBe(d);
		expect(end(d)).toBe(d);
	});
});

describe("shift constraints", () => {
	test("arrow snaps to the nearest 45 degree step and keeps its length", () => {
		// ~11 degrees -> horizontal
		let d = arrow(fresh(), { x: 10, y: 10 }, { x: 50, y: 18 }, true);
		let { from, to } = d.items[0];
		expect(to.y).toBeCloseTo(10, 6);
		expect(to.x - from.x).toBeCloseTo(Math.hypot(40, 8), 6);
		// ~53 degrees -> 45
		d = arrow(fresh(), { x: 10, y: 10 }, { x: 40, y: 50 }, true);
		({ from, to } = d.items[0]);
		expect(to.x - from.x).toBeCloseTo(to.y - from.y, 6);
		expect(to.x - from.x).toBeCloseTo(Math.hypot(30, 40) / Math.SQRT2, 6);
		// up-left diagonal
		d = arrow(fresh(), { x: 100, y: 80 }, { x: 70, y: 52 }, true);
		({ from, to } = d.items[0]);
		expect(from.x - to.x).toBeCloseTo(from.y - to.y, 6);
	});

	test("releasing shift mid-drag frees the arrow again", () => {
		let d = begin(fresh(), "arrow", { x: 10, y: 10 });
		d = drag(d, { x: 50, y: 18 }, true);
		d = drag(d, { x: 50, y: 18 }, false);
		expect(d.items[0].to).toEqual({ x: 50, y: 18 });
	});

	test("rect becomes a square, keeping the drag direction", () => {
		const d = rect(fresh(), { x: 100, y: 50 }, { x: 60, y: 45 }, true);
		const r = d.items[0].rect;
		expect(r.w).toBe(r.h);
		expect(r).toEqual({ x: 60, y: 10, w: 40, h: 40 });
	});

	test("a shifted square never leaves the image: the room on the short side wins", () => {
		const d = rect(fresh(), { x: 180, y: 50 }, { x: 900, y: 90 }, true);
		expect(d.items[0].rect).toEqual({ x: 180, y: 50, w: 20, h: 20 });
	});
});

describe("text", () => {
	test("begin('text') opens an empty editing item; commit with text keeps it as one undo step", () => {
		let d = begin(fresh(), "text", { x: 20, y: 30 }, { ...DEFAULT_STYLE, size: 24 });
		expect(d.editing).toEqual({ id: d.items[0].id, fresh: true });
		expect(d.items[0]).toMatchObject({ type: "text", at: { x: 20, y: 30 }, text: "", size: 24 });
		d = setText(d, "hello");
		d = commitText(d);
		expect(d.editing).toBeNull();
		expect(d.items[0].text).toBe("hello");
		expect(d.past).toHaveLength(1);
		expect(undo(d).items).toHaveLength(0);
	});

	test("committing an empty new text leaves no item and no history", () => {
		let d = begin(fresh(), "text", { x: 20, y: 30 });
		d = commitText(setText(d, "   "));
		expect(d.items).toHaveLength(0);
		expect(d.past).toHaveLength(0);
		expect(d.selected).toBeNull();
	});

	test("cancelEdit on a new text removes it; on an existing text restores the old string", () => {
		let d = commitText(setText(begin(fresh(), "text", { x: 20, y: 30 }), "old"));
		const id = d.items[0].id;
		d = cancelEdit(setText(editText(d, id), "new"));
		expect(d.items[0].text).toBe("old");
		expect(d.past).toHaveLength(1);
		d = cancelEdit(begin(fresh(), "text", { x: 1, y: 1 }));
		expect(d.items).toHaveLength(0);
		expect(d.past).toHaveLength(0);
	});

	test("re-editing without changes records nothing; emptying the text deletes the item as one undo step", () => {
		let d = commitText(setText(begin(fresh(), "text", { x: 20, y: 30 }), "keep"));
		const id = d.items[0].id;
		const same = commitText(editText(d, id));
		expect(same.past).toHaveLength(1);
		const gone = commitText(setText(editText(d, id), ""));
		expect(gone.items).toHaveLength(0);
		expect(undo(gone).items[0].text).toBe("keep");
	});

	test("clicking an existing text with the text tool edits it instead of stacking a second one", () => {
		let d = commitText(setText(begin(fresh(), "text", { x: 20, y: 30 }), "abc"));
		d = begin(d, "text", { x: 22, y: 34 });
		expect(d.items).toHaveLength(1);
		expect(d.editing).toEqual({ id: d.items[0].id, fresh: false });
	});

	test("the text origin is clamped into the image", () => {
		const d = begin(fresh(), "text", { x: 999, y: -5 });
		expect(d.items[0].at).toEqual({ x: W, y: 0 });
	});

	test("starting any other gesture while editing commits the text first", () => {
		let d = setText(begin(fresh(), "text", { x: 20, y: 30 }), "typed");
		d = begin(d, "arrow", { x: 5, y: 5 });
		expect(d.items[0].text).toBe("typed");
		expect(d.editing).toBeNull();
	});
});

describe("select, move, delete", () => {
	const two = () =>
		rect(arrow(fresh(), { x: 10, y: 10 }, { x: 60, y: 10 }), { x: 100, y: 20 }, { x: 150, y: 70 });

	test("select tool picks the hit item, moving it is one undo step", () => {
		let d = select(two(), null);
		d = begin(d, "select", { x: 35, y: 10 });
		expect(d.selected).toBe(d.items[0].id);
		d = drag(d, { x: 45, y: 30 });
		d = end(d);
		expect(d.items[0].from).toEqual({ x: 20, y: 30 });
		expect(d.items[0].to).toEqual({ x: 70, y: 30 });
		expect(d.past).toHaveLength(3);
		d = undo(d);
		expect(d.items[0].from).toEqual({ x: 10, y: 10 });
	});

	test("moving a rect translates it without resizing", () => {
		let d = begin(two(), "select", { x: 100, y: 45 });
		d = end(drag(d, { x: 130, y: 55 }));
		expect(d.items[1].rect).toEqual({ x: 130, y: 30, w: 50, h: 50 });
	});

	test("a click on an item without moving records nothing; a click on empty space deselects", () => {
		const base = two();
		let d = end(begin(base, "select", { x: 35, y: 10 }));
		expect(d.past).toHaveLength(base.past.length);
		d = end(begin(d, "select", { x: 190, y: 95 }));
		expect(d.selected).toBeNull();
	});

	test("undo mid-move cancels the move and restores the position", () => {
		let d = begin(two(), "select", { x: 35, y: 10 });
		d = drag(d, { x: 95, y: 60 });
		d = undo(d);
		expect(d.items[0].from).toEqual({ x: 10, y: 10 });
		expect(d.gesture).toBeNull();
		expect(d.past).toHaveLength(2);
	});

	test("delete removes the selection; undo restores the exact z-order", () => {
		let d = fresh();
		d = rect(d, { x: 10, y: 10 }, { x: 50, y: 50 });
		d = rect(d, { x: 20, y: 20 }, { x: 60, y: 60 });
		d = rect(d, { x: 30, y: 30 }, { x: 70, y: 70 });
		const order = ids(d);
		d = select(d, order[1]);
		d = deleteSelected(d);
		expect(ids(d)).toEqual([order[0], order[2]]);
		expect(d.selected).toBeNull();
		d = undo(d);
		expect(ids(d)).toEqual(order);
		d = redo(d);
		expect(ids(d)).toEqual([order[0], order[2]]);
	});

	test("delete with nothing selected is a no-op that records nothing", () => {
		const d = select(two(), null);
		const r = deleteSelected(d);
		expect(r.past).toHaveLength(d.past.length);
		expect(r.items).toHaveLength(2);
	});

	test("select() ignores ids that do not exist", () => {
		expect(select(two(), 999).selected).toBeNull();
	});

	test("restyle changes colour/width of an item as one undo step", () => {
		let d = two();
		const id = d.items[0].id;
		d = restyle(d, id, { color: "#3b82f6", width: 8 });
		expect(d.items[0]).toMatchObject({ color: "#3b82f6", width: 8 });
		expect(undo(d).items[0].color).toBe(DEFAULT_STYLE.color);
	});
});

describe("undo / redo", () => {
	test("undo then redo round-trips; nothing to undo or redo is a no-op", () => {
		const base = fresh();
		expect(undo(base)).toBe(base);
		expect(redo(base)).toBe(base);
		let d = arrow(base, { x: 1, y: 1 }, { x: 80, y: 80 });
		expect(canUndo(d)).toBe(true);
		d = undo(d);
		expect(d.items).toHaveLength(0);
		expect(canRedo(d)).toBe(true);
		d = redo(d);
		expect(d.items).toHaveLength(1);
		expect(canRedo(d)).toBe(false);
	});

	test("a new edit after undo clears the redo stack", () => {
		let d = arrow(fresh(), { x: 1, y: 1 }, { x: 80, y: 80 });
		d = undo(d);
		d = rect(d, { x: 5, y: 5 }, { x: 50, y: 50 });
		expect(canRedo(d)).toBe(false);
		expect(redo(d)).toBe(d);
	});

	test("undo in the middle of a drag cancels the drag, keeps history and redo untouched", () => {
		let d = arrow(fresh(), { x: 1, y: 1 }, { x: 80, y: 80 });
		d = undo(d); // redo stack now has the arrow
		d = begin(d, "rect", { x: 10, y: 10 });
		d = drag(d, { x: 60, y: 60 });
		expect(d.items).toHaveLength(1);
		d = undo(d);
		expect(d.items).toHaveLength(0);
		expect(d.gesture).toBeNull();
		expect(d.past).toHaveLength(0);
		expect(d.future).toHaveLength(1);
		// the cancelled gesture is gone: end and further drags do nothing
		expect(end(d)).toBe(d);
		expect(drag(d, { x: 90, y: 90 })).toBe(d);
	});

	test("undo/redo restore crop together with the items", () => {
		let d = arrow(fresh(), { x: 1, y: 1 }, { x: 80, y: 80 });
		d = setCrop(d, { x: 10, y: 10, w: 100, h: 50 });
		d = undo(d);
		expect(d.crop).toBeNull();
		expect(d.items).toHaveLength(1);
		d = redo(d);
		expect(d.crop).toEqual({ x: 10, y: 10, w: 100, h: 50 });
	});

	test("the history is bounded", () => {
		let d = fresh();
		for (let i = 0; i < 130; i++) d = rect(d, { x: 0, y: 0 }, { x: 10 + (i % 50), y: 20 });
		expect(d.past.length).toBeLessThanOrEqual(100);
		expect(d.items).toHaveLength(130);
	});

	test("undo drops the selection when the selected item disappears", () => {
		let d = arrow(fresh(), { x: 1, y: 1 }, { x: 80, y: 80 });
		expect(d.selected).not.toBeNull();
		d = undo(d);
		expect(d.selected).toBeNull();
	});
});

describe("crop", () => {
	test("setCrop clamps to the image and rounds to whole pixels", () => {
		expect(setCrop(fresh(), { x: -20, y: 10.4, w: 500, h: 500 }).crop).toEqual({
			x: 0,
			y: 10,
			w: 200,
			h: 90,
		});
	});

	test("setCrop enforces the 8x8 minimum, also at the far edges", () => {
		expect(setCrop(fresh(), { x: 50, y: 50, w: 1, h: 2 }).crop).toEqual({ x: 50, y: 50, w: 8, h: 8 });
		expect(setCrop(fresh(), { x: 199, y: 99, w: 30, h: 30 }).crop).toEqual({ x: 192, y: 92, w: 8, h: 8 });
	});

	test("setCrop accepts a rect dragged backwards (negative w/h)", () => {
		expect(setCrop(fresh(), { x: 100, y: 80, w: -60, h: -50 }).crop).toEqual({ x: 40, y: 30, w: 60, h: 50 });
	});

	test("crop is non-destructive: items and image size are untouched, and null clears it", () => {
		let d = arrow(fresh(), { x: 150, y: 90 }, { x: 190, y: 95 });
		d = setCrop(d, { x: 0, y: 0, w: 50, h: 50 });
		expect(d.items).toHaveLength(1);
		expect(d.width).toBe(W);
		expect(d.height).toBe(H);
		d = setCrop(d, null);
		expect(d.crop).toBeNull();
		expect(undo(d).crop).toEqual({ x: 0, y: 0, w: 50, h: 50 });
	});

	test("setting the same crop again records nothing", () => {
		const a = setCrop(fresh(), { x: 10, y: 10, w: 80, h: 40 });
		expect(setCrop(a, { x: 10, y: 10, w: 80, h: 40 })).toBe(a);
		expect(setCrop(fresh(), null).past).toHaveLength(0);
	});

	test("crop gesture: live while dragging, one undo step, shift makes it square", () => {
		let d = begin(fresh(), "crop", { x: 20, y: 10 });
		d = drag(d, { x: 120, y: 60 });
		expect(d.crop).toEqual({ x: 20, y: 10, w: 100, h: 50 });
		expect(d.past).toHaveLength(0);
		d = end(d);
		expect(d.past).toHaveLength(1);
		d = undo(d);
		expect(d.crop).toBeNull();
		d = end(drag(begin(fresh(), "crop", { x: 20, y: 10 }), { x: 120, y: 60 }, true));
		expect(d.crop.w).toBe(d.crop.h);
	});

	test("a crop gesture smaller than 8px is a click: the previous crop stays", () => {
		const base = setCrop(fresh(), { x: 10, y: 10, w: 80, h: 40 });
		const d = end(drag(begin(base, "crop", { x: 100, y: 50 }), { x: 104, y: 70 }));
		expect(d.crop).toEqual({ x: 10, y: 10, w: 80, h: 40 });
		expect(d.past).toHaveLength(base.past.length);
	});

	test("undo mid crop drag restores the previous crop", () => {
		const base = setCrop(fresh(), { x: 10, y: 10, w: 80, h: 40 });
		let d = drag(begin(base, "crop", { x: 100, y: 50 }), { x: 190, y: 90 });
		expect(d.crop.w).toBe(90);
		d = undo(d);
		expect(d.crop).toEqual({ x: 10, y: 10, w: 80, h: 40 });
	});
});

describe("distToSegment", () => {
	const a = { x: 0, y: 0 };
	const b = { x: 100, y: 0 };
	test("on the segment is 0, beside it is the perpendicular distance", () => {
		expect(distToSegment({ x: 40, y: 0 }, a, b)).toBe(0);
		expect(distToSegment({ x: 40, y: 5 }, a, b)).toBe(5);
	});
	test("beyond an end cap it is the distance to that endpoint, not to the infinite line", () => {
		expect(distToSegment({ x: 105, y: 0 }, a, b)).toBe(5);
		expect(distToSegment({ x: 103, y: 4 }, a, b)).toBe(5);
		expect(distToSegment({ x: -30, y: 40 }, a, b)).toBe(50);
	});
	test("a zero-length segment is the distance to its point (no NaN)", () => {
		expect(distToSegment({ x: 3, y: 4 }, a, a)).toBe(5);
	});
});

describe("hitTest", () => {
	test("an arrow hits within 6px of the segment, not at 7px, and within the end caps", () => {
		const d = arrow(fresh(), { x: 20, y: 50 }, { x: 120, y: 50 });
		const id = d.items[0].id;
		expect(hitTest(d, { x: 70, y: 50 })).toBe(id);
		expect(hitTest(d, { x: 70, y: 56 })).toBe(id);
		expect(hitTest(d, { x: 70, y: 57 })).toBeNull();
		expect(hitTest(d, { x: 125, y: 50 })).toBe(id);
		expect(hitTest(d, { x: 127, y: 50 })).toBeNull();
		expect(hitTest(d, { x: 14, y: 50 })).toBe(id);
		expect(hitTest(d, { x: 12, y: 50 })).toBeNull();
	});

	test("an unfilled rect is hit on its outline, not in its empty middle", () => {
		const d = rect(fresh(), { x: 20, y: 20 }, { x: 120, y: 80 });
		const id = d.items[0].id;
		expect(hitTest(d, { x: 20, y: 50 })).toBe(id);
		expect(hitTest(d, { x: 70, y: 22 })).toBe(id);
		expect(hitTest(d, { x: 70, y: 50 })).toBeNull();
		expect(hitTest(d, { x: 190, y: 5 })).toBeNull();
	});

	test("highlight and blur are hit anywhere inside; text inside its box", () => {
		let d = gesture(fresh(), "highlight", { x: 10, y: 10 }, { x: 60, y: 40 });
		d = gesture(d, "blur", { x: 100, y: 10 }, { x: 150, y: 40 });
		d = commitText(setText(begin(d, "text", { x: 20, y: 60 }, { ...DEFAULT_STYLE, size: 20 }), "hello"));
		const [h, b, t] = ids(d);
		expect(hitTest(d, { x: 30, y: 25 })).toBe(h);
		expect(hitTest(d, { x: 120, y: 25 })).toBe(b);
		const box = textBox("hello", 20);
		expect(hitTest(d, { x: 20 + box.w / 2, y: 60 + box.h / 2 })).toBe(t);
		expect(hitTest(d, { x: 20 + box.w + 20, y: 60 + box.h / 2 })).toBeNull();
	});

	test("overlapping items: the topmost (last) wins, and the one below is reachable where the top does not cover", () => {
		let d = gesture(fresh(), "highlight", { x: 10, y: 10 }, { x: 100, y: 60 });
		d = gesture(d, "blur", { x: 50, y: 10 }, { x: 150, y: 60 });
		const [low, top] = ids(d);
		expect(hitTest(d, { x: 70, y: 30 })).toBe(top);
		expect(hitTest(d, { x: 20, y: 30 })).toBe(low);
	});

	test("a filled rect (highlight fill) is hit in its middle", () => {
		const d = rect(fresh(), { x: 20, y: 20 }, { x: 120, y: 80 });
		const filled = { ...d, items: d.items.map((i) => ({ ...i, fill: "highlight" })) };
		expect(hitTest(filled, { x: 70, y: 50 })).toBe(filled.items[0].id);
	});

	test("an empty document hits nothing", () => {
		expect(hitTest(fresh(), { x: 10, y: 10 })).toBeNull();
	});
});

describe("escape", () => {
	test("cancels the gesture first, then the edit, then deselects, then asks to close", () => {
		let d = begin(arrow(fresh(), { x: 1, y: 1 }, { x: 80, y: 80 }), "rect", { x: 10, y: 10 });
		d = drag(d, { x: 60, y: 60 });
		let r = escape(d);
		expect(r.action).toBe("gesture");
		expect(r.doc.items).toHaveLength(1);
		expect(r.doc.selected).not.toBeNull();
		r = escape(r.doc);
		expect(r.action).toBe("deselect");
		expect(r.doc.selected).toBeNull();
		r = escape(r.doc);
		expect(r.action).toBe("close");
		const editing = begin(fresh(), "text", { x: 5, y: 5 });
		expect(escape(editing).action).toBe("edit");
		expect(escape(editing).doc.items).toHaveLength(0);
	});
});

describe("dirty tracking", () => {
	test("dirty after an edit, clean again after undoing back to the saved state", () => {
		let d = fresh();
		const saved0 = markSaved(d);
		expect(isDirty(d, saved0)).toBe(false);
		d = arrow(d, { x: 1, y: 1 }, { x: 80, y: 80 });
		expect(isDirty(d, saved0)).toBe(true);
		const saved1 = markSaved(d);
		expect(isDirty(d, saved1)).toBe(false);
		d = rect(d, { x: 5, y: 5 }, { x: 50, y: 50 });
		expect(isDirty(d, saved1)).toBe(true);
		expect(isDirty(undo(d), saved1)).toBe(false);
		expect(isDirty(undo(undo(d)), saved1)).toBe(true);
	});

	test("selecting or deselecting is not an edit", () => {
		const d = arrow(fresh(), { x: 1, y: 1 }, { x: 80, y: 80 });
		const saved = markSaved(d);
		expect(isDirty(select(d, null), saved)).toBe(false);
	});
});

describe("boundsOf", () => {
	test("arrow bounds are the normalised segment box; rect-likes their rect; text its measured box", () => {
		const d = arrow(fresh(), { x: 80, y: 60 }, { x: 20, y: 10 });
		expect(boundsOf(d.items[0])).toEqual({ x: 20, y: 10, w: 60, h: 50 });
		const r = rect(fresh(), { x: 5, y: 5 }, { x: 25, y: 45 });
		expect(boundsOf(r.items[0])).toEqual({ x: 5, y: 5, w: 20, h: 40 });
		const t = commitText(
			setText(begin(fresh(), "text", { x: 7, y: 9 }, { ...DEFAULT_STYLE, size: 20 }), "ab"),
		);
		expect(boundsOf(t.items[0])).toEqual({ x: 7, y: 9, ...textBox("ab", 20) });
	});
});

// ---------------------------------------------------------------------------------------------------------------
// Counter, spotlight, marker, text styles, palette (CleanShot parity)
// ---------------------------------------------------------------------------------------------------------------
const click = (doc, tool, p, style = DEFAULT_STYLE) => end(begin(doc, tool, p, style));
const counters = (doc) => doc.items.filter((i) => i.type === "counter");
const nums = (doc) => {
	const m = counterNumbers(doc.items);
	return counters(doc).map((c) => m.get(c.id));
};

describe("counter", () => {
	test("a click places a counter with the style colour and size, selects it and records one undo step", () => {
		const d = click(fresh(), "counter", { x: 40, y: 30 }, { ...DEFAULT_STYLE, color: "#3b82f6", width: 6 });
		expect(d.items).toHaveLength(1);
		expect(d.items[0]).toMatchObject({ type: "counter", at: { x: 40, y: 30 }, color: "#3b82f6", width: 6 });
		expect(d.selected).toBe(d.items[0].id);
		expect(d.past).toHaveLength(1);
		expect(d.gesture).toBeNull();
	});

	test("numbers are 1, 2, 3 in placement order", () => {
		let d = fresh();
		for (const x of [20, 60, 100]) d = click(d, "counter", { x, y: 20 });
		expect(nums(d)).toEqual([1, 2, 3]);
	});

	test("deleting #2 renumbers the following ones; undo brings the old numbers back", () => {
		let d = fresh();
		for (const x of [20, 60, 100]) d = click(d, "counter", { x, y: 20 });
		const [a, b, c] = ids(d);
		d = deleteSelected(select(d, b));
		expect(nums(d)).toEqual([1, 2]);
		expect(counterNumbers(d.items).get(c)).toBe(2);
		expect(counterNumbers(d.items).has(b)).toBe(false);
		d = undo(d);
		expect(nums(d)).toEqual([1, 2, 3]);
		expect(counterNumbers(d.items).get(c)).toBe(3);
		d = redo(d);
		expect(counterNumbers(d.items).get(c)).toBe(2);
		expect(counterNumbers(d.items).get(a)).toBe(1);
	});

	test("other items do not consume numbers; the number follows z-order, not the id", () => {
		let d = click(fresh(), "counter", { x: 20, y: 20 });
		d = rect(d, { x: 50, y: 50 }, { x: 90, y: 90 });
		d = click(d, "counter", { x: 120, y: 20 });
		expect(nums(d)).toEqual([1, 2]);
	});

	test("undo of the placement removes it and the next one is #1 again", () => {
		let d = click(fresh(), "counter", { x: 20, y: 20 });
		d = undo(d);
		d = click(d, "counter", { x: 50, y: 50 });
		expect(nums(d)).toEqual([1]);
	});

	test("a counter is placed inside the image even when clicked outside", () => {
		const d = click(fresh(), "counter", { x: 999, y: -5 });
		expect(d.items[0].at).toEqual({ x: W, y: 0 });
	});

	test("moving a counter is one undo step and does not change its number", () => {
		let d = click(click(fresh(), "counter", { x: 20, y: 20 }), "counter", { x: 100, y: 50 });
		const first = d.items[0].id;
		d = end(drag(begin(d, "select", { x: 20, y: 20 }), { x: 40, y: 40 }));
		expect(d.items[0].at).toEqual({ x: 40, y: 40 });
		expect(counterNumbers(d.items).get(first)).toBe(1);
		expect(undo(d).items[0].at).toEqual({ x: 20, y: 20 });
	});

	test("hit-testing: the disc (radius from the width) is hit, outside it is not; the topmost wins", () => {
		let d = click(fresh(), "counter", { x: 100, y: 50 }, { ...DEFAULT_STYLE, width: 4 });
		const r = boundsOf(d.items[0]);
		expect(r.w).toBe(r.h);
		expect(r.x + r.w / 2).toBe(100);
		expect(hitTest(d, { x: 100, y: 50 })).toBe(d.items[0].id);
		expect(hitTest(d, { x: 100 + r.w / 2 + 1, y: 50 })).toBeNull();
		d = click(d, "counter", { x: 104, y: 50 });
		expect(hitTest(d, { x: 102, y: 50 })).toBe(d.items[1].id);
	});

	test("a bigger stroke width makes a bigger counter", () => {
		const small = click(fresh(), "counter", { x: 50, y: 50 }, { ...DEFAULT_STYLE, width: 2 });
		const big = click(fresh(), "counter", { x: 50, y: 50 }, { ...DEFAULT_STYLE, width: 10 });
		expect(boundsOf(big.items[0]).w).toBeGreaterThan(boundsOf(small.items[0]).w);
	});

	test("restyle recolours and resizes a counter as one undo step", () => {
		let d = click(fresh(), "counter", { x: 50, y: 50 });
		d = restyle(d, d.items[0].id, { color: "#22c55e", width: 10 });
		expect(d.items[0]).toMatchObject({ color: "#22c55e", width: 10 });
		expect(undo(d).items[0]).toMatchObject({ color: DEFAULT_STYLE.color, width: DEFAULT_STYLE.width });
	});
});

describe("spotlight", () => {
	test("dragging creates a rect spotlight by default and an ellipse one when the style says so", () => {
		let d = gesture(fresh(), "spotlight", { x: 20, y: 20 }, { x: 80, y: 60 });
		expect(d.items[0]).toMatchObject({
			type: "spotlight",
			shape: "rect",
			rect: { x: 20, y: 20, w: 60, h: 40 },
		});
		d = gesture(d, "spotlight", { x: 100, y: 20 }, { x: 150, y: 70 }, false, {
			...DEFAULT_STYLE,
			shape: "ellipse",
		});
		expect(d.items[1]).toMatchObject({ type: "spotlight", shape: "ellipse" });
		expect(d.past).toHaveLength(2);
	});

	test("a click or a sub-3px drag adds nothing", () => {
		expect(click(fresh(), "spotlight", { x: 50, y: 50 }).items).toHaveLength(0);
		expect(gesture(fresh(), "spotlight", { x: 50, y: 50 }, { x: 52, y: 90 }).items).toHaveLength(0);
	});

	test("shift makes the spotlight square, and it stays inside the image", () => {
		const d = gesture(fresh(), "spotlight", { x: 150, y: 50 }, { x: 900, y: -40 });
		expect(d.items[0].rect).toEqual({ x: 150, y: 0, w: 50, h: 50 });
		const sq = gesture(fresh(), "spotlight", { x: 10, y: 10 }, { x: 70, y: 30 }, true);
		expect(sq.items[0].rect.w).toBe(sq.items[0].rect.h);
	});

	test("it is hit anywhere inside its rect (it is a region, not an outline) and moves like a rect", () => {
		let d = gesture(fresh(), "spotlight", { x: 20, y: 20 }, { x: 120, y: 80 });
		expect(hitTest(d, { x: 70, y: 50 })).toBe(d.items[0].id);
		expect(hitTest(d, { x: 190, y: 5 })).toBeNull();
		d = end(drag(begin(select(d, null), "select", { x: 70, y: 50 }), { x: 80, y: 60 }));
		expect(d.items[0].rect).toEqual({ x: 30, y: 30, w: 100, h: 60 });
	});

	test("undo/redo and delete work for spotlights", () => {
		let d = gesture(fresh(), "spotlight", { x: 20, y: 20 }, { x: 80, y: 60 });
		d = undo(d);
		expect(d.items).toHaveLength(0);
		d = redo(d);
		expect(d.items).toHaveLength(1);
		// Undo/redo restore the items but not the selection (restore() drops it for an item that was gone), so the
		// person selects the spotlight again, as for any other item, before deleting it.
		d = deleteSelected(select(d, d.items[0].id));
		expect(d.items).toHaveLength(0);
		expect(undo(d).items).toHaveLength(1);
	});
});

describe("marker (freehand highlighter)", () => {
	test("begin/drag/end collects the points, with the style colour and width, as one undo step", () => {
		let d = begin(fresh(), "marker", { x: 10, y: 10 }, { ...DEFAULT_STYLE, color: "#facc15", width: 10 });
		d = drag(d, { x: 30, y: 12 });
		d = drag(d, { x: 60, y: 20 });
		expect(d.past).toHaveLength(0);
		d = end(d);
		expect(d.items[0]).toMatchObject({
			type: "marker",
			color: "#facc15",
			width: 10,
			points: [
				{ x: 10, y: 10 },
				{ x: 30, y: 12 },
				{ x: 60, y: 20 },
			],
		});
		expect(d.past).toHaveLength(1);
		expect(d.selected).toBe(d.items[0].id);
	});

	test("a click with no movement leaves nothing; a very short stroke too", () => {
		expect(click(fresh(), "marker", { x: 50, y: 50 }).items).toHaveLength(0);
		const d = end(drag(begin(fresh(), "marker", { x: 50, y: 50 }), { x: 51, y: 51 }));
		expect(d.items).toHaveLength(0);
		expect(d.past).toHaveLength(0);
	});

	test("points are clamped into the image and consecutive duplicates are dropped", () => {
		let d = begin(fresh(), "marker", { x: 10, y: 10 });
		d = drag(d, { x: 10, y: 10 });
		d = drag(d, { x: 500, y: -50 });
		d = end(d);
		expect(d.items[0].points).toEqual([
			{ x: 10, y: 10 },
			{ x: W, y: 0 },
		]);
	});

	test("undo mid-stroke cancels it", () => {
		let d = drag(begin(fresh(), "marker", { x: 10, y: 10 }), { x: 60, y: 60 });
		d = undo(d);
		expect(d.items).toHaveLength(0);
		expect(d.gesture).toBeNull();
	});

	test("hit-test: near the polyline (half the width plus slack), not away from it; bounds cover the stroke", () => {
		let d = begin(fresh(), "marker", { x: 20, y: 50 }, { ...DEFAULT_STYLE, width: 10 });
		d = end(drag(d, { x: 120, y: 50 }));
		const id = d.items[0].id;
		expect(hitTest(d, { x: 70, y: 50 })).toBe(id);
		expect(hitTest(d, { x: 70, y: 55 })).toBe(id);
		expect(hitTest(d, { x: 70, y: 70 })).toBeNull();
		const b = boundsOf(d.items[0]);
		expect(b.x).toBeLessThanOrEqual(20);
		expect(b.x + b.w).toBeGreaterThanOrEqual(120);
		expect(b.y).toBeLessThanOrEqual(45);
		expect(b.y + b.h).toBeGreaterThanOrEqual(55);
	});

	test("moving translates every point", () => {
		let d = end(drag(begin(fresh(), "marker", { x: 20, y: 50 }), { x: 120, y: 50 }));
		d = end(drag(begin(d, "select", { x: 70, y: 50 }), { x: 80, y: 60 }));
		expect(d.items[0].points).toEqual([
			{ x: 30, y: 60 },
			{ x: 130, y: 60 },
		]);
	});

	test("the existing highlight rect tool still works next to it", () => {
		const d = gesture(fresh(), "highlight", { x: 5, y: 5 }, { x: 55, y: 25 });
		expect(d.items[0]).toMatchObject({ type: "highlight", rect: { x: 5, y: 5, w: 50, h: 20 } });
	});
});

describe("text styles", () => {
	test("a new text takes size, weight and background from the style", () => {
		const d = begin(
			fresh(),
			"text",
			{ x: 20, y: 30 },
			{ ...DEFAULT_STYLE, size: 48, weight: "bold", bg: "pill" },
		);
		expect(d.items[0]).toMatchObject({ type: "text", size: 48, weight: "bold", bg: "pill" });
	});

	test("the default style is regular weight, no background", () => {
		const d = begin(fresh(), "text", { x: 20, y: 30 });
		expect(d.items[0]).toMatchObject({ weight: DEFAULT_STYLE.weight, bg: DEFAULT_STYLE.bg });
		expect(DEFAULT_STYLE.bg).toBe("none");
	});

	test("restyle changes size/weight/bg/colour of a text in one undo step, and only the fields it has", () => {
		let d = commitText(setText(begin(fresh(), "text", { x: 20, y: 30 }), "hi"));
		const id = d.items[0].id;
		d = restyle(d, id, { size: 64, weight: "bold", bg: "solid", color: "#111111" });
		expect(d.items[0]).toMatchObject({ size: 64, weight: "bold", bg: "solid", color: "#111111", text: "hi" });
		expect(d.past).toHaveLength(2);
		expect(undo(d).items[0]).toMatchObject({
			size: DEFAULT_STYLE.size,
			weight: DEFAULT_STYLE.weight,
			bg: "none",
		});
		// a rect has no weight/bg/size: the patch is ignored and nothing is recorded
		let r = rect(fresh(), { x: 5, y: 5 }, { x: 50, y: 50 });
		const before = r.past.length;
		r = restyle(r, r.items[0].id, { weight: "bold", bg: "pill", size: 10 });
		expect(r.past).toHaveLength(before);
	});

	test("editing a styled text in place keeps its style", () => {
		let d = commitText(
			setText(
				begin(fresh(), "text", { x: 20, y: 30 }, { ...DEFAULT_STYLE, weight: "bold", size: 36 }),
				"abc",
			),
		);
		d = commitText(setText(editText(d, d.items[0].id), "abcd"));
		expect(d.items[0]).toMatchObject({ text: "abcd", weight: "bold", size: 36 });
	});

	test("the text box grows with the size preset", () => {
		expect(TEXT_SIZES.map((s) => s.size)).toEqual([...TEXT_SIZES.map((s) => s.size)].sort((a, b) => a - b));
		expect(TEXT_SIZES.map((s) => s.label)).toEqual(["S", "M", "L", "XL"]);
	});
});

describe("palette, widths and hex input", () => {
	test("at least 8 distinct valid colours, the first stays red, widths are 2/4/6/10", () => {
		expect(COLORS.length).toBeGreaterThanOrEqual(8);
		expect(new Set(COLORS).size).toBe(COLORS.length);
		expect(COLORS[0]).toBe("#ef4444");
		for (const c of COLORS) expect(isHex(c)).toBe(true);
		expect(WIDTHS).toEqual([2, 4, 6, 10]);
	});

	test("isHex/normalizeHex accept #rgb and #rrggbb with or without #, in any case, and nothing else", () => {
		expect(normalizeHex("#ABC")).toBe("#aabbcc");
		expect(normalizeHex("3b82f6")).toBe("#3b82f6");
		expect(normalizeHex("  #3B82F6 ")).toBe("#3b82f6");
		for (const bad of [
			"",
			"#",
			"#12",
			"#12345",
			"#1234567",
			"#ggg",
			"red",
			"rgb(1,2,3)",
			"#3b82f6; x",
			"#3b82f6aa",
		])
			expect(normalizeHex(bad), bad).toBeNull();
		expect(isHex("#3b82f6")).toBe(true);
		expect(isHex("#3B82F6")).toBe(true);
		expect(isHex("3b82f6")).toBe(false);
	});

	test("restyle on the selected item applies colour and width; styleOf builds the style new items get", () => {
		let d = arrow(fresh(), { x: 10, y: 10 }, { x: 80, y: 40 });
		d = restyle(d, d.items[0].id, { color: "#a855f7", width: 10 });
		expect(d.items[0]).toMatchObject({ color: "#a855f7", width: 10 });
		const s = styleOf({ color: "#a855f7", width: 10 });
		expect(s).toMatchObject({ ...DEFAULT_STYLE, color: "#a855f7", width: 10 });
		d = arrow(d, { x: 20, y: 20 }, { x: 90, y: 60 });
		expect(d.items[1]).toMatchObject({ color: DEFAULT_STYLE.color });
		d = gesture(d, "arrow", { x: 5, y: 5 }, { x: 60, y: 60 }, false, s);
		expect(d.items.at(-1)).toMatchObject({ color: "#a855f7", width: 10 });
	});
});

describe("history over every new item kind", () => {
	test("each kind: create, undo, redo, delete, undo restores the same z-order", () => {
		let d = click(fresh(), "counter", { x: 20, y: 20 });
		d = gesture(d, "spotlight", { x: 40, y: 40 }, { x: 90, y: 80 });
		d = end(drag(begin(d, "marker", { x: 10, y: 90 }), { x: 100, y: 95 }));
		d = commitText(setText(begin(d, "text", { x: 120, y: 10 }), "t"));
		const order = ids(d);
		expect(d.items.map((i) => i.type)).toEqual(["counter", "spotlight", "marker", "text"]);
		for (let i = 0; i < 4; i++) d = undo(d);
		expect(d.items).toHaveLength(0);
		for (let i = 0; i < 4; i++) d = redo(d);
		expect(ids(d)).toEqual(order);
		d = deleteSelected(select(d, order[1]));
		expect(ids(d)).toEqual([order[0], order[2], order[3]]);
		expect(ids(undo(d))).toEqual(order);
	});

	test("a different item selected in between does not leak the old selection through undo", () => {
		let d = click(fresh(), "counter", { x: 20, y: 20 });
		d = undo(d);
		expect(d.selected).toBeNull();
	});
});
