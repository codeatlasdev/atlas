// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";
import { guardFocus, inParkedLayer } from "./parkedLayer.ts";

// Incident (#121): a space switch restyled every node of two layers (57 ms per layer of 23k nodes) because the pager kept a
// resting neighbour out of reach with `inert` and `pointer-events: none`. Both are inherited, so flipping either on a layer
// root makes Chromium recompute the style of every descendant. Without them a parked layer must still be unreachable:
// no focus (Tab, a script's `.focus()`) and no swipe zones. These tests hold that replacement to the contract `inert` had.

const layer = (role: "current" | "near") => {
	const l = document.createElement("div");
	l.className = "space-layer";
	l.dataset.role = role;
	const body = document.createElement("div");
	body.className = "space-body";
	const button = document.createElement("button");
	button.textContent = role;
	body.append(button);
	l.append(body);
	return { l, button };
};

afterEach(() => {
	document.body.innerHTML = "";
});

test("an element inside a near layer is parked; inside the current layer, or outside any layer, it is not", () => {
	const cur = layer("current");
	const near = layer("near");
	const outside = document.createElement("button");
	document.body.append(cur.l, near.l, outside);
	expect(inParkedLayer(near.button)).toBe(true);
	expect(inParkedLayer(near.l)).toBe(true);
	expect(inParkedLayer(cur.button)).toBe(false);
	expect(inParkedLayer(outside)).toBe(false);
});

test("a layer that becomes current is no longer parked, with no other change to its subtree", () => {
	const a = layer("near");
	document.body.append(a.l);
	expect(inParkedLayer(a.button)).toBe(true);
	a.l.dataset.role = "current";
	expect(inParkedLayer(a.button)).toBe(false);
});

test("focus that lands in a parked layer goes back to where it came from", () => {
	const cur = layer("current");
	const near = layer("near");
	document.body.append(cur.l, near.l);
	const stop = guardFocus(document.body);
	cur.button.focus();
	expect(document.activeElement).toBe(cur.button);
	near.button.focus();
	expect(document.activeElement).toBe(cur.button);
	stop();
});

test("focus can move freely inside the current layer and to elements outside the pager", () => {
	const cur = layer("current");
	const second = document.createElement("button");
	cur.l.firstChild.append(second);
	const outside = document.createElement("button");
	document.body.append(cur.l, outside);
	const stop = guardFocus(document.body);
	cur.button.focus();
	second.focus();
	expect(document.activeElement).toBe(second);
	outside.focus();
	expect(document.activeElement).toBe(outside);
	stop();
});

test("with nothing focused before, a script focusing a parked element is blurred, not left in the hidden layer", () => {
	const near = layer("near");
	document.body.append(near.l);
	const stop = guardFocus(document.body);
	near.button.focus();
	expect(document.activeElement).not.toBe(near.button);
	stop();
});

test("a layer that was current keeps focus handling right after it parks (the guard reads the role live)", () => {
	const a = layer("current");
	document.body.append(a.l);
	const stop = guardFocus(document.body);
	a.button.focus();
	expect(document.activeElement).toBe(a.button);
	a.button.blur();
	a.l.dataset.role = "near";
	a.button.focus();
	expect(document.activeElement).not.toBe(a.button);
	stop();
});

test("the guard stops guarding once detached", () => {
	const near = layer("near");
	document.body.append(near.l);
	const stop = guardFocus(document.body);
	stop();
	near.button.focus();
	expect(document.activeElement).toBe(near.button);
});

// Incident (#121, found in the real app): focusing an element of a parked layer makes the browser scroll the pager's
// `overflow: hidden` box to reveal it BEFORE `focusin` fires. Bouncing focus back left the pager scrolled by one page,
// so the parked layer sat under the pointer and took every click. `inert` had prevented the scroll; the guard must undo it.
test("bouncing focus also undoes the scroll the browser did to reveal the parked element", () => {
	const cur = layer("current");
	const near = layer("near");
	const pager = document.createElement("div");
	pager.append(cur.l, near.l);
	document.body.append(pager);
	const stop = guardFocus(pager);
	cur.button.focus();
	// the browser's reveal-on-focus, which happens before the event
	near.button.addEventListener("focus", () => {
		pager.scrollLeft = 996;
		near.l.scrollLeft = 12;
	});
	near.button.focus();
	expect(pager.scrollLeft).toBe(0);
	expect(near.l.scrollLeft).toBe(0);
	stop();
});
