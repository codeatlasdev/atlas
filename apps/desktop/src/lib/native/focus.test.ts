// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, expect, test } from "bun:test";
import { startNativeFocus } from "./focus.ts";

// Incident (2026-10-26): open the "+" menu over a terminal (or a browser tab), click the terminal: the menu closed and
// the kit restored focus to the "+" button AFTER the press, so keys never reached the terminal. A press on a native
// surface hands it focus, and a focus restore that lands elsewhere right after is undone. Every kind, by registration.
let stop = () => {};
afterEach(() => {
	stop();
	document.body.innerHTML = "";
});

function setup() {
	const slot = document.createElement("div");
	const trigger = document.createElement("button");
	const other = document.createElement("input");
	document.body.append(slot, trigger, other);
	const focused: string[] = [];
	const s = { slot: () => slot, focus: () => void focused.push("surface") };
	stop = startNativeFocus({ surfaces: () => [s], now: () => t });
	return { slot, trigger, other, focused };
}
let t = 0;

test("a press on a native slot focuses its surface", () => {
	const { slot, focused } = setup();
	slot.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
	expect(focused).toEqual(["surface"]);
});

test("a focus restored to the menu trigger right after the press is given back to the surface", async () => {
	const { slot, trigger, focused } = setup();
	t = 0;
	slot.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
	t = 40;
	trigger.focus();
	await Promise.resolve();
	expect(focused).toEqual(["surface", "surface"]);
});

test("a deliberate focus elsewhere later (the user clicked an input) is left alone", async () => {
	const { slot, other, focused } = setup();
	t = 0;
	slot.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
	t = 2000;
	other.focus();
	await Promise.resolve();
	expect(focused).toEqual(["surface"]);
});

test("a press on the page itself claims nothing", () => {
	const { trigger, focused } = setup();
	trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
	trigger.focus();
	expect(focused).toEqual([]);
});

// Seen in the real app (agent instance): after the menu closed, focus ended on <body>, not on the trigger. A blur to
// nothing fires no focusin, so the surface must also be refocused when its focus is simply dropped during the grace.
test("focus dropped to the body right after the press goes back to the surface", async () => {
	const { slot, focused } = setup();
	const sink = document.createElement("textarea");
	slot.append(sink);
	t = 0;
	slot.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
	sink.focus();
	t = 60;
	sink.blur();
	await new Promise((r) => setTimeout(r, 0));
	expect(focused).toEqual(["surface", "surface"]);
});

// Seen in the real app: the press lands on the slot's parent (`.wrap`; the slot itself lets the pointer through), so
// matching by DOM containment found no surface. A surface owns a press by where it lands, not by which element got it.
test("a press inside the slot's rect, delivered to an ancestor, focuses the surface", () => {
	const { slot, focused } = setup();
	const wrap = document.createElement("div");
	document.body.append(wrap);
	wrap.append(slot);
	slot.getBoundingClientRect = () => new DOMRect(100, 100, 400, 300);
	wrap.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 200, clientY: 200 }));
	expect(focused).toEqual(["surface"]);
	wrap.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 50, clientY: 50 }));
	expect(focused).toEqual(["surface"]);
});
