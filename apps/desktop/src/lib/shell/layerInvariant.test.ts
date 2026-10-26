// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, expect, test } from "bun:test";
import { offendingLayers } from "./layerInvariant.ts";

// Guards the ghost-frames incident (docs/postmortems/ghost-frames.md): a parked space layer left in view.
let root: HTMLElement;

function layer(id: string, role: "current" | "near", x: number, visible: boolean) {
	const el = document.createElement("div");
	el.className = "space-layer";
	el.dataset.space = id;
	el.dataset.role = role;
	el.style.visibility = visible ? "visible" : "hidden";
	el.getBoundingClientRect = () => new DOMRect(x, 0, window.innerWidth, window.innerHeight);
	root.append(el);
	return el;
}

function setup() {
	root = document.createElement("div");
	root.getBoundingClientRect = () => new DOMRect(0, 0, window.innerWidth, window.innerHeight);
	document.body.append(root);
	const w = window.innerWidth;
	return {
		a: layer("a", "current", 0, true),
		b: layer("b", "near", w, true),
		c: layer("c", "near", -w, false),
	};
}

afterEach(() => root.remove());

test("parked neighbours off screen or hidden are not ghosts", () => {
	setup();
	expect(offendingLayers(root)).toEqual([]);
});

test("after a commit the previous current, now near and parked, is not reported", () => {
	const { a, b } = setup();
	a.dataset.role = "near";
	a.getBoundingClientRect = () => new DOMRect(-window.innerWidth, 0, window.innerWidth, window.innerHeight);
	b.dataset.role = "current";
	b.getBoundingClientRect = () => new DOMRect(0, 0, window.innerWidth, window.innerHeight);
	expect(offendingLayers(root)).toEqual([]);
});

test("a near layer visible and in view while idle is a ghost", () => {
	const { b } = setup();
	b.getBoundingClientRect = () => new DOMRect(0, 0, window.innerWidth, window.innerHeight);
	expect(offendingLayers(root)).toEqual([b]);
});

test("a partly visible near layer counts; the pair a gesture keeps does not", () => {
	const { b, c } = setup();
	b.getBoundingClientRect = () =>
		new DOMRect(window.innerWidth / 2, 0, window.innerWidth, window.innerHeight);
	c.style.visibility = "visible";
	c.getBoundingClientRect = () =>
		new DOMRect(-window.innerWidth / 2, 0, window.innerWidth, window.innerHeight);
	expect(offendingLayers(root)).toEqual([b, c]);
	expect(offendingLayers(root, ["a", "b"])).toEqual([c]);
});

// Incident (gesture bench, first run): every run reported the parked neighbours as ghosts. A parked layer sits one
// pager-width away, which is inside the window (over the sidebar) but outside `.pager`'s `overflow: hidden` clip.
test("a parked layer outside the pager's clip but inside the window is not a ghost", () => {
	const { b, c } = setup();
	// pager at x 300..1000 in a 1000-wide window: prev parked at -700 → -400..300 (over the sidebar), next at 1000
	root.getBoundingClientRect = () => new DOMRect(300, 0, 700, window.innerHeight);
	c.style.visibility = "visible";
	c.getBoundingClientRect = () => new DOMRect(-400, 0, 700, window.innerHeight);
	b.getBoundingClientRect = () => new DOMRect(1000, 0, 700, window.innerHeight);
	expect(offendingLayers(root)).toEqual([]);
	b.getBoundingClientRect = () => new DOMRect(900, 0, 700, window.innerHeight);
	expect(offendingLayers(root)).toEqual([b]);
});
