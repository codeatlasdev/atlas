// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { TermLayout, usable } from "./termLayout.ts";

// Contract: the page never hides a native terminal for an overlay, and every terminal's rect crosses IPC once per
// frame, together. These run the REAL batcher with an injected invoke; the backend half (one CATransaction after the
// page presents) is in crates/atlas-desktop/src/term/{layering,lifecycle}.rs.

interface Body {
	tab: string;
	x: number;
	y: number;
	w: number;
	h: number;
	visible: boolean;
}
interface Answer {
	tab: string;
	presented: boolean;
	dropped: boolean;
}
type Call = { cmd: string; args: { slots: Body[] } };
type Rect = { x: number; y: number; w: number; h: number };

function rig() {
	const calls: Call[] = [];
	let reply = (slots: Body[]): Answer[] =>
		slots.map((s) => ({ tab: s.tab, presented: s.visible, dropped: false }));
	const layout = new TermLayout(async (cmd, args) => {
		const a = args as { slots: Body[] };
		calls.push({ cmd, args: a });
		return reply(a.slots);
	});
	return {
		layout,
		calls,
		setReply: (f: (slots: Body[]) => Answer[]) => (reply = f),
	};
}

function slot(tab: string, over: Partial<{ r: Rect; visible: boolean }> = {}) {
	const state: { r: Rect; visible: boolean } = {
		r: over.r ?? { x: 0, y: 0, w: 100, h: 50 },
		visible: over.visible ?? true,
	};
	const told: boolean[] = [];
	let dropped = 0;
	return {
		state,
		told,
		get dropped() {
			return dropped;
		},
		tab,
		measure: () => state.r,
		visible: () => state.visible,
		onPresented: (p: boolean) => told.push(p),
		onDropped: () => dropped++,
	};
}

test("two terminals are measured together and cross IPC in ONE call", async () => {
	const { layout, calls } = rig();
	layout.add(slot("a"));
	layout.add(slot("b", { r: { x: 100, y: 0, w: 100, h: 50 } }));
	await layout.request();
	expect(calls.length).toBe(1);
	expect(calls[0].cmd).toBe("term_layout");
	expect(calls[0].args.slots.map((s: Body) => s.tab)).toEqual(["a", "b"]);
});

test("every measurement happens before the call: layout is read in one pass, never between writes", async () => {
	const order: string[] = [];
	const layout = new TermLayout(async (cmd) => void order.push(cmd) ?? []);
	for (const t of ["a", "b", "c"])
		layout.add({ ...slot(t), measure: () => (order.push(`measure ${t}`), { x: 0, y: 0, w: 10, h: 10 }) });
	await layout.request();
	expect(order).toEqual(["measure a", "measure b", "measure c", "term_layout"]);
});

test("requests made while a batch is in flight collapse into one more batch", async () => {
	const { layout, calls } = rig();
	const a = slot("a");
	layout.add(a);
	const first = layout.request();
	a.state.r = { x: 5, y: 5, w: 100, h: 50 };
	layout.request();
	layout.request();
	layout.request();
	await first;
	await layout.request();
	expect(calls.length).toBe(2);
	expect(calls[1].args.slots[0].x).toBe(5);
});

test("an unchanged rect is not sent again, yet its slot is still answered (a swipe that lifts over the same rect)", async () => {
	const { layout, calls } = rig();
	const a = slot("a");
	layout.add(a);
	await layout.request();
	await layout.request();
	expect(calls.length).toBe(1);
	expect(a.told).toEqual([true, true]);
});

test("a hidden tab is sent as visible:false with its rect; showing it again is one more call", async () => {
	const { layout, calls } = rig();
	const a = slot("a");
	layout.add(a);
	await layout.request();
	a.state.visible = false;
	await layout.request();
	expect(calls[1].args.slots[0]).toMatchObject({ tab: "a", visible: false, w: 100 });
	a.state.visible = true;
	await layout.request();
	expect(calls[2].args.slots[0].visible).toBe(true);
	expect(a.told).toEqual([true, false, true]);
});

// Blank-terminal audit: a 0x0 slot (a collapsing split, a hidden pane) must park the view rather than shrink its
// surface, and the real rect must bring it back.
test("a zero rect is sent hidden, and the next real rect shows the view again", async () => {
	const { layout, calls } = rig();
	const a = slot("a");
	layout.add(a);
	await layout.request();
	a.state.r = { x: 0, y: 0, w: 0, h: 0 };
	await layout.request();
	expect(calls[1].args.slots[0].visible).toBe(false);
	a.state.r = { x: 0, y: 0, w: 100, h: 50 };
	await layout.request();
	expect(calls[2].args.slots[0].visible).toBe(true);
});

test("NaN and missing rects are never usable", () => {
	expect(usable(null)).toBe(false);
	expect(usable({ x: NaN, y: 0, w: 10, h: 10 })).toBe(false);
	expect(usable({ x: 0, y: 0, w: 0.5, h: 10 })).toBe(false);
	expect(usable({ x: 0, y: 0, w: 10, h: 10 })).toBe(true);
});

// Blank-terminal audit: a dropped view must reach the slot, forget its rect, and be sent again after the reopen.
test("a dropped terminal is reported, forgotten, and its rect is sent again after the reopen", async () => {
	const { layout, calls, setReply } = rig();
	const a = slot("a");
	layout.add(a);
	setReply((slots) => slots.map((s) => ({ tab: s.tab, presented: false, dropped: true })));
	await layout.request();
	expect(a.dropped).toBe(1);
	expect(a.told).toEqual([false]);
	setReply((slots) => slots.map((s) => ({ tab: s.tab, presented: true, dropped: false })));
	await layout.request();
	expect(calls.length).toBe(2);
	expect(a.told).toEqual([false, true]);
});

test("a failed batch is retried in full by the next request (nothing is remembered as sent)", async () => {
	let fail = true;
	const calls: unknown[] = [];
	const layout = new TermLayout(async (_cmd, args) => {
		calls.push(args);
		if (fail) throw new Error("ipc down");
		return (args as { slots: Body[] }).slots.map((s) => ({ tab: s.tab, presented: true, dropped: false }));
	});
	const a = slot("a");
	layout.add(a);
	await expect(layout.request()).rejects.toThrow("ipc down");
	fail = false;
	await layout.request();
	expect(calls.length).toBe(2);
	expect(a.told).toEqual([true]);
});

test("a remounted tab replaces its slot and only the current slot can unregister it", async () => {
	const { layout, calls } = rig();
	const old = slot("t");
	const offOld = layout.add(old);
	const fresh = slot("t");
	layout.add(fresh);
	offOld();
	await layout.request();
	expect(calls.length).toBe(1);
	expect(fresh.told).toEqual([true]);
	expect(old.told).toEqual([]);
});

test("the backend's own presented event is what an unchanged slot repeats", async () => {
	const { layout } = rig();
	const a = slot("a");
	layout.add(a);
	await layout.request();
	layout.presented("a", false); // a rebuilt surface: blank until its first frame
	await layout.request();
	expect(a.told).toEqual([true, false]);
});
