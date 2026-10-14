// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import atlas from "../themes/atlas.json";
import dracula from "../themes/dracula.json";
import one from "../themes/one.json";
import { contrast, fromOklch, mix, toOklch } from "../theme/color.ts";
import { complete } from "../theme/derive.ts";
import { parseFamily } from "../theme/schema.ts";
import {
	accentOf,
	accentTextOf,
	directionOf,
	hueOf,
	mountedWindow,
	neighbour,
	onAccent,
	spreadHues,
	tintOf,
} from "./space.ts";
import { MAX_HISTORY, canGo, emptyHistory, step, visit } from "./history.ts";
import { inHotZone, inPanel, peekAction } from "./peek.ts";

test("a space's hue is stable, in range, and differs between ids", () => {
	expect(hueOf("proj-a")).toBe(hueOf("proj-a"));
	for (const id of ["a", "b", "/Users/x/code/app", ""]) {
		expect(hueOf(id)).toBeGreaterThanOrEqual(0);
		expect(hueOf(id)).toBeLessThan(360);
	}
	expect(hueOf("shop")).not.toBe(hueOf("atlas"));
});

test("accent differs by theme lightness and honours a hue override", () => {
	expect(accentOf("x", false)).not.toBe(accentOf("x", true));
	expect(accentOf("x", false, 200)).toBe("oklch(0.62 0.17 200)");
	expect(tintOf("x", true, 10)).toContain(" 10 / ");
});

// Incident (measured by `atlas desktop debug contrast`): the per-space accent is one fixed lightness (0.62 light), so
// as TEXT (port chips, active toggles) it read 2.3:1 on a raised card. Text needs its own lightness per surface.
test("accentTextOf is AA on every chrome surface of every bundled theme, for every hue", () => {
	const themes = [dracula, atlas, one].flatMap((f) => parseFamily(f).themes);
	const fails = [];
	for (const t of themes) {
		const s = complete(t.style, t.appearance);
		const surfaces = [
			"background",
			"editor.background",
			"surface.background",
			"elevated_surface.background",
		].map((k) => s[k].slice(0, 7));
		for (let hue = 0; hue < 360; hue += 5) {
			const c = accentTextOf(t.appearance === "dark", hue, surfaces);
			for (const bg of surfaces)
				if (contrast(c, bg) < 4.5) fails.push(`${t.name} hue ${hue} on ${bg}: ${contrast(c, bg).toFixed(2)}`);
		}
	}
	expect(fails).toEqual([]);
	// Exhaustive (every theme x surface x hue): 8 s on a loaded release gate, over bun's 5 s default.
}, 30_000);

test("accentTextOf keeps the space's hue (it is the same accent, only moved along lightness)", () => {
	for (const hue of [25, 100, 145, 255, 330]) {
		const o = toOklch(accentTextOf(false, hue, ["#fafafa", "#ffffff"]));
		expect(Math.abs(o.h - hue)).toBeLessThan(2);
	}
});

test("spreadHues keeps spaces visually apart and is deterministic", () => {
	const ids = Array.from({ length: 10 }, (_, i) => `repo-${i}`);
	const a = spreadHues(ids);
	expect(spreadHues(ids)).toEqual(a);
	const hues = Object.values(a);
	for (let i = 0; i < hues.length; i++)
		for (let j = i + 1; j < hues.length; j++) {
			const d = Math.abs(hues[i] - hues[j]);
			expect(Math.min(d, 360 - d)).toBeGreaterThanOrEqual(24);
		}
});

test("neighbour wraps and handles a single or unknown space", () => {
	const ids = ["a", "b", "c"];
	expect(neighbour(ids, "a", 1)).toBe("b");
	expect(neighbour(ids, "c", 1)).toBe("a");
	expect(neighbour(ids, "a", -1)).toBe("c");
	expect(neighbour(ids, null, 1)).toBe("a");
	expect(neighbour(ids, "gone", -1)).toBe("c");
	expect(neighbour(["a"], "a", 1)).toBeNull();
	expect(neighbour([], null, 1)).toBeNull();
});

test("switch direction follows the sidebar order", () => {
	const ids = ["a", "b", "c"];
	expect(directionOf(ids, "a", "c")).toBe(1);
	expect(directionOf(ids, "c", "a")).toBe(-1);
	expect(directionOf(ids, null, "b")).toBe(1);
});

test("history: visits, back/forward, dedupe, forward is dropped on a new visit", () => {
	let h = emptyHistory();
	const alive = () => true;
	h = visit(h, { worktreeId: "w1", tabId: "t1" });
	h = visit(h, { worktreeId: "w1", tabId: "t1" });
	h = visit(h, { worktreeId: "w1", tabId: "t2" });
	h = visit(h, { worktreeId: "w2", tabId: "t3" });
	expect(h.entries).toHaveLength(3);
	expect(canGo(h, 1, alive)).toBe(false);
	const back = step(h, -1, alive);
	expect(back.to).toEqual({ worktreeId: "w1", tabId: "t2" });
	expect(canGo(back.history, 1, alive)).toBe(true);
	const branched = visit(back.history, { worktreeId: "w9", tabId: null });
	expect(branched.entries.map((e) => e.worktreeId)).toEqual(["w1", "w1", "w9"]);
});

test("history skips closed targets and is bounded", () => {
	let h = emptyHistory();
	h = visit(h, { worktreeId: "a", tabId: "1" });
	h = visit(h, { worktreeId: "gone", tabId: "2" });
	h = visit(h, { worktreeId: "b", tabId: "3" });
	const r = step(h, -1, (v) => v.worktreeId !== "gone");
	expect(r.to.worktreeId).toBe("a");
	expect(step(r.history, -1, () => true)).toBeNull();
	let big = emptyHistory();
	for (let i = 0; i < MAX_HISTORY + 20; i++) big = visit(big, { worktreeId: "w", tabId: `${i}` });
	expect(big.entries).toHaveLength(MAX_HISTORY);
	expect(big.at).toBe(MAX_HISTORY - 1);
});

test("edge peek: hot zone, panel hit-test and intent transitions", () => {
	expect(inHotZone("left", 4, 1000)).toBe(true);
	expect(inHotZone("left", 20, 1000)).toBe(false);
	expect(inHotZone("right", 995, 1000)).toBe(true);
	expect(inPanel("left", 200, 1000, 260)).toBe(true);
	expect(inPanel("left", 400, 1000, 260)).toBe(false);
	expect(inPanel("right", 800, 1000, 260)).toBe(true);
	expect(peekAction({ open: false }, { t: "move", hot: true, over: false })).toEqual({ do: "schedule-open" });
	expect(peekAction({ open: false }, { t: "move", hot: false, over: false })).toEqual({ do: "cancel" });
	expect(peekAction({ open: true }, { t: "move", hot: false, over: true })).toEqual({ do: "cancel" });
	expect(peekAction({ open: true }, { t: "move", hot: false, over: false })).toEqual({
		do: "schedule-close",
	});
	expect(peekAction({ open: true }, { t: "force-close" })).toEqual({ do: "close-now" });
});

// Incident (contrast audit): the monogram letter of a project tile (and every user-colour swatch) was `color: white`
// on the space accent: at the dark tone (L .74) that is ~2.2:1 for most hues. The letter reads 4.5:1 against BOTH
// ends of the tile gradient (`color-mix(accent 82%, white)` -> accent) for every hue and both tones.
describe("onAccent", () => {
	const ends = (dark: boolean, hue: number) => {
		const c = fromOklch({ ...(dark ? { l: 0.74, c: 0.14 } : { l: 0.62, c: 0.17 }), h: hue });
		return [c, mix(c, "#ffffff", 0.18)];
	};
	for (const dark of [false, true])
		test(`${dark ? "dark" : "light"} tone: every hue (0-359) has a letter colour that reads 4.5 on the whole tile`, () => {
			const bad = [];
			for (let h = 0; h < 360; h++) {
				const fg = onAccent(dark, h);
				for (const bg of ends(dark, h))
					if (contrast(fg, bg) < 4.5) bad.push(`h${h} ${fg} on ${bg}: ${contrast(fg, bg).toFixed(2)}`);
			}
			// not every hue can reach 4.5 with black/white at these tones: those are the ones the tone must be fixed for
			expect(bad).toEqual([]);
		});
});

test("the identity swatches (oklch .68 .16 h) take a black check on EVERY hue", () => {
	for (let h = 0; h < 360; h++) {
		const fill = fromOklch({ l: 0.68, c: 0.16, h });
		expect([h, contrast("#000000", fill) >= 4.5]).toEqual([h, true]);
	}
});

// Selection stall (13 spaces, measured): a layer mounting inside the commit costs ~17 ms p50 / 54 ms tail. The set of
// mounted spaces is therefore a pure function of the order, so the pager can keep it stable across a commit.
describe("mountedWindow: which spaces stay mounted around the active one", () => {
	const ids = ["a", "b", "c", "d", "e"];
	test("active plus one neighbour each side, in sidebar order", () => {
		expect(mountedWindow(ids, "c")).toEqual(["b", "c", "d"]);
	});
	test("wraps at the ends, still in sidebar order", () => {
		expect(mountedWindow(ids, "a")).toEqual(["a", "b", "e"]);
		expect(mountedWindow(ids, "e")).toEqual(["a", "d", "e"]);
	});
	test("two spaces: both, once each; one space: just it", () => {
		expect(mountedWindow(["a", "b"], "a")).toEqual(["a", "b"]);
		expect(mountedWindow(["a"], "a")).toEqual(["a"]);
	});
	test("no active space (or one that is gone) mounts nothing", () => {
		expect(mountedWindow(ids, null)).toEqual([]);
		expect(mountedWindow(ids, "zzz")).toEqual([]);
		expect(mountedWindow([], "a")).toEqual([]);
	});
	test("a selection between adjacent spaces changes at most one mounted layer", () => {
		for (let i = 0; i < ids.length; i++) {
			const from = mountedWindow(ids, ids[i]);
			const to = mountedWindow(ids, ids[(i + 1) % ids.length]);
			expect(to.filter((x) => !from.includes(x)).length).toBeLessThanOrEqual(1);
		}
	});
});

import { edgeNeighbour } from "./space.ts";

test("swiping does not wrap: past the first or last space there is nothing to go to (only a rubber band)", () => {
	const ids = ["a", "b", "c"];
	expect(edgeNeighbour(ids, "a", -1)).toBeNull();
	expect(edgeNeighbour(ids, "c", 1)).toBeNull();
	expect(edgeNeighbour(ids, "b", -1)).toBe("a");
	expect(edgeNeighbour(ids, "b", 1)).toBe("c");
	// one space, an unknown current space: nowhere to swipe
	expect(edgeNeighbour(["a"], "a", 1)).toBeNull();
	expect(edgeNeighbour(ids, "zz", 1)).toBeNull();
	expect(edgeNeighbour(ids, null, 1)).toBeNull();
});

import { stableWindow } from "./space.ts";

// Incident (selection bench, 13 spaces, A->B->A between adjacent spaces): `mountedWindow` is active +/- 1, so every
// selection unmounted one layer and mounted another (~260 nodes inside the commit, 30 ms stalls on 1 selection in 4).
// Going back and forth between two spaces must not remount anything: a layer that is still within two places of the
// active space stays mounted (and is never needed on a path the user is walking), and the ring at distance two is
// what the pager mounts while idle, so that the next step's layer already exists when the commit happens.
describe("stableWindow: the mounted set does not churn when the user steps back and forth", () => {
	const ids = ["a", "b", "c", "d", "e", "f", "g"];
	test("the first window is exactly active +/- 1 (nothing is mounted ahead of the user)", () => {
		expect(stableWindow(ids, "d", [])).toEqual(["c", "d", "e"]);
		expect(stableWindow(ids, null, ["a"])).toEqual([]);
	});
	test("a layer that was mounted and is within two places of the new active space is kept", () => {
		// d -> e: c is now two away (kept); f is one away (needed)
		expect(stableWindow(ids, "e", ["c", "d", "e"])).toEqual(["c", "d", "e", "f"]);
		// and back to d: nothing is mounted or unmounted (f is two away and kept)
		expect(stableWindow(ids, "d", ["c", "d", "e", "f"])).toEqual(["c", "d", "e", "f"]);
	});
	test("a layer more than two places away is dropped, so the set stays bounded", () => {
		expect(stableWindow(ids, "g", ["c", "d", "e", "f"])).toEqual(["a", "e", "f", "g"]);
		expect(stableWindow(ids, "a", ["d", "e"])).toEqual(["a", "b", "g"]);
	});
	test("walking back and forth between two spaces never changes the set after the first step each way", () => {
		let held = stableWindow(ids, "a", []);
		const seen: string[][] = [];
		for (let i = 0; i < 6; i++) {
			held = stableWindow(ids, i % 2 === 0 ? "b" : "a", held);
			seen.push(held);
		}
		expect(new Set(seen.slice(2).map((s) => s.join()))).toEqual(new Set([seen[2]!.join()]));
		expect(seen[2]!.length).toBeLessThanOrEqual(5);
	});
	test("with few spaces everything is mounted once, in sidebar order", () => {
		expect(stableWindow(["a", "b"], "a", [])).toEqual(["a", "b"]);
		expect(stableWindow(["a", "b", "c"], "b", ["a", "b", "c"])).toEqual(["a", "b", "c"]);
	});
});
