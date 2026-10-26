// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { afterEach, beforeEach, expect, test } from "bun:test";
import { createClock, createPoller } from "./clock.svelte.ts";

// Incident class: every panel, drawer and tab kept its own `setInterval` (15 s clocks in Panel/ActivityDrawer/
// SetupRow/tray, a 1 s clock in ActionOutputDrawer, 5-10 s pollers per open tab), so an idle app with a hidden window
// still woke JS dozens of times a minute per project. One shared clock owns the timers and sleeps while nobody looks.

let t: number;
let timers: { fn: () => void; ms: number; id: number; alive: boolean }[];
let visible: boolean;
let focus: boolean;
const listeners: Record<string, Set<() => void>> = {
	visibility: new Set(),
	focus: new Set(),
	blur: new Set(),
};

function make(blurGraceMs = 60_000) {
	return createClock({
		now: () => t,
		setInterval: (fn, ms) => {
			const id = timers.length;
			timers.push({ fn, ms, id, alive: true });
			return id;
		},
		clearInterval: (id) => id !== undefined && (timers[id].alive = false),
		setTimeout: (fn, ms) => {
			const id = timers.length;
			timers.push({ fn, ms, id, alive: true });
			return id;
		},
		clearTimeout: (id) => id !== undefined && (timers[id].alive = false),
		visible: () => visible,
		focused: () => focus,
		onVisibility: (f) => (listeners.visibility.add(f), () => listeners.visibility.delete(f)),
		onFocus: (f) => (listeners.focus.add(f), () => listeners.focus.delete(f)),
		onBlur: (f) => (listeners.blur.add(f), () => listeners.blur.delete(f)),
		blurGraceMs,
	});
}
const live = () => timers.filter((x) => x.alive);
const fire = () => live().forEach((x) => x.fn());

beforeEach(() => {
	t = 1_000_000;
	timers = [];
	visible = true;
	focus = true;
	Object.values(listeners).forEach((s) => s.clear());
});
afterEach(() => {});

test("no subscriber, no timer", () => {
	make();
	expect(live()).toHaveLength(0);
});

test("a subscriber is called on each tick and unsubscribing stops the timer", () => {
	const c = make();
	let n = 0;
	const off = c.every(1000, () => n++);
	expect(live()).toHaveLength(1);
	fire();
	fire();
	expect(n).toBe(2);
	off();
	expect(live()).toHaveLength(0);
});

test("many subscribers share ONE timer, at the finest interval; slower ones are called less often", () => {
	const c = make();
	let fast = 0;
	let slow = 0;
	c.every(1000, () => fast++);
	c.every(15_000, () => slow++);
	expect(live()).toHaveLength(1);
	expect(live()[0].ms).toBe(1000);
	for (let i = 0; i < 30; i++) {
		t += 1000;
		fire();
	}
	expect(fast).toBe(30);
	expect(slow).toBe(2);
});

test("a hidden document runs no timer at all", () => {
	const c = make();
	let n = 0;
	c.every(1000, () => n++);
	visible = false;
	listeners.visibility.forEach((f) => f());
	expect(live()).toHaveLength(0);
});

test("becoming visible again ticks every subscriber once immediately (labels are stale), then resumes", () => {
	const c = make();
	let n = 0;
	c.every(15_000, () => n++);
	visible = false;
	listeners.visibility.forEach((f) => f());
	t += 600_000;
	visible = true;
	listeners.visibility.forEach((f) => f());
	expect(n).toBe(1);
	expect(live()).toHaveLength(1);
});

test("a window blurred for longer than the grace period sleeps; a brief blur does not", () => {
	const c = make(60_000);
	c.every(1000, () => {});
	focus = false;
	listeners.blur.forEach((f) => f());
	// the grace timer is the only live one besides the tick
	expect(live().some((x) => x.ms === 60_000)).toBe(true);
	expect(live().some((x) => x.ms === 1000)).toBe(true);
	live()
		.find((x) => x.ms === 60_000)
		.fn();
	expect(live().some((x) => x.ms === 1000)).toBe(false);
	focus = true;
	listeners.focus.forEach((f) => f());
	expect(live().some((x) => x.ms === 1000)).toBe(true);
});

test("focusing before the grace elapsed cancels the sleep", () => {
	const c = make(60_000);
	c.every(1000, () => {});
	focus = false;
	listeners.blur.forEach((f) => f());
	focus = true;
	listeners.focus.forEach((f) => f());
	expect(live().some((x) => x.ms === 60_000)).toBe(false);
	expect(live().some((x) => x.ms === 1000)).toBe(true);
});

test("a subscriber added while hidden does not start a timer until visible", () => {
	visible = false;
	const c = make();
	c.every(1000, () => {});
	expect(live()).toHaveLength(0);
	visible = true;
	listeners.visibility.forEach((f) => f());
	expect(live()).toHaveLength(1);
});

test("adding a finer subscriber re-times the shared timer; removing it relaxes it again", () => {
	const c = make();
	c.every(15_000, () => {});
	expect(live()[0].ms).toBe(15_000);
	const off = c.every(1000, () => {});
	expect(live()).toHaveLength(1);
	expect(live()[0].ms).toBe(1000);
	off();
	expect(live()).toHaveLength(1);
	expect(live()[0].ms).toBe(15_000);
});

test("now() is reactive state refreshed on each tick", () => {
	const c = make();
	const off = c.every(1000, () => {});
	const before = c.now;
	t += 5000;
	fire();
	expect(c.now).toBe(before + 5000);
	off();
});

test("a throwing subscriber does not stop the others", () => {
	const c = make();
	let ok = 0;
	c.every(1000, () => {
		throw new Error("boom");
	});
	c.every(1000, () => ok++);
	expect(() => fire()).not.toThrow();
	expect(ok).toBe(1);
});

// PrTab / ChatTab / SourceControl each polled on their own timer even when their tab was a background tab of a pane.
test("a poller runs only while its gate is open and polls once when the gate opens", () => {
	const c = make();
	let n = 0;
	let open = false;
	const p = createPoller(c, 10_000, () => n++);
	p.gate(open);
	expect(live()).toHaveLength(0);
	open = true;
	p.gate(open);
	expect(n).toBe(1);
	expect(live()).toHaveLength(1);
	fire();
	expect(n).toBe(2);
	p.gate(false);
	expect(live()).toHaveLength(0);
	p.stop();
});

test("re-opening an already open gate does not poll again or stack timers", () => {
	const c = make();
	let n = 0;
	const p = createPoller(c, 10_000, () => n++);
	p.gate(true);
	p.gate(true);
	p.gate(true);
	expect(n).toBe(1);
	expect(live()).toHaveLength(1);
	p.stop();
	expect(live()).toHaveLength(0);
});
