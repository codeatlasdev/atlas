// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { beforeEach, expect, test } from "bun:test";
import { place, spacePager } from "./pager.ts";

/** Let the pager's `await host.flush()` continuations run (microtasks only, no wall clock). */
const settle = async () => {
	for (let i = 0; i < 5; i++) await Promise.resolve();
};

class Layer {
	style = {
		transform: "",
		removeProperty(p) {
			if (p === "transform") this.transform = "";
		},
	};
}

let host, layers, committed, moving, active, go;
beforeEach(() => {
	layers = { a: new Layer(), b: new Layer(), c: new Layer(), z: new Layer() };
	committed = [];
	moving = [];
	go = [];
	active = "a";
	host = {
		layer: (id) => layers[id],
		width: () => 1000,
		ids: () => ["a", "b", "z"],
		active: () => active,
		commit: (id) => {
			active = id;
			committed.push(id);
		},
		moving: (on, keep) => moving.push([on, keep]),
		flush: () => Promise.resolve(),
		// The engine's own programmatic switch (swipe_go): resolves to whether it took it.
		engineGo: async (dir) => (go.push(dir), true),
	};
	spacePager.attach(host)();
	spacePager.attach(host);
});

const f = (seq, amount, stage = "track") => spacePager.frame({ zone: "space", seq, amount, stage });
const c = (seq, dir) => spacePager.commit({ zone: "space", seq, dir });

// ---- pure placement ----

test("place: the origin sits at -amount·W, the neighbour in the direction of travel at (dir - amount)·W", () => {
	const ids = ["a", "b", "z"];
	expect(place("a", 0.25, ids, 1000)).toEqual([
		{ id: "a", x: -250 },
		{ id: "b", x: 750 },
	]);
	// Negative amount heads to the previous space; at the first space it wraps to the last (sidebar wraps).
	expect(place("a", -0.5, ids, 1000)).toEqual([
		{ id: "a", x: 500 },
		{ id: "z", x: -500 },
	]);
	// At rest nothing but the origin: no neighbour is positioned.
	expect(place("a", 0, ids, 1000)).toEqual([{ id: "a", x: -0 }]);
});

test("place: an explicit target (a jump to a non-adjacent space) is the incoming layer", () => {
	expect(place("a", 0.5, ["a", "b", "c", "z"], 1000, "c")).toEqual([
		{ id: "a", x: -500 },
		{ id: "c", x: 500 },
	]);
});

// ---- engine frames drive the layers ----

test("a frame moves the layers and flags the gesture as moving, with no state change", () => {
	f(1, 0.25);
	expect(layers.a.style.transform).toBe("translate3d(-250px,0,0)");
	expect(layers.b.style.transform).toBe("translate3d(750px,0,0)");
	expect(spacePager.active).toBe(true);
	expect(committed).toEqual([]);
	expect(moving[0]).toEqual([true, ["a", "b"]]);
});

test("heading flips without a retarget: the other neighbour is placed, the first one is released", () => {
	f(1, 0.3);
	f(1, -0.2);
	expect(layers.a.style.transform).toBe("translate3d(200px,0,0)");
	expect(layers.z.style.transform).toBe("translate3d(-800px,0,0)");
	expect(layers.b.style.transform).toBe("");
});

test("the commit selects the neighbour once, whatever the settle frames do after it", () => {
	f(1, 0.4);
	c(1, 1);
	expect(committed).toEqual(["b"]);
	f(1, 0.8, "settle");
	c(1, 1); // a duplicate delivery of the same decision
	expect(committed).toEqual(["b"]);
});

test("a stale commit (older seq than one already applied) is ignored", () => {
	f(2, 0.4);
	c(2, 1);
	c(1, -1);
	expect(committed).toEqual(["b"]);
});

test("a cancel (dir 0) selects nothing and the rest frame returns the layers to rest", async () => {
	f(1, 0.2);
	c(1, 0);
	f(1, 0.01, "settle");
	f(1, 0, "rest");
	await settle();
	expect(committed).toEqual([]);
	expect(layers.a.style.transform).toBe("");
	expect(layers.b.style.transform).toBe("");
	expect(spacePager.active).toBe(false);
	expect(moving.at(-1)).toEqual([false]);
});

test("the rest frame of a commit clears the transforms only after Svelte applied the selection", async () => {
	f(1, 0.6);
	c(1, 1);
	f(1, 1, "rest");
	expect(layers.a.style.transform).not.toBe(""); // still holding the end pose
	await settle();
	expect(layers.a.style.transform).toBe("");
	expect(layers.b.style.transform).toBe("");
	expect(moving.at(-1)).toEqual([false]);
});

test("a new gesture during the settle continues from the committed page: no jump, no stuck transform", async () => {
	f(1, 0.6);
	c(1, 1);
	f(1, 0.85, "settle");
	// seq 2 rebased on the committed page `b`: b is the origin now, amount -0.2 heads back to `a`.
	f(2, -0.15);
	expect(layers.b.style.transform).toBe("translate3d(150px,0,0)");
	expect(layers.a.style.transform).toBe("translate3d(-850px,0,0)");
	f(2, 0, "rest");
	await settle();
	expect(layers.a.style.transform).toBe("");
	expect(layers.b.style.transform).toBe("");
});

test("frames of an older generation never move layers once a newer one began", () => {
	f(2, 0.3);
	f(1, 0.9, "settle");
	expect(layers.a.style.transform).toBe("translate3d(-300px,0,0)");
});

// ---- programmatic switches ----

test("go() selects now (keyboard consumers see it at once) and asks the engine for the animation", async () => {
	expect(spacePager.go("b")).toBe(true);
	expect(committed).toEqual(["b"]);
	await settle();
	expect(go).toEqual([1]);
});

test("go() frames animate the target layer, whatever its distance, and the engine's commit is not applied twice", async () => {
	spacePager.go("z");
	await settle();
	// seq 1: origin a, target z (jump), engine commit arrives after the self-commit.
	f(1, 0.4, "settle");
	expect(layers.a.style.transform).toBe("translate3d(-400px,0,0)");
	expect(layers.z.style.transform).toBe("translate3d(600px,0,0)");
	c(1, 1);
	expect(committed).toEqual(["z"]);
});

test("go() to the active space or while the engine refuses does nothing visible", async () => {
	expect(spacePager.go("a")).toBe(false);
	host.engineGo = async () => false;
	expect(spacePager.go("b")).toBe(true); // the selection still moves (instantly)
	expect(committed).toEqual(["b"]);
	await settle();
	expect(spacePager.active).toBe(false);
});

test("go(id, origin) animates without committing (the selection already moved)", async () => {
	expect(spacePager.go("b", "a")).toBe(true);
	expect(committed).toEqual([]);
	await settle();
	expect(go).toEqual([1]);
});

// Incident (bench, every commit trace): the worst frame of a gesture came with a 10-19 ms main-thread stall right
// after the first moved frame. `frame()` told the host `moving(true, keep)` on EVERY frame; the space pager turns that
// into `$state` writes (a fresh `kept` array, `moving`, a touch), so the keyed each-block and its derived re-ran per
// frame. The contract in `PagerHost.moving` is "never called per frame": only on a change of state.
test("moving() is told once per gesture and again only when the set of layers it needs changes", () => {
	moving.length = 0;
	f(1, 0.1);
	f(1, 0.2);
	f(1, 0.3);
	f(1, 0.4);
	expect(moving).toEqual([[true, ["a", "b"]]]);
	// the finger crosses to the other side: a different neighbour is needed, so it is told again
	f(1, -0.1);
	f(1, -0.2);
	expect(moving).toEqual([
		[true, ["a", "b"]],
		[true, ["a", "z"]],
	]);
	// rest: told off once; a second gesture is told on again
	f(1, 0, "rest");
	return settle().then(() => {
		expect(moving.at(-1)).toEqual([false, undefined]);
		const n = moving.length;
		f(2, 0.1);
		f(2, 0.2);
		expect(moving.length).toBe(n + 1);
		expect(moving.at(-1)).toEqual([true, ["a", "b"]]);
	});
});
