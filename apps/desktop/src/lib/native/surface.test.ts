// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, test } from "bun:test";
import {
	createNativeSurface,
	hasNativeSurface,
	nativeSurfaces,
	reconcileNatives,
	registerNativeKind,
	reloadNativeSurface,
} from "./surface.ts";
import { clipPath, resetHoles } from "./ground.ts";
import { STRIP } from "./indicator.ts";
import { SHOW_AFTER_MS, STALL_MS } from "./loading.ts";

// The native-view core. A "future kind" is a FAKE one here: it registers an adapter and nothing else, and must get the
// whole behaviour of the terminal and the browser for free: serialized open/bounds/close, the slot hole, park, the
// reconciler, dispose ordering, the loading strip. Real kinds (terminal, browser) are thin adapters over
// the same thing and have their own tests next to their adapters.

const SLOT = { x: 200, y: 100, w: 600, h: 400 };
const flushMicro = () => Promise.resolve().then(() => Promise.resolve());

function slotEl(r = SLOT) {
	const el = document.createElement("div");
	el.getBoundingClientRect = () => new DOMRect(r.x, r.y, r.w, r.h);
	document.body.append(el);
	return el;
}

function overlay(at: { x: number; y: number; w: number; h: number }, role = "dialog") {
	const d = document.createElement("div");
	d.setAttribute("role", role);
	d.dataset.state = "open";
	d.getBoundingClientRect = () => new DOMRect(at.x, at.y, at.w, at.h);
	document.body.append(d);
	return d;
}

/** A fake native side + a fake kind adapter on top of it. `hold` names calls that block until `release(name)`. */
function fake(opts: { hold?: string[]; failOpen?: boolean; gone?: boolean } = {}) {
	const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
	const waiting: Record<string, PromiseWithResolvers<void>[]> = {};
	const tokens: Record<string, number> = {};
	const started: Record<string, PromiseWithResolvers<void>> = {};
	const call = async (cmd: string, args?: Record<string, unknown>) => {
		calls.push({ cmd, args });
		started[cmd]?.resolve();
		if (opts.hold?.includes(cmd)) {
			// a held call waits for ONE `release`; a release that came first is waiting for its call
			if (tokens[cmd]) tokens[cmd]--;
			else {
				const gate = Promise.withResolvers<void>();
				(waiting[cmd] ??= []).push(gate);
				await gate.promise;
			}
		}
		return undefined;
	};
	const live = new Set<string>();
	const natives = {
		invoke: call,
		names: () => calls.map((c) => c.cmd),
		of: (cmd: string) => calls.filter((c) => c.cmd === cmd),
		reached(cmd: string) {
			if (calls.some((c) => c.cmd === cmd)) return Promise.resolve();
			started[cmd] = Promise.withResolvers();
			return started[cmd].promise;
		},
		release(cmd: string) {
			const gate = waiting[cmd]?.shift();
			if (gate) gate.resolve();
			else tokens[cmd] = (tokens[cmd] ?? 0) + 1;
		},
		live,
	};
	return natives;
}

function adapterFor(n: ReturnType<typeof fake>, over = {}) {
	return {
		kind: "fake",
		id: "f1",
		slot: () => state.slot,
		visible: () => state.visible,
		async open(rect, shown) {
			await n.invoke("fake_open", { id: "f1", ...rect, visible: shown });
			n.live.add("f1");
		},
		async place(rect, shown) {
			await n.invoke("fake_bounds", { id: "f1", ...rect, visible: shown });
		},
		async release() {
			await n.invoke("fake_close", { id: "f1" });
			n.live.delete("f1");
		},
		...over,
	};
}
const state = { slot: undefined as HTMLElement | undefined, visible: true };
let surfaces: { dispose(): Promise<void> }[] = [];
const make = (n, over = {}, opts = {}) => {
	const s = createNativeSurface(adapterFor(n, over), opts);
	surfaces.push(s);
	return s;
};

beforeEach(() => {
	resetHoles();
	document.documentElement.className = "";
	document.body.innerHTML = "";
	state.slot = slotEl();
	state.visible = true;
	surfaces = [];
});
afterEach(async () => {
	await Promise.all(surfaces.map((s) => s.dispose()));
});

test("a fake kind opens once, then only places, strictly in order, with the rect inset by the loading strip", async () => {
	const n = fake({ hold: ["fake_open"] });
	const s = make(n);
	void s.sync();
	void s.sync();
	void s.sync();
	await n.reached("fake_open");
	expect(n.names()).toEqual(["fake_open"]); // nothing overtakes the open
	n.release("fake_open");
	await s.settled();
	expect(n.of("fake_open")).toHaveLength(1);
	expect(n.names().indexOf("fake_open")).toBeLessThan(n.names().indexOf("fake_bounds"));
	// The native rect starts STRIP below the slot's top: the strip above the view belongs to the indicator.
	expect(n.of("fake_bounds")[0].args).toMatchObject({
		x: 200,
		y: 100 + STRIP,
		w: 600,
		h: 400 - STRIP,
		visible: true,
	});
});

test("syncs coalesce and the queued one measures fresh when it runs", async () => {
	const n = fake({ hold: ["fake_bounds"] });
	const s = make(n);
	void s.sync();
	await n.reached("fake_bounds"); // open done, the first placement is in flight
	void s.sync();
	void s.sync();
	void s.sync();
	state.slot.getBoundingClientRect = () => new DOMRect(200, 100, 500, 400); // layout moved while those were queued
	n.release("fake_bounds"); // the first placement ends; the queued run starts and is held too
	n.release("fake_bounds");
	await s.settled();
	const bounds = n.of("fake_bounds");
	expect(bounds).toHaveLength(2); // the first, plus ONE for the three queued syncs
	expect(bounds[1].args.w).toBe(500);
});

// Incident (2026-10-26): a space switch unmounted a layer while its open was in flight. The close ran outside the
// open/bounds chain, found no view and returned; the open then created a view nobody owned, on screen over every other
// space. Every kind gets this ordering from the core.
test("dispose while open is in flight releases the view AFTER the open, never before", async () => {
	const n = fake({ hold: ["fake_open"] });
	const s = make(n);
	void s.sync();
	await n.reached("fake_open");
	const disposed = s.dispose();
	n.release("fake_open");
	await disposed;
	expect(n.names().at(-1)).toBe("fake_close");
	expect(n.names().indexOf("fake_close")).toBeGreaterThan(n.names().indexOf("fake_open"));
	expect(n.live.size).toBe(0);
});

test("after dispose no sync opens, places or shows the view again, and the registry forgets it", async () => {
	const n = fake();
	const s = make(n);
	expect(hasNativeSurface("fake", "f1")).toBe(true);
	await s.dispose();
	n.names().length = 0;
	await s.sync();
	expect(n.of("fake_open")).toEqual([]);
	expect(n.of("fake_bounds")).toEqual([]);
	expect(hasNativeSurface("fake", "f1")).toBe(false);
});

test("a hidden surface does not open by default (it would flash over the UI); an adapter may open parked or hold back", async () => {
	const n = fake();
	state.visible = false;
	const s = make(n);
	await s.sync();
	expect(n.of("fake_open")).toEqual([]);
	const m = fake();
	const t = createNativeSurface(adapterFor(m, { id: "f2", canOpen: () => true }));
	surfaces.push(t);
	await t.sync();
	expect(m.of("fake_open")[0].args.visible).toBe(false);
	expect(m.of("fake_bounds")[0].args.visible).toBe(false);
	// ...and `canOpen` can also say "not yet" for a visible one (a browser tab with no URL)
	const k = fake();
	const u = createNativeSurface(adapterFor(k, { id: "f4", canOpen: () => false }));
	surfaces.push(u);
	await u.sync();
	expect(k.names()).toEqual([]);
});

test("layout moves while open is in flight: the placement uses the latest rect and visibility", async () => {
	const n = fake({ hold: ["fake_open"] });
	const s = make(n);
	const done = s.sync();
	await n.reached("fake_open");
	state.slot.getBoundingClientRect = () => new DOMRect(0, 0, 300, 300); // the sidebar hid while open was in flight
	state.visible = false; // and a dialog opened
	n.release("fake_open");
	await done;
	expect(n.of("fake_bounds")[0].args).toMatchObject({ x: 0, visible: false });
});

test("no slot (not laid out) opens nothing; a throwing measure does not poison the chain", async () => {
	const n = fake();
	state.slot = undefined;
	const s = make(n);
	await s.sync();
	expect(n.names()).toEqual([]);
	let boom = true;
	const el = state.slot ?? slotEl();
	state.slot = el;
	const real = el.getBoundingClientRect.bind(el);
	el.getBoundingClientRect = () => {
		if (boom) {
			boom = false;
			throw new Error("detached");
		}
		return real();
	};
	const errors = [];
	const t = createNativeSurface(adapterFor(n, { id: "f9", onError: (e) => void errors.push(String(e)) }));
	surfaces.push(t);
	await t.sync();
	await t.sync();
	expect(errors).toEqual(["Error: detached"]);
	expect(n.of("fake_open").length).toBeGreaterThan(0);
});

test("a failing open reports the error and the next sync retries it; `stop` ends it for good", async () => {
	let fail = true;
	const n = fake();
	const errors = [];
	const s = make(n, {
		async open(rect, shown) {
			if (fail) {
				fail = false;
				throw new Error("boom");
			}
			await n.invoke("fake_open", { id: "f1", ...rect, visible: shown });
		},
		onError: (e) => void errors.push(String(e)),
	});
	await s.sync();
	expect(errors).toEqual(["Error: boom"]);
	expect(s.isOpen()).toBe(false);
	await s.sync();
	expect(s.isOpen()).toBe(true);

	const m = fake();
	const calls = [];
	const t = createNativeSurface(
		adapterFor(m, {
			id: "f3",
			async open() {
				throw new Error("fatal");
			},
			onError: (e) => {
				calls.push(String(e));
				return "stop";
			},
		}),
	);
	surfaces.push(t);
	await t.sync();
	await t.sync();
	expect(calls).toEqual(["Error: fatal"]); // a "stop" adapter is never retried
});

test("a view that went away (adapter says so) is opened again over the same id", async () => {
	const n = fake();
	let dropped = false;
	const s = make(n, {
		async place(rect, shown) {
			if (dropped) {
				dropped = false;
				throw new Error("view dropped");
			}
			await n.invoke("fake_bounds", { id: "f1", ...rect, visible: shown });
		},
		gone: (e) => String(e).includes("view dropped"),
	});
	await s.sync();
	dropped = true;
	await s.sync();
	expect(n.of("fake_open")).toHaveLength(2);
});

// ── Holes and park: free for every kind. The page is transparent over the native view's slot (`ground.ts`); overlays are
// ordinary DOM above it and never touch the hole. The hole is the view's box and follows the slot, in the same task.
const holeOf = (slot) => slot.style.clipPath;
const holeIs = (slot, r) =>
	holeOf(slot) === clipPath([r], slot.getBoundingClientRect().width, slot.getBoundingClientRect().height);

test("the view's box is a hole in its slot's fill once the view has been placed shown: never before", async () => {
	document.documentElement.classList.add("ground-holes");
	const n = fake({ hold: ["fake_bounds"] });
	const s = make(n);
	void s.sync();
	await n.reached("fake_bounds");
	// open done, the placement is in flight: the page still paints the slot (no hole over a view that has not landed)
	expect(holeOf(state.slot)).toBe("");
	n.release("fake_bounds");
	await s.settled();
	// the view's box is the slot minus the loading strip
	expect(holeIs(state.slot, { x: 0, y: STRIP, w: 600, h: 400 - STRIP })).toBe(true);
});

test("the hole follows a layout change in the same call as the sync, before any IPC answers", async () => {
	document.documentElement.classList.add("ground-holes");
	const n = fake({ hold: ["fake_bounds"] });
	const s = make(n);
	void s.sync();
	await n.reached("fake_bounds");
	n.release("fake_bounds");
	await s.settled();
	state.slot.getBoundingClientRect = () => new DOMRect(200, 100, 500, 400);
	void s.sync(); // the placement is queued behind the held IPC...
	expect(holeIs(state.slot, { x: 0, y: STRIP, w: 500, h: 400 - STRIP })).toBe(true); // ...the hole is already there
	n.release("fake_bounds");
	await s.settled();
});

test("hiding the view (inactive tab, error screen) restores the slot's paint at once, before the IPC that hides it", async () => {
	document.documentElement.classList.add("ground-holes");
	const n = fake({ hold: ["fake_bounds"] });
	const s = make(n);
	void s.sync();
	await n.reached("fake_bounds");
	n.release("fake_bounds");
	await s.settled();
	expect(holeOf(state.slot)).not.toBe("");
	state.visible = false;
	void s.sync();
	expect(holeOf(state.slot)).toBe(""); // synchronous: the page covers the slot this very frame
	await n.reached("fake_bounds");
	n.release("fake_bounds");
	await s.settled();
	expect(holeOf(state.slot)).toBe("");
});

test("an overlay over the slot changes nothing: no hole, no IPC about it", async () => {
	document.documentElement.classList.add("ground-holes");
	const n = fake();
	const s = make(n);
	await s.sync();
	const before = holeOf(state.slot);
	const bounds = n.of("fake_bounds").length;
	overlay({ x: 500, y: 300, w: 400, h: 300 });
	await s.sync();
	expect(holeOf(state.slot)).toBe(before);
	expect(n.names().filter((c) => c !== "fake_open" && c !== "fake_bounds")).toEqual([]);
	expect(n.of("fake_bounds").length).toBe(bounds + 1); // the same placement again, nothing else
	expect(n.of("fake_bounds").at(-1).args.visible).toBe(true); // a dialog never parks the view
});

test("a slot inside a swiping pane is parked: placed hidden, no hole, and the view stays alive", async () => {
	document.documentElement.classList.add("ground-holes");
	const n = fake();
	const s = make(n);
	await s.sync();
	expect(holeOf(state.slot)).not.toBe("");
	const pane = document.createElement("div");
	pane.setAttribute("data-swiping", "");
	pane.append(state.slot);
	document.body.append(pane);
	await s.sync();
	expect(n.of("fake_bounds").at(-1).args.visible).toBe(false);
	expect(holeOf(state.slot)).toBe("");
	pane.removeAttribute("data-swiping");
	await s.sync();
	expect(n.of("fake_bounds").at(-1).args.visible).toBe(true);
	expect(holeOf(state.slot)).not.toBe("");
});

test("a surface that is not visible (inactive tab) or `extraHidden` (an error screen) is placed hidden", async () => {
	const n = fake();
	let hide = false;
	const s = make(n, { extraHidden: () => hide });
	await s.sync();
	hide = true;
	await s.sync();
	expect(n.of("fake_bounds").at(-1).args.visible).toBe(false);
	hide = false;
	state.visible = false;
	await s.sync();
	expect(n.of("fake_bounds").at(-1).args.visible).toBe(false);
});

test("a view that is replaced (reopened) has no hole until the new one has landed", async () => {
	document.documentElement.classList.add("ground-holes");
	const n = fake({ hold: ["fake_open"] });
	let dropped = false;
	const s = make(n, {
		async place(rect, shown) {
			if (dropped) {
				dropped = false;
				throw new Error("view dropped");
			}
			await n.invoke("fake_bounds", { id: "f1", ...rect, visible: shown });
		},
		gone: (e) => String(e).includes("dropped"),
	});
	n.release("fake_open");
	await s.sync();
	expect(holeOf(state.slot)).not.toBe("");
	dropped = true;
	void s.sync();
	await n.reached("fake_open"); // the second open (the first was released ahead) is held mid-flight
	await flushMicro();
	// the old view is gone and the new one is not here yet: the slot is painted, not left see-through
	expect(holeOf(state.slot)).toBe("");
	n.release("fake_open");
	await s.settled();
	expect(holeOf(state.slot)).not.toBe("");
});

test("disposing the surface restores the slot's paint: the hole never outlives its owner", async () => {
	document.documentElement.classList.add("ground-holes");
	const n = fake();
	const s = make(n);
	await s.sync();
	expect(holeOf(state.slot)).not.toBe("");
	await s.dispose();
	expect(holeOf(state.slot)).toBe("");
});

// ── Registry + reconcile.
test("the registry holds every surface by kind and id; a remounted id is not removed by the old surface's dispose", async () => {
	const n = fake();
	const a = make(n);
	expect(nativeSurfaces.get("fake", "f1")).toBeDefined();
	expect(nativeSurfaces.ofKind("fake").size).toBe(1);
	const b = createNativeSurface(adapterFor(n));
	surfaces.push(b);
	await a.dispose(); // the old one goes away after the new one registered under the same id
	expect(nativeSurfaces.get("fake", "f1")).toBeDefined();
	await b.dispose();
	expect(nativeSurfaces.get("fake", "f1")).toBeUndefined();
});

test("reloadNativeSurface reaches the adapter's reload; false for a kind or id without one", async () => {
	const n = fake();
	let reloaded = 0;
	const s = make(n, {
		async reload() {
			reloaded++;
		},
	});
	await s.sync();
	expect(await reloadNativeSurface("fake", "f1")).toBe(true);
	expect(reloaded).toBe(1);
	expect(await reloadNativeSurface("fake", "nope")).toBe(false);
	expect(await reloadNativeSurface("fake", null)).toBe(false);
	await s.dispose();
	expect(await reloadNativeSurface("fake", "f1")).toBe(false);
});

// Incident (2026-10-26): a terminal of the `ac` project stayed on screen over the `analisa-ai` space. Whatever leaves a
// native view without an owner, the reconciler must take it off the screen. Per kind: the adapter says what an orphan
// is (a terminal is closed, a browser parked), the core finds them and re-syncs the owned ones.
function kindHost(live: string[], over = {}) {
	const handled: string[] = [];
	const off = registerNativeKind({
		kind: "fake",
		list: async () => live,
		orphan: async (id) => void handled.push(id),
		...over,
	});
	return { handled, off };
}

test("owner-less views of a kind are handed to that kind's orphan handler; owned views only re-sync", async () => {
	const n = fake();
	const s = make(n);
	await s.sync();
	const before = n.of("fake_bounds").length;
	const { handled, off } = kindHost(["f1", "ghost"]);
	const r = await reconcileNatives();
	off();
	expect(r).toEqual({ handled: { fake: ["ghost"] } });
	expect(handled).toEqual(["ghost"]);
	await s.settled();
	expect(n.of("fake_bounds").length).toBe(before + 1);
});

test("a kind whose list fails (a platform without it) is skipped; the others still reconcile", async () => {
	const a = kindHost([], { list: async () => Promise.reject(new Error("unsupported")) });
	const seen: string[] = [];
	const off2 = registerNativeKind({
		kind: "other",
		list: async () => ["x"],
		orphan: async (id) => void seen.push(id),
	});
	const r = await reconcileNatives();
	a.off();
	off2();
	expect(r.handled).toEqual({ other: ["x"] });
});

// Before the workspace has loaded (boot, a webview reload) no component owns anything yet: closing then would kill
// every live view the page is about to re-adopt.
test("not ready: nothing is handled and nothing is listed", async () => {
	let listed = 0;
	const { handled, off } = kindHost(["t1"], { list: async () => (listed++, ["t1"]) });
	const r = await reconcileNatives({ ready: false });
	off();
	expect(r).toEqual({ handled: {} });
	expect(listed).toBe(0);
	expect(handled).toEqual([]);
});

// ── Loading: one indicator, owned by the core, in the strip above the view; adapters only report.
test("the surface owns a 2px strip above the native view and shows it only after the show delay", async () => {
	const n = fake();
	const clock = (() => {
		const timers = new Map();
		let next = 1;
		return {
			set: (fn, ms) => (timers.set(next, { fn, ms }), next++),
			clear: (id) => timers.delete(id),
			fire: (ms) => {
				for (const [id, t] of [...timers]) if (t.ms <= ms) (timers.delete(id), t.fn());
			},
		};
	})();
	const s = make(n, {}, { timers: clock });
	await s.sync();
	const bar = state.slot.querySelector(".native-load");
	expect(bar).not.toBeNull();
	expect(bar.dataset.on).toBe("false");
	expect(bar.style.height).toBe(`${STRIP}px`);
	expect(bar.style.top).toBe("0px"); // the strip sits at the slot's top edge, the view starts STRIP below it
	s.loading.start();
	expect(bar.dataset.on).toBe("false"); // a fast load never flashes
	clock.fire(SHOW_AFTER_MS);
	expect(bar.dataset.on).toBe("true");
	s.loading.finish();
	expect(bar.dataset.on).toBe("false");
	await s.dispose();
	expect(state.slot.querySelector(".native-load")).toBeNull();
});

test("a load that stalls ends the bar and tells the adapter (the browser turns it into its error screen)", async () => {
	const n = fake();
	const stalled = [];
	const timers = new Map();
	let next = 1;
	const clock = {
		set: (fn, ms) => (timers.set(next, { fn, ms }), next++),
		clear: (id) => timers.delete(id),
	};
	const s = make(n, { onStall: () => void stalled.push(1), stallMs: 500 }, { timers: clock });
	await s.sync();
	s.loading.start();
	const stall = [...timers.values()].find((t) => t.ms === 500);
	expect(stall).toBeDefined();
	stall.fn();
	expect(stalled).toHaveLength(1);
	expect(s.loading.state().active).toBe(false);
	expect(STALL_MS).toBeGreaterThan(500); // the default stays the browser's 15 s
});

test("opening a surface is itself a load (so a slow native open shows the bar), ended when the open settles", async () => {
	const n = fake({ hold: ["fake_open"] });
	const s = make(n);
	void s.sync();
	await n.reached("fake_open");
	expect(s.loading.state().active).toBe(true);
	n.release("fake_open");
	await s.settled();
	expect(s.loading.state().active).toBe(false);
});

test("the native rect always excludes the strip, loading or not, so the layout never jumps when a load starts", async () => {
	const n = fake();
	const s = make(n);
	await s.sync();
	s.loading.start();
	await s.sync();
	const [a, b] = n.of("fake_bounds");
	expect(a.args.y).toBe(b.args.y);
	expect(a.args.h).toBe(b.args.h);
});
