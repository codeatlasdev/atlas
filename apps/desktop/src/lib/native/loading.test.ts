// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { createLoading, SHOW_AFTER_MS, STALL_MS } from "./loading.ts";

// The core owns one `loading` state per native surface (browser page load, terminal open, any future kind). Adapters
// only report events; the state machine decides when the indicator shows (never for a fast load), when it ends, and
// when a load that never reports back has stalled. A fake clock: no wall-clock timing in the tests.
function clock() {
	let t = 0;
	const timers = new Map<number, { at: number; fn: () => void }>();
	let next = 1;
	return {
		set: (fn: () => void, ms: number) => {
			timers.set(next, { at: t + ms, fn });
			return next++;
		},
		clear: (id: number) => void timers.delete(id),
		advance(ms: number) {
			const to = t + ms;
			for (;;) {
				const due = [...timers.entries()].filter(([, v]) => v.at <= to).sort((a, b) => a[1].at - b[1].at)[0];
				if (!due) break;
				t = due[1].at;
				timers.delete(due[0]);
				due[1].fn();
			}
			t = to;
		},
		pending: () => timers.size,
	};
}

function make(over = {}) {
	const c = clock();
	const log: string[] = [];
	const l = createLoading({
		timers: c,
		onChange: (s) => log.push(`${s.active ? "active" : "idle"}/${s.shown ? "shown" : "hidden"}`),
		onStall: () => log.push("stall"),
		...over,
	});
	return { c, l, log };
}

test("a fast load never shows the indicator (no flash), a slow one shows after the delay", () => {
	const { c, l } = make();
	l.start();
	c.advance(SHOW_AFTER_MS - 1);
	expect(l.state().shown).toBe(false);
	l.finish();
	c.advance(1000);
	expect(l.state()).toMatchObject({ active: false, shown: false });

	l.start();
	c.advance(SHOW_AFTER_MS);
	expect(l.state()).toMatchObject({ active: true, shown: true });
	l.finish();
	expect(l.state()).toMatchObject({ active: false, shown: false }); // the indicator fades out (CSS), state is off
});

test("a load that never reports finish stalls once and ends: no bar forever", () => {
	const { c, l, log } = make();
	l.start();
	c.advance(STALL_MS - 1);
	expect(l.state().active).toBe(true);
	c.advance(1);
	expect(log.filter((x) => x === "stall")).toHaveLength(1);
	expect(l.state()).toMatchObject({ active: false, shown: false });
	c.advance(10 * STALL_MS);
	expect(log.filter((x) => x === "stall")).toHaveLength(1); // and never again
	expect(c.pending()).toBe(0);
});

// Red test the maintainer asked for: a slow VALID page that keeps reporting progress (a redirect, a title, a nav) must
// never be marked failed, however long it takes in total.
test("a slow page that keeps reporting progress is never stalled", () => {
	const { c, l, log } = make();
	l.start();
	for (let i = 0; i < 20; i++) {
		c.advance(STALL_MS - 1000);
		l.progress();
	}
	expect(log).not.toContain("stall");
	expect(l.state().active).toBe(true);
	c.advance(STALL_MS);
	expect(log).toContain("stall"); // silence is what stalls it
});

test("a nav while loading (a redirect) re-arms the stall timer but does not restart the show delay", () => {
	const { c, l } = make();
	l.start();
	c.advance(SHOW_AFTER_MS);
	expect(l.state().shown).toBe(true);
	c.advance(STALL_MS - SHOW_AFTER_MS - 100);
	l.start(); // the redirect's nav event
	expect(l.state().shown).toBe(true); // still shown: no flicker back to hidden
	c.advance(STALL_MS - 100);
	expect(l.state().active).toBe(true); // the timer was re-armed
});

test("fail ends the load like finish, without a stall", () => {
	const { c, l, log } = make();
	l.start();
	c.advance(SHOW_AFTER_MS);
	l.fail();
	expect(l.state()).toMatchObject({ active: false, shown: false });
	c.advance(10 * STALL_MS);
	expect(log).not.toContain("stall");
});

test("independent sources (the core's own `open` and the page's `load`) both have to end", () => {
	const { c, l } = make();
	l.start("open");
	l.start("load");
	c.advance(SHOW_AFTER_MS);
	l.finish("open");
	expect(l.state().active).toBe(true); // the page is still loading
	l.finish("load");
	expect(l.state().active).toBe(false);
});

test("progress or finish with nothing loading is inert and arms nothing", () => {
	const { c, l, log } = make();
	l.progress();
	l.finish();
	l.fail();
	expect(c.pending()).toBe(0);
	expect(log).toEqual([]);
});

test("dispose cancels every timer and reports nothing afterwards", () => {
	const { c, l, log } = make();
	l.start();
	l.dispose();
	expect(c.pending()).toBe(0);
	c.advance(10 * STALL_MS);
	l.start();
	expect(log).toEqual(["active/hidden"]);
});

test("a custom stall time is honoured", () => {
	const { c, l, log } = make({ stallMs: 1000 });
	l.start();
	c.advance(1000);
	expect(log).toContain("stall");
});
