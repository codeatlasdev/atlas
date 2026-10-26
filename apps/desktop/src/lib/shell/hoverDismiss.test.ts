// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, expect, test } from "bun:test";
import { dismissHoverOnBlur } from "./hoverDismiss.ts";

// Incident (2026-10-26): the Settings tooltip stayed on screen after switching to another app. The kit closes a tooltip
// on `pointerleave`, which never fires when the window loses focus under a resting cursor.
let stop: () => void;
let trigger: HTMLElement;
afterEach(() => {
	stop?.();
	trigger?.remove();
});

function openTooltip() {
	trigger = document.createElement("button");
	trigger.setAttribute("aria-describedby", "tip");
	trigger.dataset.state = "open";
	let left = 0;
	trigger.addEventListener("pointerleave", () => {
		left++;
		trigger.dataset.state = "closed";
	});
	document.body.append(trigger);
	return () => left;
}

test("a hover tooltip open when the window loses focus gets its pointerleave", () => {
	stop = dismissHoverOnBlur();
	const left = openTooltip();
	window.dispatchEvent(new Event("blur"));
	expect(left()).toBe(1);
	expect(trigger.dataset.state).toBe("closed");
});

test("a closed trigger, and an open menu trigger (no aria-describedby), are left alone", () => {
	stop = dismissHoverOnBlur();
	const left = openTooltip();
	trigger.dataset.state = "closed";
	window.dispatchEvent(new Event("blur"));
	expect(left()).toBe(0);
	trigger.removeAttribute("aria-describedby");
	trigger.dataset.state = "open";
	window.dispatchEvent(new Event("blur"));
	expect(left()).toBe(0);
});

test("hidden document (Space switch, minimize) dismisses too; stop() removes the listeners", () => {
	stop = dismissHoverOnBlur();
	const left = openTooltip();
	Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
	document.dispatchEvent(new Event("visibilitychange"));
	expect(left()).toBe(1);
	stop();
	trigger.dataset.state = "open";
	window.dispatchEvent(new Event("blur"));
	expect(left()).toBe(1);
	delete document.visibilityState;
});
