// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";

// The webview must behave like a macOS app, not a page (the user rejects focus rings, the browser menu and zoom).
// Dynamic imports on purpose: the DOM from test/setup.ts must exist before the module loads.
const { nativeFeel: kit } = await (await import("../../test/uiKit.ts")).realKitModule("kit/native-feel.js");
const { allowContextMenu } = await import("./contextMenu.ts");
const nativeFeel = () => kit({ blockZoom: true, allowContextMenu });

let stop: () => void;
afterEach(() => stop?.());

const key = (init) =>
	window.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
const menu = (target) => {
	const e = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
	target.dispatchEvent(e);
	return e;
};

test("focus rings are for the keyboard: Tab / arrows set data-kbd, a pointer press clears it", () => {
	stop = nativeFeel();
	const root = document.documentElement;
	expect("kbd" in root.dataset).toBe(false);
	key({ key: "Tab" });
	expect("kbd" in root.dataset).toBe(true);
	window.dispatchEvent(new Event("pointerdown", { bubbles: true }));
	expect("kbd" in root.dataset).toBe(false);
	key({ key: "ArrowDown" });
	expect("kbd" in root.dataset).toBe(true);
	key({ key: "a" });
	window.dispatchEvent(new Event("pointerdown", { bubbles: true }));
	key({ key: "a" });
	expect("kbd" in root.dataset).toBe(false);
});

test("the browser context menu is blocked on chrome and kept in text fields, editors and .ca-selectable", () => {
	stop = nativeFeel();
	const chrome = document.body.appendChild(document.createElement("button"));
	expect(menu(chrome).defaultPrevented).toBe(true);
	for (const html of [
		"<input>",
		"<textarea></textarea>",
		"<div contenteditable='true'></div>",
		"<div class='cm-editor'><span>x</span></div>",
		"<p class='ca-selectable'>x</p>",
	]) {
		const host = document.body.appendChild(document.createElement("div"));
		host.innerHTML = html;
		expect([
			html,
			menu(host.firstElementChild.querySelector("span") ?? host.firstElementChild).defaultPrevented,
		]).toEqual([html, false]);
	}
});

// Incident (ui#136): the filter said `.selectable`, the CSS said `.ca-selectable`. Kit 4.0.0 has one class; the old name no
// longer opts out, so a leftover `class="selectable"` is a visible bug (menu blocked), not a half-working one.
test("the pre-4.0 .selectable class no longer keeps the menu", () => {
	stop = nativeFeel();
	const host = document.body.appendChild(document.createElement("p"));
	host.className = "selectable";
	expect(menu(host).defaultPrevented).toBe(true);
});

test("a debug build keeps the browser menu for Inspect Element", () => {
	stop = nativeFeel();
	window.__ATLAS_DEBUG__ = true;
	try {
		expect(menu(document.body.appendChild(document.createElement("button"))).defaultPrevented).toBe(false);
	} finally {
		delete window.__ATLAS_DEBUG__;
	}
});

test("page zoom is blocked: ⌘ + - = 0 and ctrl-wheel pinch; other ⌘ keys pass", () => {
	stop = nativeFeel();
	for (const k of ["=", "+", "-", "0"]) {
		const e = new KeyboardEvent("keydown", { key: k, metaKey: true, bubbles: true, cancelable: true });
		window.dispatchEvent(e);
		expect([k, e.defaultPrevented]).toEqual([k, true]);
	}
	const other = new KeyboardEvent("keydown", { key: "s", metaKey: true, bubbles: true, cancelable: true });
	window.dispatchEvent(other);
	expect(other.defaultPrevented).toBe(false);
	const pinch = new WheelEvent("wheel", { bubbles: true, cancelable: true });
	// happy-dom drops `ctrlKey` from the init dict
	Object.defineProperty(pinch, "ctrlKey", { value: true });
	window.dispatchEvent(pinch);
	expect(pinch.defaultPrevented).toBe(true);
});

// Atlas zooms the layout itself (stores/zoom.svelte.ts, ⌘+ ⌘- ⌘0): the kit only cancels the browser's own zoom, it must
// not stop the event, or the app's handler on the same window never runs.
test("blocking browser zoom still lets the app's own zoom handler see the key", () => {
	stop = nativeFeel();
	const seen: string[] = [];
	const onKey = (e: KeyboardEvent) => seen.push(e.key);
	window.addEventListener("keydown", onKey);
	window.dispatchEvent(
		new KeyboardEvent("keydown", { key: "-", metaKey: true, bubbles: true, cancelable: true }),
	);
	window.removeEventListener("keydown", onKey);
	expect(seen).toEqual(["-"]);
});

test("links and images cannot be dragged out of the window", () => {
	stop = nativeFeel();
	const a = document.body.appendChild(document.createElement("a"));
	const e = new Event("dragstart", { bubbles: true, cancelable: true });
	a.dispatchEvent(e);
	expect(e.defaultPrevented).toBe(true);
});

test("cleanup removes every listener and the data-kbd flag", () => {
	stop = nativeFeel();
	key({ key: "Tab" });
	stop();
	expect("kbd" in document.documentElement.dataset).toBe(false);
	const e = new KeyboardEvent("keydown", { key: "=", metaKey: true, bubbles: true, cancelable: true });
	window.dispatchEvent(e);
	expect(e.defaultPrevented).toBe(false);
});
