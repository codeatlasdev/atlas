// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { createGesture } from "./gesture.ts";
import { canScrollX, consumedBy } from "./scrollable.ts";
import {
	CLOSE_DELAY,
	FLICK_SPEED,
	HOT_ZONE,
	HYSTERESIS,
	OPEN_DELAY,
	inPanel,
	peekAction,
	regionOf,
	sameRegion,
} from "./peek.ts";

const W = 260;

/** Fingers moving left (next): macOS natural scrolling reports a positive deltaX. */
function swipe(g, from, steps, dx, dt = 8, dy = 0) {
	let t = from;
	let v = null;
	for (let i = 0; i < steps; i++) {
		t += dt;
		v = g.feed(dx, dy, t, W) ?? v;
	}
	return { t, v };
}

test("accumulates deltas into an offset that follows the finger, then commits past 30%", () => {
	const g = createGesture();
	const { t, v } = swipe(g, 1000, 12, 8); // 96px > 0.3 * 260 = 78
	expect(v.dir).toBe(1);
	expect(v.offset).toBe(-96); // fingers left → content left
	expect(v.progress).toBe(1);
	expect(g.poll(t + 50)).toBeNull(); // still inside the end-idle window
	expect(g.poll(t + 130)).toEqual({ kind: "commit", dir: 1 });
});

test("a short slow drag springs back", () => {
	const g = createGesture();
	const { t } = swipe(g, 1000, 5, 6, 30); // 30px at 0.2px/ms
	expect(g.poll(t + 130)).toEqual({ kind: "cancel" });
});

test("a short fast flick commits on velocity", () => {
	const g = createGesture();
	const { t } = swipe(g, 1000, 4, 12, 8); // 48px (18%) at 1.5px/ms
	expect(g.poll(t + 130)).toEqual({ kind: "commit", dir: 1 });
});

test("axis lock: a mostly vertical start belongs to scrolling and never tracks", () => {
	const g = createGesture();
	const { t, v } = swipe(g, 1000, 6, 2, 8, 12);
	expect(v).toBeNull();
	expect(g.active).toBe(false);
	expect(g.poll(t + 130)).toBeNull();
});

test("tiny jitter under the lock distance is not a gesture", () => {
	const g = createGesture();
	expect(g.feed(2, 0, 1000, W)).toBeNull();
	expect(g.feed(2, 0, 1008, W)).toBeNull();
});

test("momentum tail after a commit is ignored until the fingers are really quiet", () => {
	const g = createGesture();
	const first = swipe(g, 1000, 12, 8);
	expect(g.poll(first.t + 130)?.kind).toBe("commit");
	// Inertia keeps coming at ~16ms: within endIdle of the last event, so it must not start a new gesture.
	const tailStart = first.t + 16;
	for (let i = 0; i < 20; i++) expect(g.feed(6, 0, tailStart + i * 16, W)).toBeNull();
	expect(g.active).toBe(false);
	// A real new swipe after a pause works again.
	const later = swipe(g, tailStart + 20 * 16 + 400, 12, 8);
	expect(g.poll(later.t + 130)?.kind).toBe("commit");
});

test("direction can be refused: rubber band, no progress, cancel on release", () => {
	const g = createGesture({ can: (d) => d !== -1 });
	// Fingers right (negative DOM deltaX) = previous, which is not allowed.
	let t = 1000;
	let v = null;
	for (let i = 0; i < 12; i++) v = g.feed(-8, 0, (t += 8), W) ?? v;
	expect(v.dir).toBe(-1);
	expect(v.progress).toBe(0);
	expect(Math.abs(v.offset)).toBeLessThan(96 * 0.4); // resisted
	expect(g.poll(t + 130)).toEqual({ kind: "cancel" });
});

test("previous: fingers right commits dir -1", () => {
	const g = createGesture();
	const { t } = swipe(g, 1000, 12, -8);
	expect(g.poll(t + 130)).toEqual({ kind: "commit", dir: -1 });
});

test("reversing mid-drag past zero flips the direction and releases as a cancel", () => {
	const g = createGesture();
	let { t } = swipe(g, 1000, 6, 8); // 48px next
	({ t } = swipe(g, t, 6, -8)); // back to 0
	expect(g.poll(t + 130)).toEqual({ kind: "cancel" });
});

test("non-finite deltas are dropped", () => {
	const g = createGesture();
	expect(g.feed(NaN, 0, 1000, W)).toBeNull();
	expect(g.active).toBe(false);
});

test("horizontal scrollers consume the delta only while they can move that way", () => {
	const el = (scrollLeft, over = "auto") => ({
		scrollLeft,
		scrollWidth: 800,
		clientWidth: 400,
		overflowX: over,
		own: false,
	});
	expect(canScrollX(el(0), 5)).toBe(true); // can go right
	expect(canScrollX(el(0), -5)).toBe(false); // already at the left edge: gesture may take it
	expect(canScrollX(el(400), 5)).toBe(false); // at the right end
	expect(canScrollX(el(200), -5)).toBe(true);
	expect(canScrollX(el(0, "visible"), 5)).toBe(false);
	expect(canScrollX({ ...el(0), scrollWidth: 400 }, 5)).toBe(false); // nothing to scroll
	expect(canScrollX({ ...el(0, "hidden"), own: true }, 5)).toBe(true); // terminal / editor opt-in
	expect(consumedBy([el(400), el(0, "visible"), el(0)], 5)).toBe(true);
	expect(consumedBy([el(400), el(0, "visible")], 5)).toBe(false);
});

// ── edge peek intent ──

test("peek: a dwell in the hot zone opens after the delay, a flick opens at once", () => {
	expect(OPEN_DELAY).toBe(60);
	expect(peekAction({ open: false }, { t: "move", hot: true, over: false, speed: 0.2 })).toEqual({
		do: "schedule-open",
	});
	expect(peekAction({ open: false }, { t: "move", hot: true, over: false, speed: FLICK_SPEED })).toEqual({
		do: "open-now",
	});
	// A fast pass that never reaches the zone does nothing.
	expect(peekAction({ open: false }, { t: "move", hot: false, over: false, speed: 5 })).toEqual({
		do: "cancel",
	});
});

test("peek: close has grace, and the panel keeps the pointer for width + hysteresis", () => {
	expect(CLOSE_DELAY).toBe(250);
	expect(HYSTERESIS).toBe(24);
	const size = 260;
	const width = 1200;
	expect(inPanel("left", size + HYSTERESIS, width, size)).toBe(true);
	expect(inPanel("left", size + HYSTERESIS + 1, width, size)).toBe(false);
	expect(inPanel("right", width - size - HYSTERESIS, width, size)).toBe(true);
	expect(peekAction({ open: true }, { t: "move", hot: false, over: true })).toEqual({ do: "cancel" });
	expect(peekAction({ open: true }, { t: "move", hot: false, over: false })).toEqual({
		do: "schedule-close",
	});
	expect(peekAction({ open: true }, { t: "force-close" })).toEqual({ do: "close-now" });
});

test("peek hot path: moves inside one region are no-ops (no timers, no reactive writes)", () => {
	const size = 260;
	const width = 1200;
	// 1000 pointer moves across the middle of the window while the peek is closed.
	let region = regionOf("left", 600, width, size, false);
	let crossings = 0;
	for (let x = 400; x < 1100; x += 0.7) {
		const next = regionOf("left", x, width, size, false);
		if (!sameRegion(next, region)) crossings++;
		region = next;
	}
	expect(crossings).toBe(0);
	// Sweeping to the edge and back crosses exactly twice (enter, leave).
	region = regionOf("left", 600, width, size, false);
	crossings = 0;
	for (const x of [500, 300, 100, 20, HOT_ZONE, 2, 0, 2, HOT_ZONE, 20, 100, 300, 500]) {
		const next = regionOf("left", x, width, size, false);
		if (!sameRegion(next, region)) crossings++;
		region = next;
	}
	expect(crossings).toBe(2);
});

test("momentum decay ends the gesture at once instead of after the quiet gap", () => {
	const g = createGesture({ endIdle: 90 });
	let t = 0;
	// Fingers accelerate left (next), then lift: deltas decay geometrically.
	for (const d of [4, 10, 18, 26, 30]) g.feed(d, 0, (t += 8), 400);
	for (const d of [16, 12, 9]) g.feed(d, 0, (t += 8), 400);
	// No quiet gap has passed, yet the release is already decided.
	expect(g.poll(t + 1)).toEqual({ kind: "commit", dir: 1 });
	// The rest of the momentum tail is swallowed, not a new gesture.
	expect(g.feed(6, 0, (t += 8), 400)).toBeNull();
	expect(g.active).toBe(false);
});

test("slowing fingers that never decay below the peak fraction keep tracking", () => {
	const g = createGesture({ endIdle: 90 });
	let t = 0;
	for (const d of [10, 12, 12, 11, 11, 10]) g.feed(d, 0, (t += 8), 400);
	expect(g.poll(t + 1)).toBeNull();
	expect(g.active).toBe(true);
});
