// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { paneSync, reconcile, staleViews } from "./browserSync.ts";

const R = { x: 240, y: 80, w: 900, h: 600 };

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
	const errors = [];
	const p = paneSync({
		tab: "t1",
		invoke: n.invoke,
		measure: () => rect,
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
});

test("a hidden tab opens parked (visible:false) with its real rect", async () => {
	const n = native();
	const t = pane(n);
	t.setShown(false);
	await t.p.sync();
	expect(n.calls.find((c) => c.cmd === "browser_open").visible).toBe(false);
	expect(n.calls.find((c) => c.cmd === "browser_bounds").visible).toBe(false);
});

test("no url or no element: nothing is opened", async () => {
	const n = native();
	const t = pane(n);
	t.setUrl(undefined);
	await t.p.sync();
	const gone = pane(n, { measure: () => null });
	await gone.p.sync();
	expect(n.calls).toEqual([]);
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
});

test("a throwing measure does not poison the chain", async () => {
	const n = native();
	let boom = true;
	const t = pane(n, {
		measure: () => {
			if (boom) {
				boom = false;
				throw new Error("detached");
			}
			return R;
		},
	});
	await t.p.sync();
	await t.p.sync();
	expect(n.names()).toContain("browser_open");
	expect(t.errors).toEqual(["Error: detached"]);
});
