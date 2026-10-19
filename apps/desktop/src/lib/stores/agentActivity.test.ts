// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { activityTracker, QUIET_MS } from "./agentActivity.ts";

/** A manual clock: `advance(ms)` runs every timer that falls due, in order. */
function clock() {
	let now = 0;
	let seq = 0;
	const due = new Map();
	return {
		timers: {
			set: (fn, ms) => (due.set(++seq, { at: now + ms, fn }), seq),
			clear: (h) => due.delete(h),
		},
		advance(ms) {
			now += ms;
			for (const [h, t] of [...due].sort((a, b) => a[1].at - b[1].at))
				if (t.at <= now && due.delete(h)) t.fn();
		},
	};
}

describe("agent activity without hooks (#56)", () => {
	test("output is working once per burst, idle only after QUIET_MS of silence", () => {
		const c = clock();
		const seen = [];
		const a = activityTracker((pane, s) => seen.push(`${pane}:${s}`), c.timers);
		a.bump("t1");
		c.advance(QUIET_MS - 1);
		a.bump("t1");
		c.advance(QUIET_MS - 1);
		expect(seen).toEqual(["t1:working"]);
		c.advance(1);
		expect(seen).toEqual(["t1:working", "t1:idle"]);
		a.bump("t1");
		expect(seen.at(-1)).toBe("t1:working");
	});

	test("panes are independent, and a forgotten pane never reports idle", () => {
		const c = clock();
		const seen = [];
		const a = activityTracker((pane, s) => seen.push(`${pane}:${s}`), c.timers);
		a.bump("a");
		a.bump("b");
		a.forget("a");
		c.advance(QUIET_MS);
		expect(seen).toEqual(["a:working", "b:working", "b:idle"]);
	});
});
