// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	HUD_MODES,
	MIN_REGION,
	actionOf,
	cycleMode,
	handleAt,
	initialState,
	labelPlacement,
	loupeOrigin,
	moveRect,
	nudgeRect,
	positionLabel,
	reduce,
	resizeRect,
	resolveCommit,
	selectionAria,
	sizeLabel,
	windowAt,
} from "./overlay.ts";

const primary = { id: 1, frame: { x: 0, y: 0, w: 1000, h: 800 }, scale: 2 };
// A second display to the right of a 1512-wide primary, and 200pt above it (global y is negative).
const second = { id: 2, frame: { x: 1512, y: -200, w: 1000, h: 800 }, scale: 1 };

const win = (id, x, y, w, h, title = `w${id}`) => ({ id, frame: { x, y, w, h }, title });

function boot(display = primary, mode = "region", windows = []) {
	const cursor = { x: display.frame.x + 10, y: display.frame.y + 10 };
	return reduce(initialState(), { type: "init", init: { display, mode, windows, cursor } }).state;
}

// Apply a list of events, return the last step (state + that step's effects) and every effect emitted on the way.
function run(s, events) {
	let step = { state: s, effects: [] };
	const all = [];
	for (const ev of events) {
		step = reduce(step.state, ev);
		all.push(...step.effects);
	}
	return { ...step, all };
}

const down = (x, y, extra = {}) => ({ type: "down", x, y, button: 0, shift: false, alt: false, ...extra });
const move = (x, y, extra = {}) => ({ type: "move", x, y, ...extra });
const up = (x, y) => ({ type: "up", x, y });
const key = (k, extra = {}) => ({ type: "key", key: k, ...extra });

const commitOf = (effects) => effects.find((e) => e.type === "commit");
const enter = key("Enter");
const hudInit = (lastMode = "region", extra = {}) => ({
	lastMode,
	timerSecs: 0,
	hasPrevious: true,
	...extra,
});

// A booted overlay in the All-in-One HUD, opened on `lastMode`.
function bootHud(lastMode = "region", display = primary, windows = [], extra = {}) {
	const cursor = { x: display.frame.x + 10, y: display.frame.y + 10 };
	const init = { display, mode: "all", windows, cursor, hud: hudInit(lastMode, extra) };
	return reduce(initialState(), { type: "init", init }).state;
}

describe("first frame", () => {
	test("the initial state is idle with no display and commits nothing", () => {
		const s = initialState();
		expect(s.drag).toBeNull();
		expect(s.display).toBeNull();
		const r = run(s, [down(10, 10), move(200, 200), up(200, 200)]);
		expect(r.all.filter((e) => e.type === "commit")).toEqual([]);
	});

	test("init stores the display-local cursor and the mode; record and gif select a region", () => {
		const s = boot(second, "record");
		expect(s.pointer).toEqual({ x: 10, y: 10 });
		expect(s.pick).toBe("region");
		expect(boot(second, "gif").pick).toBe("region");
		expect(boot(second, "window").pick).toBe("window");
	});
});

describe("drag", () => {
	test("all four directions give the same rect", () => {
		const want = { x: 100, y: 200, w: 300, h: 150 };
		const corners = [
			[
				[100, 200],
				[400, 350],
			],
			[
				[400, 200],
				[100, 350],
			],
			[
				[100, 350],
				[400, 200],
			],
			[
				[400, 350],
				[100, 200],
			],
		];
		for (const [a, b] of corners) {
			const r = run(boot(), [down(...a), move(...b), up(...b), enter]);
			expect(commitOf(r.all), `from ${a} to ${b}`).toEqual({
				type: "commit",
				payload: { kind: "region", rect: want, display: 1 },
			});
		}
	});

	test("while dragging the state exposes the live rect for the page", () => {
		const r = run(boot(), [down(100, 100), move(250, 300)]);
		expect(r.state.drag).not.toBeNull();
		expect(r.state.rect).toEqual({ x: 100, y: 100, w: 150, h: 200 });
	});

	test("a release below MIN_REGION stays idle, commits nothing and does not cancel", () => {
		expect(MIN_REGION).toBe(4);
		for (const [dx, dy] of [
			[0, 0],
			[3, 3],
			[3.9, 100],
			[100, 3.9],
		]) {
			const r = run(boot(), [down(50, 50), move(50 + dx, 50 + dy), up(50 + dx, 50 + dy)]);
			expect(
				r.all.filter((e) => e.type === "commit" || e.type === "cancel"),
				`${dx}x${dy}`,
			).toEqual([]);
			expect(r.state.drag).toBeNull();
			expect(r.state.rect).toBeNull();
		}
	});

	test("exactly MIN_REGION commits", () => {
		const r = run(boot(), [down(50, 50), up(54, 54), enter]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 50, y: 50, w: 4, h: 4 });
	});

	test("a release without a press is ignored", () => {
		const r = run(boot(), [up(100, 100)]);
		expect(r.all).toEqual([]);
	});

	test("the commit ends the drag and later moves do nothing", () => {
		const r = run(boot(), [down(10, 10), up(100, 100), enter]);
		expect(r.state.drag).toBeNull();
	});
});

describe("clamp to the display", () => {
	const cases = [
		["left", [500, 400], [-300, 500], { x: 0, y: 400, w: 500, h: 100 }],
		["right", [500, 400], [1300, 500], { x: 500, y: 400, w: 500, h: 100 }],
		["top", [500, 400], [600, -90], { x: 500, y: 0, w: 100, h: 400 }],
		["bottom", [500, 400], [600, 2000], { x: 500, y: 400, w: 100, h: 400 }],
	];
	for (const [name, a, b, want] of cases) {
		test(name, () => {
			const r = run(boot(), [down(...a), move(...b), up(...b), enter]);
			expect(commitOf(r.all).payload.rect).toEqual(want);
		});
	}

	test("a press outside the frame is clamped onto it", () => {
		const r = run(boot(), [down(-50, -50), up(100, 100), enter]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 0, y: 0, w: 100, h: 100 });
	});
});

describe("shift = square", () => {
	test("grows on the longer side toward the pointer", () => {
		const r = run(boot(), [down(100, 100), move(300, 160, { shift: true }), up(300, 160), enter]);
		// the shift flag travels on the down and the last move; the up uses the last known modifiers
		expect(commitOf(r.all).payload.rect).toEqual({ x: 100, y: 100, w: 200, h: 200 });
	});

	test("a square drawn up-left keeps the press point fixed", () => {
		const r = run(boot(), [
			down(400, 400, { shift: true }),
			move(300, 380, { shift: true }),
			up(300, 380),
			enter,
		]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 300, y: 300, w: 100, h: 100 });
	});

	test("stays square when the edge clamps ONE axis (Rust: a_square_stays_square_when_the_edge_cuts_one_axis)", () => {
		// press at (900, 100); drag far right/down: x has 100pt of room, y has 700. The side must shrink to 100.
		const r = run(boot(), [
			down(900, 100, { shift: true }),
			move(1500, 700, { shift: true }),
			up(1500, 700),
			enter,
		]);
		const rect = commitOf(r.all).payload.rect;
		expect(rect.w).toBe(rect.h);
		expect(rect).toEqual({ x: 900, y: 100, w: 100, h: 100 });
	});

	test("stays square when the TOP edge clamps one axis", () => {
		const r = run(boot(), [
			down(100, 50, { shift: true }),
			move(600, -400, { shift: true }),
			up(600, -400),
			enter,
		]);
		const rect = commitOf(r.all).payload.rect;
		expect(rect.w).toBe(rect.h);
		expect(rect).toEqual({ x: 100, y: 0, w: 50, h: 50 });
	});

	test("pressing shift mid-drag (a key-less modifier change on move) squares the live rect", () => {
		const r = run(boot(), [down(100, 100), move(300, 160), move(300, 160, { shift: true })]);
		expect(r.state.rect).toEqual({ x: 100, y: 100, w: 200, h: 200 });
	});
});

describe("alt = from the centre", () => {
	test("the press point is the centre and the rect grows out on both sides", () => {
		const r = run(boot(), [
			down(500, 400, { alt: true }),
			move(600, 450, { alt: true }),
			up(600, 450),
			enter,
		]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 400, y: 350, w: 200, h: 100 });
	});

	test("the centred rect is clamped to the display", () => {
		const r = run(boot(), [down(50, 400, { alt: true }), move(200, 450, { alt: true }), up(200, 450), enter]);
		const rect = commitOf(r.all).payload.rect;
		// 150 to the right, only 50 to the left: the symmetric half is capped to what fits on both sides
		expect(rect).toEqual({ x: 0, y: 350, w: 100, h: 100 });
	});

	test("alt + shift is a centred square", () => {
		const r = run(boot(), [
			down(500, 400, { alt: true, shift: true }),
			move(560, 500, { alt: true, shift: true }),
			up(560, 500),
			enter,
		]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 400, y: 300, w: 200, h: 200 });
	});
});

describe("space while dragging moves the rect", () => {
	test("size is kept and the rect follows the pointer", () => {
		let r = run(boot(), [down(100, 100), move(200, 160)]);
		r = run(r.state, [key(" "), move(300, 260)]);
		expect(r.state.rect).toEqual({ x: 200, y: 200, w: 100, h: 60 });
		r = run(r.state, [up(300, 260), enter]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 200, y: 200, w: 100, h: 60 });
	});

	test("it does NOT toggle region/window while dragging", () => {
		const r = run(boot(), [down(100, 100), move(200, 160), key(" ")]);
		expect(r.state.pick).toBe("region");
	});

	test("releasing space goes back to resizing from the (moved) anchor", () => {
		let r = run(boot(), [down(100, 100), move(200, 160), key(" "), move(300, 260)]);
		r = run(r.state, [key(" ", { up: true }), move(400, 360)]);
		// the anchor moved with the rect: (100,100) + (100,100) = (200,200)
		expect(r.state.rect).toEqual({ x: 200, y: 200, w: 200, h: 160 });
	});

	test("moving is clamped so the rect never leaves the display, size unchanged", () => {
		let r = run(boot(), [down(100, 100), move(200, 160), key(" "), move(5000, 5000)]);
		expect(r.state.rect).toEqual({ x: 900, y: 740, w: 100, h: 60 });
		r = run(r.state, [move(-5000, -5000)]);
		expect(r.state.rect).toEqual({ x: 0, y: 0, w: 100, h: 60 });
	});

	test("space before there is a rectangle (below MIN_REGION) is ignored: the drag keeps resizing", () => {
		const r = run(boot(), [down(100, 100), move(102, 102), key(" "), move(300, 300), up(300, 300), enter]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 100, y: 100, w: 200, h: 200 });
	});
});

describe("space when idle toggles the mode", () => {
	test("region -> window -> region", () => {
		let r = run(boot(), [key(" ")]);
		expect(r.state.pick).toBe("window");
		r = run(r.state, [key(" ")]);
		expect(r.state.pick).toBe("region");
	});

	test("a keyup of space does nothing when idle", () => {
		const r = run(boot(), [key(" ", { up: true })]);
		expect(r.state.pick).toBe("region");
	});
});

describe("window mode", () => {
	const windows = [win(7, 100, 100, 400, 300), win(8, 0, 0, 900, 700), win(9, 600, 500, 100, 100)];

	test("hover is the front-most window containing the pointer", () => {
		const s = boot(primary, "window", windows);
		expect(run(s, [move(150, 150)]).state.hover?.id).toBe(7);
		expect(run(s, [move(10, 10)]).state.hover?.id).toBe(8);
		expect(run(s, [move(650, 550)]).state.hover?.id).toBe(8);
	});

	test("windows not under the pointer are ignored and the hover clears", () => {
		const s = boot(primary, "window", windows);
		const r = run(s, [move(150, 150), move(950, 750)]);
		expect(r.state.hover).toBeNull();
	});

	test("a click commits the window id; nothing is committed over empty desktop", () => {
		const s = boot(primary, "window", windows);
		let r = run(s, [move(150, 150), down(150, 150), up(150, 150)]);
		expect(commitOf(r.all)).toEqual({ type: "commit", payload: { kind: "window", windowId: 7 } });
		r = run(s, [move(950, 750), down(950, 750), up(950, 750)]);
		expect(commitOf(r.all)).toBeUndefined();
	});

	test("window mode never starts a drag", () => {
		const s = boot(primary, "window", windows);
		const r = run(s, [down(150, 150), move(300, 300)]);
		expect(r.state.drag).toBeNull();
		expect(r.state.rect).toBeNull();
	});

	test("pressing and releasing on different windows commits the one under the release", () => {
		const s = boot(primary, "window", windows);
		const r = run(s, [move(150, 150), down(150, 150), move(10, 10), up(10, 10)]);
		expect(commitOf(r.all).payload).toEqual({ kind: "window", windowId: 8 });
	});

	test("space toggles from region to window mode and hover then follows the pointer", () => {
		let r = run(boot(primary, "region", windows), [move(150, 150)]);
		expect(r.state.hover).toBeNull();
		r = run(r.state, [key(" ")]);
		expect(r.state.pick).toBe("window");
		expect(r.state.hover?.id).toBe(7);
	});

	test("a `windows` update re-resolves the hover", () => {
		let s = boot(primary, "window", windows);
		s = run(s, [move(150, 150)]).state;
		const r = run(s, [{ type: "windows", windows: [win(42, 120, 120, 100, 100)] }]);
		expect(r.state.hover?.id).toBe(42);
		expect(run(r.state, [{ type: "windows", windows: [] }]).state.hover).toBeNull();
	});

	test("windowAt is front-most-first and edge-exclusive on the right/bottom", () => {
		expect(windowAt(windows, { x: 100, y: 100 })?.id).toBe(7);
		expect(windowAt([win(1, 0, 0, 10, 10)], { x: 10, y: 5 })).toBeNull();
		expect(windowAt([win(1, 0, 0, 10, 10)], { x: 5, y: 10 })).toBeNull();
		expect(windowAt([], { x: 1, y: 1 })).toBeNull();
	});
});

describe("cancel", () => {
	const cancels = (effects) => effects.filter((e) => e.type === "cancel");

	test("Esc cancels when idle, dragging and in window mode", () => {
		expect(cancels(run(boot(), [key("Escape")]).all)).toHaveLength(1);
		expect(cancels(run(boot(), [down(10, 10), move(80, 80), key("Escape")]).all)).toHaveLength(1);
		expect(cancels(run(boot(primary, "window", [win(1, 0, 0, 50, 50)]), [key("Escape")]).all)).toHaveLength(
			1,
		);
	});

	test("the right button cancels when idle, dragging and in window mode, and never starts a drag", () => {
		const rb = { button: 2 };
		expect(cancels(run(boot(), [down(10, 10, rb)]).all)).toHaveLength(1);
		const mid = run(boot(), [down(10, 10), move(80, 80), down(80, 80, rb)]);
		expect(cancels(mid.all)).toHaveLength(1);
		expect(cancels(run(boot(primary, "window", []), [down(10, 10, rb)]).all)).toHaveLength(1);
		expect(run(boot(), [down(10, 10, rb)]).state.drag).toBeNull();
	});

	test("after a cancel nothing further commits", () => {
		const r = run(boot(), [down(10, 10), key("Escape"), up(200, 200)]);
		expect(commitOf(r.all)).toBeUndefined();
	});

	test("the middle button is ignored", () => {
		const r = run(boot(), [down(10, 10, { button: 1 }), up(200, 200)]);
		expect(r.all).toEqual([]);
	});

	test("other keys do nothing", () => {
		expect(run(boot(), [key("a"), key("Enter")]).all).toEqual([]);
	});
});

describe("global points in the commit", () => {
	test("a region on a display whose frame is (1512, -200) is offset by the frame", () => {
		const s = boot(second);
		const r = run(s, [down(100, 50), move(300, 250), up(300, 250), enter]);
		expect(commitOf(r.all)).toEqual({
			type: "commit",
			payload: { kind: "region", rect: { x: 1612, y: -150, w: 200, h: 200 }, display: 2 },
		});
		// the live rect the page draws is still display-local
		expect(r.state.rect).toBeNull();
	});

	test("the local rect stays local while dragging", () => {
		const r = run(boot(second), [down(100, 50), move(300, 250)]);
		expect(r.state.rect).toEqual({ x: 100, y: 50, w: 200, h: 200 });
	});

	test("clamp uses the display size, not global coordinates", () => {
		const r = run(boot(second), [down(900, 700), move(5000, 5000), up(5000, 5000), enter]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 2412, y: 500, w: 100, h: 100 });
	});

	test("a window commit carries only the id", () => {
		const s = boot(second, "window", [win(5, 10, 10, 100, 100)]);
		const r = run(s, [move(20, 20), down(20, 20), up(20, 20)]);
		expect(commitOf(r.all).payload).toEqual({ kind: "window", windowId: 5 });
	});
});

describe("readout", () => {
	test("sizeLabel is pixels (points x scale, rounded)", () => {
		expect(sizeLabel({ x: 0, y: 0, w: 100, h: 50 }, 2)).toBe("200 × 100");
		expect(sizeLabel({ x: 0, y: 0, w: 100.4, h: 49.6 }, 1)).toBe("100 × 50");
		expect(sizeLabel({ x: 0, y: 0, w: 33.25, h: 10 }, 2)).toBe("67 × 20");
	});
});

describe("labelPlacement", () => {
	const bounds = { x: 0, y: 0, w: 1000, h: 800 };
	const label = { w: 80, h: 24 };

	test("sits below the rect, centred horizontally", () => {
		const p = labelPlacement({ x: 400, y: 100, w: 200, h: 100 }, label, bounds);
		expect(p.where).toBe("below");
		expect(p.y).toBeGreaterThanOrEqual(200);
		expect(p.x + label.w / 2).toBe(500);
	});

	test("flips above at the bottom edge", () => {
		const rect = { x: 400, y: 600, w: 200, h: 190 };
		const p = labelPlacement(rect, label, bounds);
		expect(p.where).toBe("above");
		expect(p.y + label.h).toBeLessThanOrEqual(rect.y);
	});

	test("goes inside (bottom-aligned) when neither side fits", () => {
		const rect = { x: 400, y: 10, w: 200, h: 785 };
		const p = labelPlacement(rect, label, bounds);
		expect(p.where).toBe("inside");
		expect(p.y).toBeGreaterThanOrEqual(rect.y);
		expect(p.y + label.h).toBeLessThanOrEqual(rect.y + rect.h);
	});

	test("never leaves the display horizontally", () => {
		const left = labelPlacement({ x: 0, y: 100, w: 20, h: 20 }, label, bounds);
		expect(left.x).toBeGreaterThanOrEqual(0);
		const right = labelPlacement({ x: 980, y: 100, w: 20, h: 20 }, label, bounds);
		expect(right.x + label.w).toBeLessThanOrEqual(1000);
	});
});

describe("loupeOrigin", () => {
	const bounds = { x: 0, y: 0, w: 1000, h: 800 };

	test("sits down-right of the pointer", () => {
		expect(loupeOrigin({ x: 100, y: 100 }, 120, bounds)).toEqual({ x: 124, y: 124 });
	});

	test("flips left at the right edge and up at the bottom edge", () => {
		expect(loupeOrigin({ x: 950, y: 100 }, 120, bounds)).toEqual({ x: 950 - 24 - 120, y: 124 });
		expect(loupeOrigin({ x: 100, y: 780 }, 120, bounds)).toEqual({ x: 124, y: 780 - 24 - 120 });
	});

	test("never leaves the top-left corner", () => {
		const o = loupeOrigin({ x: 5, y: 5 }, 120, { x: 0, y: 0, w: 100, h: 100 });
		expect(o.x).toBeGreaterThanOrEqual(0);
		expect(o.y).toBeGreaterThanOrEqual(0);
	});
});

// ---------------------------------------------------------------------------------------------------------------------
// precision: the selection stays after the release
// ---------------------------------------------------------------------------------------------------------------------

const held = (s, r) => ({ ...s, held: r });
// a 200x100 selection at (300, 200) on the 1000x800 primary display, held
const sel = () => run(boot(), [down(300, 200), move(500, 300), up(500, 300)]).state;
const SEL = { x: 300, y: 200, w: 200, h: 100 };
const B = { w: 1000, h: 800 };

describe("held selection", () => {
	test("releasing a drag keeps the selection and commits nothing", () => {
		const r = run(boot(), [down(300, 200), move(500, 300), up(500, 300)]);
		expect(r.all).toEqual([]);
		expect(r.state.held).toEqual(SEL);
		expect(r.state.drag).toBeNull();
		expect(r.state.done).toBe(false);
	});

	test("a release below MIN_REGION keeps nothing held", () => {
		const r = run(boot(), [down(50, 50), up(52, 52)]);
		expect(r.state.held).toBeNull();
	});

	test("Return commits the held selection in global points; Return with nothing held does nothing", () => {
		const r = run(sel(), [enter]);
		expect(commitOf(r.all)).toEqual({ type: "commit", payload: { kind: "region", rect: SEL, display: 1 } });
		expect(r.state.done).toBe(true);
		expect(run(boot(), [enter]).all).toEqual([]);
	});

	test("Return commits global points on a second display", () => {
		const s = run(boot(second), [down(100, 50), move(300, 250), up(300, 250)]).state;
		expect(commitOf(run(s, [enter]).all).payload.rect).toEqual({ x: 1612, y: -150, w: 200, h: 200 });
	});

	test("Return never commits a selection below MIN_REGION", () => {
		for (const r of [
			{ x: 10, y: 10, w: 3, h: 100 },
			{ x: 10, y: 10, w: 100, h: 3 },
			{ x: 10, y: 10, w: 0, h: 0 },
		]) {
			expect(run(held(boot(), r), [enter]).all, JSON.stringify(r)).toEqual([]);
		}
	});

	test("a double click inside the selection commits it; a double click outside does not", () => {
		expect(commitOf(run(sel(), [down(400, 250, { clicks: 2 })]).all).payload.rect).toEqual(SEL);
		const out = run(sel(), [down(50, 50, { clicks: 2 })]);
		expect(commitOf(out.all)).toBeUndefined();
		expect(out.state.drag).not.toBeNull();
	});

	test("a press outside the selection and its handles starts a new drag and drops the old selection", () => {
		const r = run(sel(), [down(50, 50), move(150, 150), up(150, 150)]);
		expect(r.state.held).toEqual({ x: 50, y: 50, w: 100, h: 100 });
	});

	test("a click without a drag over a window in region mode still commits nothing", () => {
		const s = boot(primary, "region", [win(7, 100, 100, 400, 300)]);
		expect(run(s, [down(150, 150), up(150, 150)]).all).toEqual([]);
	});
});

describe("move the held selection", () => {
	test("dragging inside moves it by the pointer's delta and it stays held", () => {
		const r = run(sel(), [down(400, 250), move(450, 270), up(450, 270)]);
		expect(r.state.held).toEqual({ x: 350, y: 220, w: 200, h: 100 });
		expect(r.state.edit).toBeNull();
		expect(r.all).toEqual([]);
	});

	test("it is clamped to the display on every side and keeps its size", () => {
		const far = (dx, dy) =>
			run(sel(), [down(400, 250), move(400 + dx, 250 + dy), up(400 + dx, 250 + dy)]).state.held;
		expect(far(-9999, 0)).toEqual({ ...SEL, x: 0 });
		expect(far(9999, 0)).toEqual({ ...SEL, x: 800 });
		expect(far(0, -9999)).toEqual({ ...SEL, y: 0 });
		expect(far(0, 9999)).toEqual({ ...SEL, y: 700 });
	});

	test("moveRect is pure", () => {
		expect(moveRect(SEL, 10, -20, B)).toEqual({ x: 310, y: 180, w: 200, h: 100 });
		expect(moveRect(SEL, -999, -999, B)).toEqual({ x: 0, y: 0, w: 200, h: 100 });
	});
});

describe("handleAt", () => {
	const r = { x: 100, y: 100, w: 200, h: 100 };
	const spots = {
		nw: [100, 100],
		n: [200, 100],
		ne: [300, 100],
		e: [300, 150],
		se: [300, 200],
		s: [200, 200],
		sw: [100, 200],
		w: [100, 150],
	};
	for (const [h, [x, y]] of Object.entries(spots)) {
		test(`finds ${h}`, () => expect(handleAt(r, { x, y }, 6)).toBe(h));
	}
	test("reach is inclusive and the middle of the selection is no handle", () => {
		expect(handleAt(r, { x: 106, y: 94 }, 6)).toBe("nw");
		expect(handleAt(r, { x: 107, y: 100 }, 6)).toBeNull();
		expect(handleAt(r, { x: 200, y: 150 }, 6)).toBeNull();
	});
});

describe("resize with each handle", () => {
	const O = { x: 300, y: 200, w: 200, h: 100 };
	const cases = [
		["e", 50, 0, { x: 300, y: 200, w: 250, h: 100 }],
		["w", 50, 0, { x: 350, y: 200, w: 150, h: 100 }],
		["n", 0, -40, { x: 300, y: 160, w: 200, h: 140 }],
		["s", 0, 40, { x: 300, y: 200, w: 200, h: 140 }],
		["se", 20, 30, { x: 300, y: 200, w: 220, h: 130 }],
		["nw", -20, -30, { x: 280, y: 170, w: 220, h: 130 }],
		["ne", 20, -30, { x: 300, y: 170, w: 220, h: 130 }],
		["sw", -20, 30, { x: 280, y: 200, w: 220, h: 130 }],
	];
	for (const [h, dx, dy, want] of cases) {
		test(`${h} moves only its own edges`, () => expect(resizeRect(O, h, dx, dy, B, false)).toEqual(want));
	}

	test("an edge handle never moves the perpendicular axis", () => {
		expect(resizeRect(O, "e", 50, 999, B, false)).toEqual({ x: 300, y: 200, w: 250, h: 100 });
		expect(resizeRect(O, "n", 999, -40, B, false)).toEqual({ x: 300, y: 160, w: 200, h: 140 });
	});

	test("clamps to the display edges", () => {
		expect(resizeRect(O, "e", 9999, 0, B, false)).toEqual({ x: 300, y: 200, w: 700, h: 100 });
		expect(resizeRect(O, "w", -9999, 0, B, false)).toEqual({ x: 0, y: 200, w: 500, h: 100 });
		expect(resizeRect(O, "n", 0, -9999, B, false)).toEqual({ x: 300, y: 0, w: 200, h: 300 });
		expect(resizeRect(O, "s", 0, 9999, B, false)).toEqual({ x: 300, y: 200, w: 200, h: 600 });
		expect(resizeRect(O, "se", 9999, 9999, B, false)).toEqual({ x: 300, y: 200, w: 700, h: 600 });
		expect(resizeRect(O, "nw", -9999, -9999, B, false)).toEqual({ x: 0, y: 0, w: 500, h: 300 });
	});

	test("stops MIN_REGION short of the opposite edge instead of flipping or vanishing", () => {
		expect(resizeRect(O, "e", -9999, 0, B, false)).toEqual({ x: 300, y: 200, w: MIN_REGION, h: 100 });
		expect(resizeRect(O, "w", 9999, 0, B, false)).toEqual({ x: 496, y: 200, w: MIN_REGION, h: 100 });
		expect(resizeRect(O, "s", 0, -9999, B, false)).toEqual({ x: 300, y: 200, w: 200, h: MIN_REGION });
		expect(resizeRect(O, "n", 0, 9999, B, false)).toEqual({ x: 300, y: 296, w: 200, h: MIN_REGION });
		expect(resizeRect(O, "nw", 9999, 9999, B, false)).toEqual({ x: 496, y: 296, w: 4, h: 4 });
	});

	test("through the reducer: press on a handle, drag, release: the selection stays held and resized", () => {
		const r = run(sel(), [down(500, 300), move(550, 360), up(550, 360)]);
		expect(r.state.held).toEqual({ x: 300, y: 200, w: 250, h: 160 });
		expect(r.all).toEqual([]);
	});

	test("a press within reach of a handle resizes instead of moving, even on a tiny selection", () => {
		// (108, 108) is 4pt from the `e` handle at (108, 104): the first handle in toolbar order wins, so it is `e`
		const r = run(held(boot(), { x: 100, y: 100, w: 8, h: 8 }), [
			down(108, 108),
			move(150, 150),
			up(150, 150),
		]);
		expect(r.state.held).toEqual({ x: 100, y: 100, w: 50, h: 8 });
	});
});

describe("shift locks the aspect ratio of the selection when the resize began", () => {
	const O = { x: 300, y: 200, w: 200, h: 100 }; // 2:1
	const ratio = (r) => r.w / r.h;
	const cases = [
		["se", 40, 0, { x: 300, y: 200, w: 240, h: 120 }],
		["se", 0, 60, { x: 300, y: 200, w: 320, h: 160 }],
		["nw", -40, 0, { x: 260, y: 180, w: 240, h: 120 }],
		["ne", 40, 0, { x: 300, y: 180, w: 240, h: 120 }],
		["sw", -40, 0, { x: 260, y: 200, w: 240, h: 120 }],
		["e", 50, 0, { x: 300, y: 187.5, w: 250, h: 125 }],
		["w", -50, 0, { x: 250, y: 187.5, w: 250, h: 125 }],
		["s", 0, 50, { x: 250, y: 200, w: 300, h: 150 }],
		["n", 0, -50, { x: 250, y: 150, w: 300, h: 150 }],
	];
	for (const [h, dx, dy, want] of cases) {
		test(`${h} (${dx}, ${dy})`, () => {
			const got = resizeRect(O, h, dx, dy, B, true);
			expect(got).toEqual(want);
			expect(ratio(got)).toBeCloseTo(2, 9);
		});
	}

	test("it still fits the display: a corner scales down, an edge is capped by the centre line", () => {
		const corner = resizeRect(O, "se", 9999, 9999, B, true);
		expect(corner.x).toBe(300);
		expect(corner.y).toBe(200);
		expect(corner.x + corner.w).toBeLessThanOrEqual(1000 + 1e-9);
		expect(corner.y + corner.h).toBeLessThanOrEqual(800 + 1e-9);
		expect(ratio(corner)).toBeCloseTo(2, 9);
		expect(resizeRect(O, "s", 0, 9999, B, true)).toEqual({ x: 0, y: 200, w: 800, h: 400 });
	});

	test("through the reducer: shift on the press locks the ratio of the selection at that moment", () => {
		const s = sel();
		const r = run(s, [down(500, 300, { shift: true }), move(540, 300, { shift: true }), up(540, 300)]);
		expect(r.state.held).toEqual({ x: 300, y: 200, w: 240, h: 120 });
	});

	test("shift pressed mid-resize locks against the ORIGINAL ratio, not the live one", () => {
		let r = run(sel(), [down(500, 300), move(500, 400)]);
		expect(r.state.held.h).toBe(200);
		r = run(r.state, [move(540, 300, { shift: true })]);
		expect(r.state.held).toEqual({ x: 300, y: 200, w: 240, h: 120 });
		// letting go of shift frees the resize again
		r = run(r.state, [move(540, 300, { shift: false })]);
		expect(r.state.held).toEqual({ x: 300, y: 200, w: 240, h: 100 });
	});

	test("never drops below MIN_REGION with the lock on", () => {
		const r = resizeRect(O, "se", -9999, -9999, B, true);
		expect(r.w).toBeGreaterThanOrEqual(MIN_REGION);
		expect(r.h).toBeGreaterThanOrEqual(MIN_REGION);
	});
});

describe("nudge", () => {
	const R = { x: 100, y: 100, w: 200, h: 100 };
	test("arrows move by 1 px, Shift by 10", () => {
		expect(nudgeRect(R, "ArrowRight", false, false, B)).toEqual({ ...R, x: 101 });
		expect(nudgeRect(R, "ArrowLeft", false, false, B)).toEqual({ ...R, x: 99 });
		expect(nudgeRect(R, "ArrowUp", false, false, B)).toEqual({ ...R, y: 99 });
		expect(nudgeRect(R, "ArrowDown", false, false, B)).toEqual({ ...R, y: 101 });
		expect(nudgeRect(R, "ArrowRight", true, false, B)).toEqual({ ...R, x: 110 });
		expect(nudgeRect(R, "ArrowUp", true, false, B)).toEqual({ ...R, y: 90 });
	});

	test("Alt resizes from the bottom-right edge by 1 px, Shift+Alt by 10; the top-left stays", () => {
		expect(nudgeRect(R, "ArrowRight", false, true, B)).toEqual({ ...R, w: 201 });
		expect(nudgeRect(R, "ArrowLeft", false, true, B)).toEqual({ ...R, w: 199 });
		expect(nudgeRect(R, "ArrowDown", false, true, B)).toEqual({ ...R, h: 101 });
		expect(nudgeRect(R, "ArrowUp", false, true, B)).toEqual({ ...R, h: 99 });
		expect(nudgeRect(R, "ArrowRight", true, true, B)).toEqual({ ...R, w: 210 });
		expect(nudgeRect(R, "ArrowDown", true, true, B)).toEqual({ ...R, h: 110 });
	});

	test("moving is clamped to the display, resizing too", () => {
		expect(nudgeRect({ ...R, x: 0 }, "ArrowLeft", true, false, B)).toEqual({ ...R, x: 0 });
		expect(nudgeRect({ ...R, x: 800 }, "ArrowRight", true, false, B)).toEqual({ ...R, x: 800 });
		expect(nudgeRect({ ...R, y: 0 }, "ArrowUp", true, false, B)).toEqual({ ...R, y: 0 });
		expect(nudgeRect({ ...R, y: 700 }, "ArrowDown", true, false, B)).toEqual({ ...R, y: 700 });
		expect(nudgeRect({ ...R, x: 800 }, "ArrowRight", true, true, B)).toEqual({ ...R, x: 800 });
		expect(nudgeRect({ ...R, y: 700 }, "ArrowDown", true, true, B)).toEqual({ ...R, y: 700 });
	});

	test("resizing stops at MIN_REGION", () => {
		const tiny = { x: 100, y: 100, w: 5, h: 5 };
		expect(nudgeRect(tiny, "ArrowLeft", true, true, B)).toEqual({ x: 100, y: 100, w: 4, h: 5 });
		expect(nudgeRect(tiny, "ArrowUp", true, true, B)).toEqual({ x: 100, y: 100, w: 5, h: 4 });
	});

	test("other keys are not a nudge", () => {
		expect(nudgeRect(R, "a", false, false, B)).toBeNull();
		expect(nudgeRect(R, "Enter", false, false, B)).toBeNull();
	});

	test("through the reducer the held selection follows the keys, only while held", () => {
		const r = run(sel(), [
			key("ArrowRight"),
			key("ArrowDown", { shift: true }),
			key("ArrowRight", { alt: true }),
		]);
		expect(r.state.held).toEqual({ x: 301, y: 210, w: 201, h: 100 });
		expect(r.all).toEqual([]);
		expect(run(boot(), [key("ArrowRight")]).state.held).toBeNull();
	});

	test("a key release is not a nudge", () => {
		expect(run(sel(), [key("ArrowRight", { up: true })]).state.held).toEqual(SEL);
	});

	test("the nudged selection is what Return commits", () => {
		const r = run(sel(), [key("ArrowLeft", { shift: true }), enter]);
		expect(commitOf(r.all).payload.rect).toEqual({ ...SEL, x: 290 });
	});
});

describe("cancel from every phase", () => {
	const cancels = (effects) => effects.filter((e) => e.type === "cancel");
	const phases = {
		idle: () => boot(),
		dragging: () => run(boot(), [down(10, 10), move(80, 80)]).state,
		held: () => sel(),
		moving: () => run(sel(), [down(400, 250), move(420, 260)]).state,
		resizing: () => run(sel(), [down(500, 300), move(520, 320)]).state,
		window: () => boot(primary, "window", [win(1, 0, 0, 50, 50)]),
		hudRegion: () => bootHud("region"),
		hudScreen: () => bootHud("screen"),
	};
	for (const [name, mk] of Object.entries(phases)) {
		test(`Esc cancels while ${name}`, () => {
			const r = run(mk(), [key("Escape")]);
			expect(cancels(r.all)).toHaveLength(1);
			expect(r.state.done).toBe(true);
			expect(r.state.held).toBeNull();
		});
		test(`the right button cancels while ${name}`, () => {
			expect(cancels(run(mk(), [down(10, 10, { button: 2 })]).all)).toHaveLength(1);
		});
	}

	test("after cancelling a held selection Return commits nothing", () => {
		expect(commitOf(run(sel(), [key("Escape"), enter]).all)).toBeUndefined();
	});
});

describe("readout at scale 2", () => {
	test("size and position are in pixels, not points", () => {
		const r = { x: 150, y: 75.4, w: 200.2, h: 100 };
		expect(sizeLabel(r, 2)).toBe("400 × 200");
		expect(positionLabel(r, 2)).toBe("300, 151");
		expect(positionLabel(r, 1)).toBe("150, 75");
	});

	test("the selection's accessible name carries the pixel size", () => {
		expect(selectionAria({ x: 10, y: 20, w: 100, h: 50 }, 2)).toBe("Selection 200 × 100 pixels at 20, 40");
	});

	test("the held selection of the scale-2 primary reads in pixels", () => {
		expect(sizeLabel(sel().held, primary.scale)).toBe("400 × 200");
	});
});

// ---------------------------------------------------------------------------------------------------------------------
// All-in-One HUD
// ---------------------------------------------------------------------------------------------------------------------

describe("HUD modes", () => {
	test("the toolbar order is region, window, screen, record, gif, text", () => {
		expect(HUD_MODES).toEqual(["region", "window", "screen", "record", "gif", "text"]);
	});

	test("actionOf: only record, gif and text change the action", () => {
		expect(HUD_MODES.map(actionOf)).toEqual(["capture", "capture", "capture", "record", "gif", "text"]);
	});

	test("cycleMode wraps in both directions", () => {
		expect(cycleMode("region")).toBe("window");
		expect(cycleMode("text")).toBe("region");
		expect(cycleMode("region", -1)).toBe("text");
		expect(cycleMode("gif", -1)).toBe("record");
	});

	/** The timer button asks Rust for the next value and tells the reducer what came back. */
	test("a timer event updates only the HUD's timer, and does nothing without a HUD", () => {
		const s = bootHud("region", primary, [], { timerSecs: 0, hasPrevious: true });
		const next = reduce(s, { type: "timer", secs: 5 });
		expect(next.state.hud).toEqual({ mode: "region", timerSecs: 5, hasPrevious: true });
		expect(next.effects).toEqual([]);
		const plain = reduce(initialState(), { type: "timer", secs: 3 });
		expect(plain.state.hud).toBeNull();
		// a value Rust would never send is ignored
		expect(reduce(s, { type: "timer", secs: 7 }).state.hud?.timerSecs).toBe(0);
	});

	test("init opens on hud.lastMode, with the timer and previous flag, only when mode is all", () => {
		for (const m of HUD_MODES) {
			const s = bootHud(m, primary, [], { timerSecs: 5, hasPrevious: false });
			expect(s.hud).toEqual({ mode: m, timerSecs: 5, hasPrevious: false });
			expect(s.pick).toBe(m === "window" ? "window" : "region");
		}
		const plain = reduce(initialState(), {
			type: "init",
			init: { display: primary, mode: "region", windows: [], cursor: { x: 1, y: 1 }, hud: hudInit("window") },
		}).state;
		expect(plain.hud).toBeNull();
		expect(plain.pick).toBe("region");
	});

	test("an unknown lastMode falls back to region", () => {
		expect(bootHud("bogus").hud.mode).toBe("region");
	});

	test("keys 1..6 pick the mode and Tab / Shift+Tab cycle it", () => {
		HUD_MODES.forEach((m, i) => expect(run(bootHud("gif"), [key(String(i + 1))]).state.hud.mode).toBe(m));
		expect(run(bootHud("region"), [key("Tab")]).state.hud.mode).toBe("window");
		expect(run(bootHud("region"), [key("Tab", { shift: true })]).state.hud.mode).toBe("text");
		expect(run(bootHud("text"), [key("Tab")]).state.hud.mode).toBe("region");
		expect(run(bootHud("region"), [key("7")]).state.hud.mode).toBe("region");
		expect(run(bootHud("region"), [key("0")]).state.hud.mode).toBe("region");
	});

	test("picking a mode drops the selection and sets the pick; window hover follows", () => {
		const windows = [win(7, 0, 0, 400, 400)];
		let r = run(bootHud("region", primary, windows), [down(100, 100), move(300, 300), up(300, 300)]);
		expect(r.state.held).not.toBeNull();
		r = run(r.state, [{ type: "mode", mode: "window" }]);
		expect(r.state.held).toBeNull();
		expect(r.state.pick).toBe("window");
		expect(r.state.hover?.id).toBe(7);
		expect(run(r.state, [{ type: "mode", mode: "record" }]).state.pick).toBe("region");
	});

	test("digits and Tab do nothing in a plain overlay, and mid-drag in the HUD", () => {
		expect(run(boot(), [key("3"), key("Tab")]).state.pick).toBe("region");
		const r = run(bootHud("region"), [down(10, 10), move(80, 80), key("3")]);
		expect(r.state.hud.mode).toBe("region");
	});

	test("Space does not flip region/window inside the HUD", () => {
		expect(run(bootHud("region"), [key(" ")]).state.pick).toBe("region");
	});

	test("Previous area: an effect when there is one, nothing when there is not", () => {
		const r = run(bootHud("region"), [{ type: "previous" }]);
		expect(r.all).toEqual([{ type: "previous" }]);
		expect(r.state.done).toBe(true);
		expect(run(bootHud("region", primary, [], { hasPrevious: false }), [{ type: "previous" }]).all).toEqual(
			[],
		);
		expect(run(boot(), [{ type: "previous" }]).all).toEqual([]);
	});
});

describe("HUD commit resolution: every (mode, gesture)", () => {
	const rect = { x: 10, y: 20, w: 300, h: 200 };
	const window7 = { id: 7, frame: { x: 100, y: 100, w: 400, h: 300 }, title: "w" };
	const gestures = {
		region: { kind: "region", rect, display: 1 },
		window: { kind: "window", window: window7, display: 1 },
		screen: { kind: "screen", display: 1 },
	};
	const want = {
		"region/region": { kind: "region", rect, display: 1, action: "capture" },
		"region/window": { kind: "window", windowId: 7, action: "capture" },
		"region/screen": { kind: "screen", display: 1, action: "capture" },
		"window/region": { kind: "region", rect, display: 1, action: "capture" },
		"window/window": { kind: "window", windowId: 7, action: "capture" },
		"window/screen": { kind: "screen", display: 1, action: "capture" },
		"screen/region": { kind: "region", rect, display: 1, action: "capture" },
		"screen/window": { kind: "window", windowId: 7, action: "capture" },
		"screen/screen": { kind: "screen", display: 1, action: "capture" },
		"record/region": { kind: "region", rect, display: 1, action: "record" },
		"record/window": { kind: "region", rect: window7.frame, display: 1, action: "record" },
		"record/screen": { kind: "screen", display: 1, action: "record" },
		"gif/region": { kind: "region", rect, display: 1, action: "gif" },
		"gif/window": { kind: "region", rect: window7.frame, display: 1, action: "gif" },
		"gif/screen": { kind: "screen", display: 1, action: "gif" },
		"text/region": { kind: "region", rect, display: 1, action: "text" },
		"text/window": { kind: "region", rect: window7.frame, display: 1, action: "text" },
		"text/screen": { kind: "region", rect: primary.frame, display: 1, action: "text" },
	};
	for (const mode of HUD_MODES) {
		for (const [g, gesture] of Object.entries(gestures)) {
			test(`${mode} + ${g}`, () => {
				expect(resolveCommit(mode, gesture, primary)).toEqual(want[`${mode}/${g}`]);
			});
		}
	}

	test("text of a screen needs the display", () => {
		expect(resolveCommit("text", gestures.screen)).toBeNull();
	});

	test("a plain overlay (no HUD) sends no action and keeps windows as windows", () => {
		expect(resolveCommit(null, gestures.region)).toEqual({ kind: "region", rect, display: 1 });
		expect(resolveCommit(null, gestures.window)).toEqual({ kind: "window", windowId: 7 });
		expect(resolveCommit(null, gestures.screen)).toEqual({ kind: "screen", display: 1 });
	});
});

describe("HUD end to end through the reducer", () => {
	const windows = [win(7, 100, 100, 400, 300)];
	const drag = [down(300, 200), move(500, 300), up(500, 300), enter];
	const regionOf = { x: 300, y: 200, w: 200, h: 100 };

	test("region + drag + Return = region/capture", () => {
		expect(commitOf(run(bootHud("region"), drag).all).payload).toEqual({
			kind: "region",
			rect: regionOf,
			display: 1,
			action: "capture",
		});
	});

	test("record, gif and text + drag = region with that action", () => {
		for (const m of ["record", "gif", "text"]) {
			expect(commitOf(run(bootHud(m), drag).all).payload, m).toEqual({
				kind: "region",
				rect: regionOf,
				display: 1,
				action: m,
			});
		}
	});

	test("window mode: a click commits the window, action capture", () => {
		const r = run(bootHud("window", primary, windows), [move(150, 150), down(150, 150), up(150, 150)]);
		expect(commitOf(r.all).payload).toEqual({ kind: "window", windowId: 7, action: "capture" });
	});

	test("window mode: Return commits the hovered window; empty desktop commits nothing", () => {
		const s = bootHud("window", primary, windows);
		expect(commitOf(run(s, [move(150, 150), enter]).all).payload).toEqual({
			kind: "window",
			windowId: 7,
			action: "capture",
		});
		expect(run(s, [move(950, 750), down(950, 750), up(950, 750), enter]).all).toEqual([]);
	});

	test("window mode on a second display: windows are display-local, ids are global", () => {
		const s = bootHud("window", second, [win(5, 10, 10, 100, 100)]);
		const r = run(s, [move(20, 20), down(20, 20), up(20, 20)]);
		expect(commitOf(r.all).payload).toEqual({ kind: "window", windowId: 5, action: "capture" });
	});

	test("a window click in record/gif/text becomes the window's frame as a region (global points)", () => {
		for (const m of ["record", "gif", "text"]) {
			const r = run(bootHud(m, primary, windows), [move(150, 150), down(150, 150), up(150, 150)]);
			expect(commitOf(r.all).payload, m).toEqual({
				kind: "region",
				rect: { x: 100, y: 100, w: 400, h: 300 },
				display: 1,
				action: m,
			});
		}
		const second2 = bootHud("record", second, [win(5, 10, 10, 100, 100)]);
		const r = run(second2, [down(20, 20), up(20, 20)]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 1522, y: -190, w: 100, h: 100 });
	});

	test("a click over empty desktop in record mode commits nothing, and a drag over a window is a region", () => {
		expect(run(bootHud("record", primary, windows), [down(900, 700), up(900, 700)]).all).toEqual([]);
		const r = run(bootHud("record", primary, windows), [down(150, 150), move(300, 300), up(300, 300), enter]);
		expect(commitOf(r.all).payload.kind).toBe("region");
		expect(commitOf(r.all).payload.rect).toEqual({ x: 150, y: 150, w: 150, h: 150 });
	});

	test("region mode: a click over a window commits nothing", () => {
		expect(run(bootHud("region", primary, windows), [down(150, 150), up(150, 150)]).all).toEqual([]);
	});

	test("screen mode: a click anywhere captures the display under the pointer", () => {
		expect(commitOf(run(bootHud("screen"), [down(400, 400), up(400, 400)]).all).payload).toEqual({
			kind: "screen",
			display: 1,
			action: "capture",
		});
		expect(commitOf(run(bootHud("screen", second), [down(5, 5), up(5, 5)]).all).payload).toEqual({
			kind: "screen",
			display: 2,
			action: "capture",
		});
	});

	test("screen mode never starts a drag, and a release without its press is not a click", () => {
		expect(run(bootHud("screen"), [down(10, 10), move(300, 300)]).state.drag).toBeNull();
		expect(run(bootHud("screen"), [up(10, 10)]).all).toEqual([]);
	});

	test("Return with nothing held: screen = screen/capture, record/gif = screen/action, text = display frame as text", () => {
		expect(commitOf(run(bootHud("screen"), [enter]).all).payload).toEqual({
			kind: "screen",
			display: 1,
			action: "capture",
		});
		expect(commitOf(run(bootHud("record"), [enter]).all).payload).toEqual({
			kind: "screen",
			display: 1,
			action: "record",
		});
		expect(commitOf(run(bootHud("gif", second), [enter]).all).payload).toEqual({
			kind: "screen",
			display: 2,
			action: "gif",
		});
		expect(commitOf(run(bootHud("text", second), [enter]).all).payload).toEqual({
			kind: "region",
			rect: second.frame,
			display: 2,
			action: "text",
		});
		expect(run(bootHud("region"), [enter]).all).toEqual([]);
	});

	test("a plain overlay never sends an action", () => {
		const r = run(boot(), [down(300, 200), move(500, 300), up(500, 300), enter]);
		expect(commitOf(r.all).payload).not.toHaveProperty("action");
		const w = run(boot(primary, "window", windows), [down(150, 150), up(150, 150)]);
		expect(commitOf(w.all).payload).toEqual({ kind: "window", windowId: 7 });
	});

	test("a plain record overlay keeps window clicks inert (pick is region)", () => {
		expect(run(boot(primary, "record", windows), [down(150, 150), up(150, 150)]).all).toEqual([]);
	});
});

describe("capture on release", () => {
	// Real inits always carry `hud`, plain overlays included; `captureOnRelease` rides in it.
	const bootRel = (on, mode = "region", extra = {}, windows = []) => {
		const cursor = { x: 10, y: 10 };
		const hud = { ...hudInit("region"), captureOnRelease: on };
		const init = { display: primary, mode, windows, cursor, hud, ...extra };
		return reduce(initialState(), { type: "init", init }).state;
	};
	const drag = [down(100, 100), move(300, 250), up(300, 250)];
	const want = { x: 100, y: 100, w: 200, h: 150 };

	test("on: releasing a valid drag commits it at once, no Return", () => {
		const r = run(bootRel(true), drag);
		expect(r.all).toEqual([{ type: "commit", payload: { kind: "region", rect: want, display: 1 } }]);
		expect(r.state.done).toBe(true);
		expect(r.state.held).toBeNull();
	});

	test("on: the release point counts as the last move of the drag", () => {
		const r = run(bootRel(true), [down(100, 100), up(300, 250)]);
		expect(commitOf(r.all).payload.rect).toEqual(want);
	});

	test("on: record, gif and text overlays commit on release too, with the HUD's action", () => {
		for (const m of ["record", "gif", "text"]) {
			const hud = { ...hudInit(m), captureOnRelease: true };
			const s = reduce(initialState(), {
				type: "init",
				init: { display: primary, mode: "all", windows: [], cursor: { x: 1, y: 1 }, hud },
			}).state;
			expect(commitOf(run(s, drag).all).payload, m).toEqual({
				kind: "region",
				rect: want,
				display: 1,
				action: m,
			});
		}
		expect(commitOf(run(bootRel(true, "record"), drag).all).payload.rect).toEqual(want);
	});

	test("off: the selection is held and nothing commits until Return", () => {
		const r = run(bootRel(false), drag);
		expect(r.all).toEqual([]);
		expect(r.state.held).toEqual(want);
		expect(r.state.done).toBe(false);
		expect(commitOf(run(r.state, [enter]).all).payload.rect).toEqual(want);
	});

	test("a missing flag (older init) behaves as off: held, Return commits", () => {
		const r = run(boot(), drag);
		expect(r.all).toEqual([]);
		expect(r.state.held).toEqual(want);
	});

	test("on: a drag below MIN_REGION does not commit, cancel or hold", () => {
		for (const [dx, dy] of [
			[0, 0],
			[3, 3],
			[3.9, 100],
			[100, 3.9],
		]) {
			const r = run(bootRel(true), [down(50, 50), move(50 + dx, 50 + dy), up(50 + dx, 50 + dy)]);
			expect(r.all, `${dx}x${dy}`).toEqual([]);
			expect(r.state.held).toBeNull();
			expect(r.state.done).toBe(false);
		}
	});

	test("on: exactly MIN_REGION commits", () => {
		expect(commitOf(run(bootRel(true), [down(50, 50), up(54, 54)]).all).payload.rect).toEqual({
			x: 50,
			y: 50,
			w: 4,
			h: 4,
		});
	});

	test("on: Space-move while dragging still commits the moved rectangle on release", () => {
		const r = run(bootRel(true), [
			down(100, 100),
			move(200, 200),
			key(" "),
			move(250, 230),
			key(" ", { up: true }),
			up(250, 230),
		]);
		expect(commitOf(r.all).payload.rect).toEqual({ x: 150, y: 130, w: 100, h: 100 });
	});

	test("on: moving or resizing a held selection never commits on release", () => {
		// held comes from a pref-off session (or a mode where release does not commit): edits end held
		let s = run(bootRel(false), drag).state;
		s = { ...s, captureOnRelease: true };
		const moved = run(s, [down(150, 150), move(200, 220), up(200, 220)]);
		expect(moved.all).toEqual([]);
		expect(moved.state.held).toEqual({ x: 150, y: 170, w: 200, h: 150 });
		const resized = run(s, [down(300, 250), move(340, 280), up(340, 280)]);
		expect(resized.all).toEqual([]);
		expect(resized.state.held).toEqual({ x: 100, y: 100, w: 240, h: 180 });
	});

	test("on: a click on a window in window mode, and on screen mode, behave as before", () => {
		const windows = [win(7, 100, 100, 400, 300)];
		const w = run(bootRel(true, "window", {}, windows), [move(150, 150), down(150, 150), up(150, 150)]);
		expect(commitOf(w.all).payload).toEqual({ kind: "window", windowId: 7 });
		const hud = { ...hudInit("screen"), captureOnRelease: true };
		const s = reduce(initialState(), {
			type: "init",
			init: { display: primary, mode: "all", windows: [], cursor: { x: 1, y: 1 }, hud },
		}).state;
		expect(commitOf(run(s, [down(5, 5), up(5, 5)]).all).payload.kind).toBe("screen");
	});

	test("on: a click (no drag) over a window in record mode still records that window", () => {
		const windows = [win(7, 100, 100, 400, 300)];
		const hud = { ...hudInit("record"), captureOnRelease: true };
		const s = reduce(initialState(), {
			type: "init",
			init: { display: primary, mode: "all", windows, cursor: { x: 1, y: 1 }, hud },
		}).state;
		expect(commitOf(run(s, [down(150, 150), up(150, 150)]).all).payload.rect).toEqual({
			x: 100,
			y: 100,
			w: 400,
			h: 300,
		});
	});

	test("on: after the commit the overlay is done and later events do nothing", () => {
		const r = run(bootRel(true), [...drag, down(10, 10), up(300, 300), enter]);
		expect(r.all.filter((e) => e.type === "commit")).toHaveLength(1);
	});
});
