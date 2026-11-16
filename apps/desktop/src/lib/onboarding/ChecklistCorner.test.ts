// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";

// Incident (maintainer screenshot, 2026-11-15): a toast sat on the open "Getting started" card. The card is a
// corner card now: a toast folds it to its header (title + progress), it opens again when the corner is free, and
// the header is a real button. Real Checklist, real column, real coordinator.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => ({}) }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
await (await import("../../../test/uiKit.ts")).useRealKit("Kbd");
const { flushSync, mount, tick, unmount } = await import("svelte");
const { default: Harness } = await import("../../../test/ChecklistCornerHarness.svelte");
const { corner } = await import("#lib/shell/corner.svelte.ts");
const { onboarding } = await import("./state.svelte.ts");

let done;
let h;
const realMatchMedia = window.matchMedia;
beforeEach(() => {
	window.matchMedia = (q) => ({
		matches: q.includes("reduce"),
		addEventListener() {},
		removeEventListener() {},
		media: q,
	});
	onboarding.loaded = true;
	onboarding.state = { done: true, toured: true, checklist: { done: ["project"], dismissed: false } };
	const target = document.createElement("div");
	document.body.append(target);
	h = flushSync(() => mount(Harness, { target }));
	done = () => (unmount(h), target.remove());
});
afterEach(() => {
	done?.();
	corner.reset();
	window.matchMedia = realMatchMedia;
});

const settle = async () => (
	flushSync(),
	await tick(),
	await new Promise((r) => setTimeout(r, 30)),
	flushSync()
);
const card = () => document.querySelector('#atlas-corner [aria-label="Getting started"]');
const head = () => card().querySelector("button[aria-expanded]");
const items = () => card().querySelectorAll("ul li");

test("the checklist is a named complementary card with its items and a progress header", async () => {
	await settle();
	expect(card().getAttribute("role")).toBe("complementary");
	expect(items()).toHaveLength(6);
	expect(head().getAttribute("aria-expanded")).toBe("true");
	expect(head().textContent).toContain("1/6");
	expect(card().querySelector('button[aria-label="Dismiss checklist"]')).not.toBeNull();
});

test("a toast folds it to the header, and it opens again when the toast is gone", async () => {
	await settle();
	h.toast(true);
	await settle();
	expect(items()).toHaveLength(0);
	expect(head().getAttribute("aria-expanded")).toBe("false");
	expect(head().textContent).toContain("Getting started");
	expect(head().textContent).toContain("1/6");
	h.toast(false);
	await settle();
	expect(items()).toHaveLength(6);
});

test("one the user closed by hand stays closed after the toast leaves", async () => {
	await settle();
	head().click();
	await settle();
	expect(items()).toHaveLength(0);
	h.toast(true);
	await settle();
	h.toast(false);
	await settle();
	expect(items()).toHaveLength(0);
	expect(head().getAttribute("aria-expanded")).toBe("false");
});

test("Esc on the open card folds it; the choice is remembered", async () => {
	await settle();
	card().dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
	await settle();
	expect(items()).toHaveLength(0);
	expect(localStorage.getItem("atlas.checklistOpen")).toBe("0");
});

test("a suggestion sits above the checklist and does not fold it", async () => {
	await settle();
	h.suggestion(true);
	await settle();
	const cards = [...document.querySelectorAll("#atlas-corner [data-corner-card]")];
	const bottomUp = cards
		.sort((a, b) => Number(a.style.order) - Number(b.style.order))
		.map((c) => c.dataset.cornerCard);
	expect(bottomUp).toEqual(["checklist", "tip"]);
	expect(items()).toHaveLength(6);
});
