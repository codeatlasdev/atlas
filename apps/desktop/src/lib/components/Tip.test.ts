// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, mock, test } from "bun:test";
import { resolve } from "node:path";

// `Tip` is the kit's Tooltip plus the command's current key binding ("Toggle left dock  ⌘B"). It used to build the
// tooltip lazily on the first pointer/focus event, because a closed kit 0.7.1 tooltip cost an imperative root under
// <body> (codeatlasdev/ui#27) and a space layer carries a dozen. Kit 2.2 renders nothing while closed (kitOverlays.test.ts),
// so the control is the kit's own trigger from the first paint: never re-created, so keyboard focus can never be lost.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
await (await import("../../../test/uiKit.ts")).useRealKit("Tooltip");
let binding = "⌘B";
mock.module(resolve(import.meta.dir, "../keymap.svelte.ts"), () => ({ shortcutFor: () => binding }));

const { flushSync, mount, tick, unmount } = await import("svelte");
const { default: Harness } = await import("../../../test/TipHarness.svelte");

function setup(props = {}) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() =>
		mount(Harness, {
			target,
			props: { label: "Toggle left dock", command: "workspace::ToggleLeftDock", ...props },
		}),
	);
	return { target, done: () => (unmount(app), target.remove()) };
}
const tooltip = () => document.querySelector(".ca-tooltip-content");

test("a Tip renders its control and puts nothing under <body> until it is shown", () => {
	const before = document.body.childNodes.length;
	const { target, done } = setup();
	expect(target.querySelector(".ctl")).not.toBeNull();
	expect(tooltip()).toBeNull();
	expect(document.body.childNodes.length).toBe(before + 1); // only the test's own mount point
	done();
});

test("keyboard focus shows the label with the command's current key binding", async () => {
	const { target, done } = setup();
	target.querySelector(".ctl").focus();
	target.querySelector(".ctl").dispatchEvent(new FocusEvent("focus"));
	flushSync();
	await tick();
	expect(tooltip()?.textContent).toBe("Toggle left dock  ⌘B");
	done();
});

test("without a command the tooltip is the bare label", async () => {
	const { target, done } = setup({ command: undefined });
	target.querySelector(".ctl").dispatchEvent(new FocusEvent("focus"));
	flushSync();
	await tick();
	expect(tooltip()?.textContent).toBe("Toggle left dock");
	done();
});

test("showing the tooltip never re-creates the control: it keeps keyboard focus and its place", async () => {
	const { target, done } = setup();
	const before = target.querySelector(".ctl");
	before.focus();
	before.dispatchEvent(new FocusEvent("focus"));
	flushSync();
	await tick();
	expect(tooltip()).not.toBeNull();
	expect(target.querySelector(".ctl")).toBe(before);
	expect(document.activeElement).toBe(before);
	done();
});
