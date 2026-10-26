// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import "../../test/setup.ts";
import { browserSurface, reconcile, staleViews } from "./browserSync.ts";
import { STRIP } from "./native/indicator.ts";

const R = { x: 240, y: 80, w: 900, h: 600 };
/** What the browser's view gets: the pane, minus the core's loading strip on top. */
const V = { x: R.x, y: R.y + STRIP, w: R.w, h: R.h - STRIP };

/**
 * A fake native side. `hold` names commands that block until `release(cmd)` is called, so a
 * test decides exactly when an in-flight call finishes (no wall-clock timing).
 */
function native(opts = {}) {
	const calls = [];
	const held = {};
	let live = opts.live ?? [];
	const started = {};
	const invoke = async (cmd, args = {}) => {
		calls.push({ cmd, ...args });
		started[cmd]?.resolve();
		if (opts.hold?.includes(cmd)) {
			held[cmd] = Promise.withResolvers();
			await held[cmd].promise;
		}
		if (cmd === "browser_list") return live;
		if (cmd === "browser_close") live = live.filter((t) => t !== args.tab);
		if (opts.fail === cmd) throw new Error(`${cmd} failed`);
		return undefined;
	};
	return {
		calls,
		invoke,
		names: () => calls.map((c) => c.cmd),
		/** Resolves once `cmd` has been invoked (it may still be held). */
		reached(cmd) {
			if (calls.some((c) => c.cmd === cmd)) return Promise.resolve();
			started[cmd] = Promise.withResolvers();
			return started[cmd].promise;
		},
		release(cmd) {
			held[cmd]?.resolve();
		},
	};
}

test("staleViews: native views without a tab", () => {
	expect(staleViews(["a", "b", "c"], ["b"])).toEqual(["a", "c"]);
	expect(staleViews([], ["b"])).toEqual([]);
	expect(staleViews(["a"], [])).toEqual(["a"]);
});

test("reconcile closes only orphans and reports them", async () => {
	const n = native({ live: ["keep", "gone1", "gone2"] });
	const closed = await reconcile(n.invoke, ["keep", "other-kind-tab"]);
	expect(closed).toEqual(["gone1", "gone2"]);
	expect(n.calls.filter((c) => c.cmd === "browser_close").map((c) => c.tab)).toEqual(["gone1", "gone2"]);
});

test("reconcile with nothing live is a no-op; a failing close does not throw", async () => {
	const none = native({ live: [] });
	expect(await reconcile(none.invoke, ["a"])).toEqual([]);
	expect(none.names()).toEqual(["browser_list"]);
	const bad = native({ live: ["x"], fail: "browser_close" });
	expect(await reconcile(bad.invoke, [])).toEqual(["x"]);
});

function pane(n, over = {}) {
	let rect = R;
	let shown = true;
	let url = "https://example.com";
	let attached = true;
	const errors = [];
	const el = document.createElement("div");
	el.getBoundingClientRect = () => new DOMRect(rect.x, rect.y, rect.w, rect.h);
	document.body.append(el);
	const p = browserSurface({
		tab: "t1",
		invoke: n.invoke,
		slot: () => (attached ? el : undefined),
		shown: () => shown,
		url: () => url,
		onError: (e) => errors.push(String(e)),
		...over,
	});
	return {
		p,
		errors,
		setRect: (r) => (rect = r),
		setShown: (s) => (shown = s),
		setUrl: (u) => (url = u),
		detach: () => (attached = false),
		el,
	};
}

test("concurrent observers open once, then only bounds, strictly in order", async () => {
	const n = native({ hold: ["browser_open"] });
	const { p } = pane(n);
	// ResizeObserver + MutationObserver + effect + window resize all firing at mount,
	// while `browser_open` is still in flight.
	void p.sync();
	void p.sync();
	void p.sync();
	await n.reached("browser_open");
	expect(n.names()).toEqual(["browser_open"]); // nothing overtakes the open
	n.release("browser_open");
	await p.settled();
	const names = n.names();
	expect(names.filter((c) => c === "browser_open")).toHaveLength(1);
	expect(names[0]).toBe("browser_open");
	expect(names.indexOf("browser_open")).toBeLessThan(names.indexOf("browser_bounds"));
	expect(n.calls.find((c) => c.cmd === "browser_open")).toMatchObject({
		tab: "t1",
		url: "https://example.com",
		...V,
	});
	await p.dispose();
});

test("syncs coalesce, and the queued one measures fresh when it runs", async () => {
	const n = native({ hold: ["browser_bounds"] });
	const t = pane(n);
	void t.p.sync();
	await n.reached("browser_bounds"); // open done, first bounds in flight
	n.calls.length = 0;
	void t.p.sync();
	void t.p.sync();
	void t.p.sync();
	t.setRect({ ...R, w: 500 }); // layout moved while those were queued
	n.release("browser_bounds"); // first bounds finishes; the queued run starts
	await n.reached("browser_bounds");
	n.release("browser_bounds");
	await t.p.settled();
	const bounds = n.calls.filter((c) => c.cmd === "browser_bounds");
	expect(bounds).toHaveLength(1); // three queued syncs collapsed into one
	expect(bounds[0].w).toBe(500);
	await t.p.dispose();
});

test("a hidden tab opens parked (visible:false) with its real rect", async () => {
	const n = native();
	const t = pane(n);
	t.setShown(false);
	await t.p.sync();
	expect(n.calls.find((c) => c.cmd === "browser_open")).toMatchObject({ visible: false, ...V });
	expect(n.calls.find((c) => c.cmd === "browser_bounds").visible).toBe(false);
	await t.p.dispose();
});

test("no url or no element: nothing is opened", async () => {
	const n = native();
	const t = pane(n);
	t.setUrl(undefined);
	await t.p.sync();
	const gone = pane(n);
	gone.detach();
	await gone.p.sync();
	expect(n.calls).toEqual([]);
	await t.p.dispose();
	await gone.p.dispose();
});

test("layout moves during open: bounds use the latest rect and visibility", async () => {
	const n = native({ hold: ["browser_open"] });
	const t = pane(n);
	const done = t.p.sync();
	await n.reached("browser_open");
	t.setRect({ ...R, x: 0 }); // sidebar hid while open was in flight
	t.setShown(false); // and a dialog opened
	n.release("browser_open");
	await done;
	const b = n.calls.find((c) => c.cmd === "browser_bounds");
	expect(b.x).toBe(0);
	expect(b.visible).toBe(false);
	await t.p.dispose();
});

test("dispose waits for in-flight work, parks last, and later syncs are inert", async () => {
	const n = native({ hold: ["browser_bounds"] });
	const t = pane(n);
	void t.p.sync();
	await n.reached("browser_bounds"); // a bounds call is in flight at destroy time
	const disposed = t.p.dispose();
	n.release("browser_bounds");
	await disposed;
	expect(n.names().at(-1)).toBe("browser_park");
	n.calls.length = 0;
	await t.p.sync(); // late ResizeObserver callback after destroy
	expect(n.calls).toEqual([]);
});

test("a failing open reports the error and the next sync retries it", async () => {
	let fail = true;
	const n = native();
	const inv = async (cmd, a) => {
		if (cmd === "browser_open" && fail) {
			fail = false;
			throw new Error("boom");
		}
		return n.invoke(cmd, a);
	};
	const t = pane(n, { invoke: inv });
	await t.p.sync();
	expect(t.errors).toEqual(["Error: boom"]);
	expect(t.p.isOpen()).toBe(false);
	await t.p.sync();
	expect(t.p.isOpen()).toBe(true);
	await t.p.dispose();
});

test("a throwing measure does not poison the chain", async () => {
	const n = native();
	let boom = true;
	const t = pane(n);
	const el = t.el;
	const real = el.getBoundingClientRect.bind(el);
	el.getBoundingClientRect = () => {
		if (boom) {
			boom = false;
			throw new Error("detached");
		}
		return real();
	};
	await t.p.sync();
	await t.p.sync();
	expect(n.names()).toContain("browser_open");
	expect(t.errors).toEqual(["Error: detached"]);
	await t.p.dispose();
});

// Maintainer GIF (2026-10-26): a pale-blue load bar on top of the white native page, stuck. The page's own load events
// (`nav`, `load`, `fail`, a title) feed the core's loading state; a page that never answers ends as a stall (its error
// screen), and the bar is the core's strip above the view, never inside the page.
function fakeTimers() {
	const timers = new Map();
	let next = 1;
	let now = 0;
	return {
		set: (fn, ms) => (timers.set(next, { fn, at: now + ms }), next++),
		clear: (id) => timers.delete(id),
		advance(ms) {
			const to = now + ms;
			for (;;) {
				const due = [...timers].filter(([, t]) => t.at <= to).sort((x, y) => x[1].at - y[1].at)[0];
				if (!due) break;
				now = due[1].at;
				timers.delete(due[0]);
				due[1].fn();
			}
			now = to;
		},
	};
}

test("a page that never answers stalls once (its error screen); the page keeps its rect below the strip", async () => {
	const n = native();
	const stalled = [];
	const clock = fakeTimers();
	const t = pane(n, { onStall: () => stalled.push(1), stallMs: 15_000 });
	await t.p.sync();
	expect(n.calls.find((c) => c.cmd === "browser_bounds")).toMatchObject(V);
	await t.p.dispose();
	const u = browserSurface(
		{
			tab: "t2",
			invoke: n.invoke,
			slot: () => document.body.firstElementChild,
			shown: () => true,
			url: () => "http://a/",
			onError() {},
			onStall: () => stalled.push(2),
			stallMs: 15_000,
		},
		{ timers: clock },
	);
	u.loading.start(); // the page's `nav`
	clock.advance(14_999);
	expect(stalled).toEqual([]);
	clock.advance(1);
	expect(stalled).toEqual([2]);
	await u.dispose();
});

// Red test the maintainer asked for: a slow VALID page that keeps reporting progress is never marked failed.
test("a slow valid page that keeps reporting progress is never stalled", async () => {
	const n = native();
	const stalled = [];
	const clock = fakeTimers();
	const el = document.createElement("div");
	el.getBoundingClientRect = () => new DOMRect(0, 0, 100, 100);
	document.body.append(el);
	const u = browserSurface(
		{
			tab: "t3",
			invoke: n.invoke,
			slot: () => el,
			shown: () => true,
			url: () => "http://slow/",
			onError() {},
			onStall: () => stalled.push(1),
		},
		{ timers: clock },
	);
	u.loading.start();
	for (let i = 0; i < 12; i++) {
		clock.advance(10_000);
		u.loading.progress(); // a title, a redirect's nav: the page is alive
	}
	expect(stalled).toEqual([]);
	u.loading.finish();
	expect(u.loading.state().active).toBe(false);
	await u.dispose();
});

test("a failed page is parked (failed()), never a white rectangle", async () => {
	const n = native();
	let failed = false;
	const t = pane(n, { failed: () => failed });
	await t.p.sync();
	failed = true;
	await t.p.sync();
	expect(n.calls.filter((c) => c.cmd === "browser_bounds").at(-1).visible).toBe(false);
	await t.p.dispose();
});
