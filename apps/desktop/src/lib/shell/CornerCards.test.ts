// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, test } from "bun:test";

// Incident (maintainer screenshot, 2026-11-15): toasts and the Getting started card piled up in the corner. What a
// reader (and a screen reader) gets from the corner is its roles, names, keyboard and what is on screen; real
// column, real cards, real coordinator. No toast store here: the plan is what is under test, the kit queue only feeds it.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
const { flushSync, mount, tick, unmount } = await import("svelte");
const { default: Harness } = await import("../../../test/CornerHarness.svelte");
const { corner } = await import("./corner.svelte.ts");
const { cornerFlow } = await import("./cornerMotion.ts");

let done;
let h;
let reduce = true;
const realMatchMedia = window.matchMedia;
beforeEach(() => {
	// Reduce motion: DOM changes are immediate, so a test does not wait for a leave animation.
	window.matchMedia = (q) => ({
		matches: reduce && q.includes("reduce"),
		addEventListener() {},
		removeEventListener() {},
		media: q,
	});
	const target = document.createElement("div");
	document.body.append(target);
	h = flushSync(() => mount(Harness, { target }));
	done = () => (unmount(h), target.remove());
});
afterEach(() => {
	done?.();
	corner.reset();
	window.matchMedia = realMatchMedia;
	reduce = true;
});

const settle = async () => (
	flushSync(),
	await tick(),
	await new Promise((r) => setTimeout(r, 30)),
	flushSync()
);
const col = () => document.getElementById("atlas-corner");
const cards = () => [...col().querySelectorAll("[data-corner-card]")];
const put = async (items) => (h.set(items), await settle());

test("an error is announced assertively, a toast politely, a suggestion not at all; every card has a name", async () => {
	await put([
		{ id: "e", kind: "alert", title: "Could not start web" },
		{ id: "t", kind: "toast", title: "Copied 12 lines" },
	]);
	expect(col().querySelector('[role="alert"]').getAttribute("aria-label")).toBe("Could not start web");
	expect(col().querySelector('[role="status"]').getAttribute("aria-label")).toBe("Copied 12 lines");
	await put([{ id: "s", kind: "suggestion", title: "Enable Claude Code integration" }]);
	const group = col().querySelector('[role="group"]');
	expect(group.getAttribute("aria-label")).toBe("Enable Claude Code integration");
	expect(col().querySelector('[role="alert"], [role="status"]')).toBeNull();
});

test("the column is a named region and a dismiss button is named", async () => {
	await put([{ id: "t", kind: "toast", title: "Saved", ondismiss() {} }]);
	expect(col().getAttribute("role")).toBe("region");
	expect(col().getAttribute("aria-label")).toBeTruthy();
	expect(col().querySelector("button").getAttribute("aria-label")).toBe("Dismiss");
});

test("Esc inside a card dismisses that card only and does not reach the window", async () => {
	const dismissed = [];
	await put([
		{ id: "a", kind: "toast", title: "A", ondismiss: () => dismissed.push("a") },
		{ id: "b", kind: "toast", title: "B", ondismiss: () => dismissed.push("b") },
	]);
	let leaked = 0;
	const spy = () => leaked++;
	window.addEventListener("keydown", spy);
	const first = col().querySelector('[data-corner-card="a"] .card');
	first.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
	window.removeEventListener("keydown", spy);
	expect(dismissed).toEqual(["a"]);
	expect(leaked).toBe(0);
});

test("a card with no dismiss action ignores Esc (it lets the key through)", async () => {
	await put([{ id: "a", kind: "toast", title: "A" }]);
	let leaked = 0;
	const spy = () => leaked++;
	window.addEventListener("keydown", spy);
	col()
		.querySelector(".card")
		.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
	window.removeEventListener("keydown", spy);
	expect(leaked).toBe(1);
});

test("a suggestion waits while a toast is up and comes back when it leaves", async () => {
	await put([
		{ id: "tip", kind: "suggestion", title: "Enable Claude Code integration" },
		{ id: "t", kind: "toast", title: "Copied" },
	]);
	expect(cards().map((c) => c.dataset.cornerCard)).toEqual(["t"]);
	await put([{ id: "tip", kind: "suggestion", title: "Enable Claude Code integration" }]);
	expect(cards().map((c) => c.dataset.cornerCard)).toEqual(["tip"]);
});

test("only one suggestion at a time, the first to arrive; the next after it is dismissed", async () => {
	await put([
		{ id: "one", kind: "suggestion", title: "One" },
		{ id: "two", kind: "suggestion", title: "Two" },
	]);
	expect(cards().map((c) => c.dataset.cornerCard)).toEqual(["one"]);
	await put([{ id: "two", kind: "suggestion", title: "Two" }]);
	expect(cards().map((c) => c.dataset.cornerCard)).toEqual(["two"]);
});

test("five toasts: three cards and a +2 more row that opens them all and folds back", async () => {
	await put(Array.from({ length: 5 }, (_, i) => ({ id: `t${i}`, kind: "toast", title: `Toast ${i}` })));
	expect(cards().filter((c) => c.dataset.cornerCard !== "corner:more")).toHaveLength(3);
	const more = col().querySelector("button.more");
	expect(more.textContent.trim()).toBe("+2 more");
	expect(more.getAttribute("aria-label")).toBe("Show 2 more notifications");
	more.click();
	await settle();
	expect(cards().filter((c) => c.dataset.cornerCard !== "corner:more")).toHaveLength(5);
	expect(col().querySelector("button.more").textContent.trim()).toBe("Show less");
	col().querySelector("button.more").click();
	await settle();
	expect(cards().filter((c) => c.dataset.cornerCard !== "corner:more")).toHaveLength(3);
});

test("the newest card is nearest the corner and an error outranks plain toasts", async () => {
	await put([
		{ id: "old", kind: "toast", title: "old" },
		{ id: "err", kind: "alert", title: "err" },
		{ id: "new", kind: "toast", title: "new" },
	]);
	const order = Object.fromEntries(cards().map((c) => [c.dataset.cornerCard, Number(c.style.order)]));
	expect(order.new).toBe(0);
	expect(order.err).toBe(1);
	expect(order.old).toBe(2);
});

test("with reduce motion the card is removed at once, no leave animation", async () => {
	await put([{ id: "t", kind: "toast", title: "t" }]);
	expect(cards()).toHaveLength(1);
	h.set([]);
	flushSync();
	expect(cards()).toHaveLength(0);
});

test("reduce motion turns every transition off; otherwise enter and leave have a duration", () => {
	const node = document.createElement("div");
	reduce = true;
	expect(cornerFlow(node).duration).toBe(0);
	reduce = false;
	expect(cornerFlow(node).duration).toBeGreaterThan(0);
	expect(cornerFlow(node, { out: true }).duration).toBeGreaterThan(0);
});
