// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { beforeEach, expect, test } from "bun:test";
import { placeTabs, tabPager } from "./tabPager.ts";

const settle = async () => {
	for (let i = 0; i < 5; i++) await Promise.resolve();
};

let el, activated, active, order, strip;
beforeEach(() => {
	activated = [];
	active = "t2";
	order = ["t1", "t2", "t3"];
	const mk = () => ({ style: { transform: "", visibility: "" } });
	el = { t1: mk(), t2: mk(), t3: mk() };
	strip = { style: { transform: "" } };
	tabPager.reset();
});

const host = () => ({
	pane: (id) => el[id],
	strip: () => strip,
	width: () => 800,
	order: () => order,
	active: () => active,
	activate: (id) => {
		active = id;
		activated.push(id);
	},
});
const f = (seq, amount, stage = "track") =>
	tabPager.frame("g", host(), { zone: "tab", id: "g", seq, amount, stage });
const c = (seq, dir) => tabPager.commit("g", host(), { zone: "tab", id: "g", seq, dir });

test("placeTabs: the active pane moves with the finger, the tab it would land on comes in beside it", () => {
	expect(placeTabs(["t1", "t2", "t3"], "t2", 0.25, 800)).toEqual([
		{ id: "t2", x: -200 },
		{ id: "t3", x: 600 },
	]);
	expect(placeTabs(["t1", "t2", "t3"], "t2", -0.5, 800)).toEqual([
		{ id: "t2", x: 400 },
		{ id: "t1", x: -400 },
	]);
});

test("placeTabs: no neighbour on that side is a rubber band over an empty body (only the active pane moves)", () => {
	expect(placeTabs(["t1", "t2", "t3"], "t3", 0.1, 800)).toEqual([{ id: "t3", x: -80 }]);
	expect(placeTabs(["t1", "t2", "t3"], "t1", -0.1, 800)).toEqual([{ id: "t1", x: 80 }]);
});

test("frames move panes, show the incoming one and nudge the strip", () => {
	f(1, 0.25);
	expect(el.t2.style.transform).toBe("translate3d(-200px,0,0)");
	expect(el.t3.style.transform).toBe("translate3d(600px,0,0)");
	expect(el.t3.style.visibility).toBe("visible");
	expect(strip.style.transform).toBe("translate3d(-6px,0,0)");
	expect(activated).toEqual([]);
});

test("the commit activates the neighbour once; a duplicate or stale decision does nothing", () => {
	f(2, 0.6);
	c(2, 1);
	c(2, 1);
	c(1, -1);
	expect(activated).toEqual(["t3"]);
});

test("rest clears every transform and visibility, after a commit and after a cancel", async () => {
	f(1, 0.6);
	c(1, 1);
	f(1, 1, "rest");
	await settle();
	for (const id of ["t1", "t2", "t3"]) {
		expect(el[id].style.transform).toBe("");
		expect(el[id].style.visibility).toBe("");
	}
	expect(strip.style.transform).toBe("");
	f(2, 0.2);
	c(2, 0);
	f(2, 0, "rest");
	await settle();
	expect(el.t3.style.transform).toBe("");
	expect(activated).toEqual(["t3"]);
});

test("a gesture preempting a settle starts from the tab it was heading to", async () => {
	f(1, 0.6);
	c(1, 1);
	f(1, 0.9, "settle");
	// seq 2 is relative to the now-active t3; -0.1 heads back toward t2
	f(2, -0.1);
	expect(el.t3.style.transform).toBe("translate3d(80px,0,0)");
	expect(el.t2.style.transform).toBe("translate3d(-720px,0,0)");
});

test("two groups never share generation state", () => {
	tabPager.frame("g", host(), { zone: "tab", id: "g", seq: 5, amount: 0.1, stage: "track" });
	tabPager.frame("h", host(), { zone: "tab", id: "h", seq: 1, amount: 0.1, stage: "track" });
	expect(el.t2.style.transform).not.toBe("");
});

// Regression (#69): terminals and browsers are native views above the webview; a CSS transform cannot move them, so
// during a tab swipe they stayed at the old rect while the DOM pane slid, and a sync mid-gesture measured the rect
// WITH the in-flight translate3d and left the native view shoved sideways after `rest`. The pager must mark the
// panes it moves (`data-swiping`) for exactly the generation, so the native views are parked (never moved per frame)
// and `watchSlot` re-syncs them at the untransformed rect when the mark is lifted.
const marked = () => ["t1", "t2", "t3"].filter((id) => el[id].dataset.swiping === "");
beforeEach(() => {
	for (const id of ["t1", "t2", "t3"]) {
		const dataset = {};
		el[id].dataset = dataset;
		el[id].toggleAttribute = (name, on) => {
			if (name === "data-swiping") on ? (dataset.swiping = "") : delete dataset.swiping;
		};
	}
});

test("the panes in flight are marked swiping from the first frame, and only those", () => {
	f(1, 0.25);
	expect(marked()).toEqual(["t2", "t3"]);
	f(1, 0.4);
	expect(marked()).toEqual(["t2", "t3"]);
});

test("a swipe that turns around moves the mark to the new neighbour and drops it from the old one", () => {
	f(1, 0.25);
	f(1, -0.25);
	expect(marked()).toEqual(["t1", "t2"]);
});

test("rest lifts the mark after a commit and after a cancel", async () => {
	f(1, 0.6);
	c(1, 1);
	f(1, 1, "rest");
	await settle();
	expect(marked()).toEqual([]);
	f(2, -0.2); // t3 is last now: the neighbour is t2
	expect(marked()).toEqual(["t2", "t3"]);
	c(2, 0);
	f(2, 0, "rest");
	await settle();
	expect(marked()).toEqual([]);
});

test("the mark never appears for a rest-only generation (a no-op cannot park a terminal)", async () => {
	f(1, 0, "rest");
	await settle();
	expect(marked()).toEqual([]);
});
