// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterAll, afterEach, beforeAll, beforeEach, expect, test } from "bun:test";
import { readFileSync } from "node:fs";

// Incident (codeatlasdev/atlas#501): open a modal, close it by clicking outside it, click another button at once: the
// click did nothing and a second one was needed. A kit modal stays a modal (`showModal()`, the rest of the page inert)
// until its 150 ms exit animation ends, so a press on the page in that window is dropped by the browser. These tests run
// the REAL kit components (Dialog, AlertDialog, Drawer, Sheet, Popover) and the real `closingLayers` module.
//
// happy-dom has no top layer, no inertness, no hit-testing and no animation engine, so the browser half is modelled by
// the three small stand-ins below, each written from the platform's rule and checked against WebKit (Playwright, same
// scenario, 0/30/80/150 ms after the dismissal; the PR records the numbers):
//   1. `showModal()` marks the dialog modal, `close()` and `show()` clear it (`:modal`);
//   2. `press()` delivers a click only inside the topmost modal while one is open (HTML: "blocked by a modal dialog");
//   3. a kit overlay with `data-state="closed"` has one running animation (`getAnimations()`), as measured on WebKit:
//      `ca-dialog-out` / `ca-overlay-out`, 150 ms. Here it runs until the test ends it (`endExit`): the exit is a state
//      the test holds the dialog in, not a 150 ms race against a real timer (the tests pressed "mid-exit" 40 ms after the
//      dismissal and timed out or lost the window on a loaded machine, where those 40 ms took longer than the exit).
// The workaround must give the page its clicks back AT ONCE and still let the exit fade play, so the tests press one
// microtask after the dismissal (nothing has rendered yet) and again mid-exit.
const { flushSync, mount, unmount, createRawSnippet } = await import("svelte");
const { proxy } = await import("svelte/internal/client");
const { realKitComponent } = await import("../../../test/uiKit.ts");
const { releaseClosingModals } = await import("./closingLayers.ts");
const { until } = await import("../../../test/until.ts");

const EXIT_MS = 150;
const modals = new Set<Element>();
const saved = {
	showModal: HTMLDialogElement.prototype.showModal,
	show: HTMLDialogElement.prototype.show,
	close: HTMLDialogElement.prototype.close,
	matches: Element.prototype.matches,
	getAnimations: Element.prototype.getAnimations,
};
type FakeAnimation = { playState: string; finished: Promise<void>; finish(): void; cancel(): void };
const fake = new WeakMap<Element, FakeAnimation>();
/** The exit animations that have not ended: `endExit` ends them. */
const exits = new Set<FakeAnimation>();

beforeAll(() => {
	HTMLDialogElement.prototype.showModal = function () {
		this.setAttribute("open", "");
		modals.add(this);
	};
	HTMLDialogElement.prototype.show = function () {
		this.setAttribute("open", "");
	};
	HTMLDialogElement.prototype.close = function () {
		this.removeAttribute("open");
		modals.delete(this);
	};
	Element.prototype.matches = function (selector) {
		return selector === ":modal" ? modals.has(this) : saved.matches.call(this, selector);
	};
	Element.prototype.getAnimations = function () {
		if (!this.matches("dialog.ca-overlay") || this.getAttribute("data-state") !== "closed") return [];
		let a = fake.get(this);
		if (!a) {
			let done: () => void;
			const finished = new Promise<void>((r) => (done = r));
			const end = () => {
				a.playState = "finished";
				done();
			};
			a = {
				playState: "running",
				finished,
				effect: { getComputedTiming: () => ({ endTime: EXIT_MS }) },
				finish: end,
				cancel: end,
			};
			fake.set(this, a);
			exits.add(a);
		}
		return [a];
	};
});
afterAll(() => {
	Object.assign(HTMLDialogElement.prototype, {
		showModal: saved.showModal,
		show: saved.show,
		close: saved.close,
	});
	Element.prototype.matches = saved.matches;
	Element.prototype.getAnimations = saved.getAnimations;
});

let stop = () => {};
let host: HTMLElement;
const apps: unknown[] = [];
beforeEach(() => {
	host = document.createElement("div");
	document.body.append(host);
});
afterEach(() => {
	stop();
	stop = () => {};
	while (apps.length) unmount(apps.pop());
	modals.clear();
	exits.clear();
	host.remove();
	document.body.innerHTML = "";
});

const nextFrame = () => new Promise((r) => requestAnimationFrame(r));
// Two animation frames: what the kit needs to notice a close (they pass whatever the load, the exit does not end on its own).
const frames = async () => (await nextFrame(), await nextFrame());
/** End the exit animation: the kit then closes the layer and removes it. */
function finishExits() {
	for (const a of exits) a.finish();
	exits.clear();
}
async function endExit(dialog: Element) {
	finishExits();
	await until(() => !dialog.isConnected, "the kit removes the dialog after its exit");
}

/** A click on `el` the way the browser delivers it: nothing outside the topmost modal while a modal is open. */
function press(el: Element) {
	const top = [...modals].at(-1);
	if (top && !top.contains(el)) return false;
	for (const type of ["pointerdown", "pointerup", "click"])
		el.dispatchEvent(
			new (type === "click" ? MouseEvent : PointerEvent)(type, { bubbles: true, composed: true }),
		);
	return true;
}

/** The scrim of a kit modal is the `<dialog>` itself, pressed outside its box (happy-dom boxes are 0x0). */
function pressScrim(dialog: Element) {
	const at = { bubbles: true, clientX: 500, clientY: 500 };
	dialog.dispatchEvent(new PointerEvent("pointerdown", at));
	dialog.dispatchEvent(new PointerEvent("pointerup", at));
	dialog.dispatchEvent(new MouseEvent("click", at));
}

const body = createRawSnippet(() => ({ render: () => `<button class="in">in</button>` }));
let clicks = 0;
function addNextButton() {
	const next = document.createElement("button");
	next.className = "next";
	next.addEventListener("click", () => clicks++);
	host.append(next);
	clicks = 0;
	return next;
}

const dismissEscape = () =>
	document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));

const modalKinds = {
	Dialog: { props: () => ({ title: "t", children: body }), dismiss: pressScrim },
	Drawer: { props: () => ({ title: "t", side: "right", children: body }), dismiss: pressScrim },
	Sheet: { props: () => ({ title: "t", children: body }), dismiss: pressScrim },
	// An alert dialog ignores the scrim on purpose: Escape (or Cancel) is how it closes.
	AlertDialog: { props: () => ({ title: "t", onconfirm() {} }), dismiss: dismissEscape },
};

async function openKit(name: string) {
	const Component = await realKitComponent(name);
	const props = proxy({ open: false, ...modalKinds[name].props() });
	apps.push(flushSync(() => mount(Component, { target: host, props })));
	props.open = true;
	flushSync();
	await until(
		() => document.querySelector("dialog")?.matches(":modal"),
		"the kit opens the dialog as a modal",
	);
	return { props, dialog: document.querySelector("dialog") };
}

for (const name of Object.keys(modalKinds)) {
	test(`${name}: the next click after an outside dismissal runs its handler once, at once and mid-exit`, async () => {
		stop = releaseClosingModals();
		const { props, dialog } = await openKit(name);
		const next = addNextButton();
		// The page is inert while the modal is up: the control (the model agrees with the browser).
		expect(press(next)).toBe(false);
		modalKinds[name].dismiss(dialog);
		flushSync();
		expect(props.open).toBe(false);
		// The user's very next press: one microtask after the dismissal, before any frame.
		await Promise.resolve();
		expect(press(next)).toBe(true);
		expect(clicks).toBe(1);
		// And again in the middle of the exit, with the dialog still fading in the page.
		await frames();
		expect(dialog.isConnected).toBe(true);
		expect(press(next)).toBe(true);
		expect(clicks).toBe(2);
		// The exit ends and the layer is removed, with the scrim stand-in.
		await endExit(dialog);
		expect(modals.size).toBe(0);
		expect(document.querySelector("dialog")).toBeNull();
		expect(document.querySelector(".closing-scrim")).toBeNull();
	});
}

test("a released dialog keeps fading: still in the page, inert, not modal, with a scrim stand-in, until the exit ends", async () => {
	stop = releaseClosingModals();
	const { props, dialog } = await openKit("Dialog");
	pressScrim(dialog);
	flushSync();
	expect(props.open).toBe(false);
	await frames();
	expect(dialog.isConnected).toBe(true);
	expect(dialog.getAttribute("data-state")).toBe("closed");
	expect(dialog.hasAttribute("open")).toBe(true);
	expect(dialog.matches(":modal")).toBe(false);
	expect(dialog.inert).toBe(true);
	const scrim = document.querySelector(".closing-scrim");
	expect(scrim).not.toBeNull();
	expect(scrim.nextElementSibling).toBe(dialog);
	await endExit(dialog);
	expect(dialog.isConnected).toBe(false);
});

test("opening the dialog again during its exit makes it the modal it was (no inert shell, no stray scrim)", async () => {
	stop = releaseClosingModals();
	const { props, dialog } = await openKit("Dialog");
	pressScrim(dialog);
	flushSync();
	await frames();
	expect(dialog.matches(":modal")).toBe(false);
	props.open = true;
	flushSync();
	await Promise.resolve();
	expect(dialog.getAttribute("data-state")).toBe("open");
	expect(dialog.matches(":modal")).toBe(true);
	expect(dialog.inert).toBe(false);
	expect(document.querySelector(".closing-scrim")).toBeNull();
	const next = addNextButton();
	expect(press(next)).toBe(false);
	expect(press(dialog.querySelector("button.in"))).toBe(true);
	// The old exit ends now (the kit had its animation to wait for): a dialog that was opened again is not removed.
	finishExits();
	await frames();
	expect(dialog.isConnected).toBe(true);
});

test("a control focused during the exit keeps focus when the kit removes the layer and restores the opener", async () => {
	stop = releaseClosingModals();
	const opener = addNextButton();
	opener.focus();
	const { dialog } = await openKit("Dialog");
	const other = document.createElement("button");
	host.append(other);
	pressScrim(dialog);
	flushSync();
	await frames();
	other.focus();
	// A browser reports focus with `focusin`; happy-dom's `focus()` does not.
	other.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
	expect(document.activeElement).toBe(other);
	await endExit(dialog);
	expect(dialog.isConnected).toBe(false);
	expect(document.activeElement).toBe(other);
});

// The control: without the workaround the kit keeps the closing dialog modal for its whole exit animation and the click
// is dropped. When this goes red the kit releases the modal itself: delete `closingLayers.ts` (see its header).
test("without the workaround, a click during the exit animation of a Dialog is dropped (the kit bug)", async () => {
	const { props, dialog } = await openKit("Dialog");
	const next = addNextButton();
	pressScrim(dialog);
	flushSync();
	expect(props.open).toBe(false);
	await frames();
	expect(dialog.isConnected).toBe(true);
	expect(dialog.matches(":modal")).toBe(true);
	expect(press(next)).toBe(false);
	expect(clicks).toBe(0);
	// Once the exit animation ends the kit releases it and the same click works: the loss is the window, not the button.
	await endExit(dialog);
	expect(press(next)).toBe(true);
	expect(clicks).toBe(1);
});

test("a spinner that never ends inside a closing modal is cancelled and no longer keeps it alive", async () => {
	stop = releaseClosingModals();
	const Component = await realKitComponent("Dialog");
	const props = proxy({ open: true, title: "t", children: body });
	apps.push(flushSync(() => mount(Component, { target: host, props })));
	await until(
		() => document.querySelector("dialog")?.matches(":modal"),
		"the kit opens the dialog as a modal",
	);
	const dialog = document.querySelector("dialog");
	const forever = {
		playState: "running",
		finished: new Promise(() => {}),
		effect: { getComputedTiming: () => ({ endTime: Infinity }) },
		cancel() {
			this.playState = "idle";
		},
		finish() {
			throw new Error("InvalidStateError: cannot finish an infinite animation");
		},
	};
	const base = Element.prototype.getAnimations;
	Element.prototype.getAnimations = function (o) {
		return [...base.call(this, o), ...(this === dialog && forever.playState === "running" ? [forever] : [])];
	};
	props.open = false;
	flushSync();
	await frames();
	expect(forever.playState).toBe("idle");
	// The kit's own exit animation is all that is left to wait for.
	await endExit(dialog);
	Element.prototype.getAnimations = base;
	expect(document.querySelector("dialog")).toBeNull();
});

test("a popover dismissed by a press outside it lets the next click through once", async () => {
	stop = releaseClosingModals();
	// The harness declares a kit Popover whose trigger spreads the kit's props, as every Atlas popover does. Another file's
	// stub of the package entry must not replace it: the real components are pointed at again here.
	await (
		await import("../../../test/uiKit.ts")
	).useRealKit("Dialog", "DropdownMenu", "DropdownMenu/DropdownMenuItem", "Popover");
	const { default: Harness } = await import("../../../test/DialogOverlaysHarness.svelte");
	apps.push(flushSync(() => mount(Harness, { target: host, props: { open: false } })));
	const next = addNextButton();
	const content = () => [...document.querySelectorAll("span")].find((s) => s.textContent === "body");
	host.querySelector(".pop-t").dispatchEvent(new MouseEvent("click", { bubbles: true }));
	await until(content, "the popover opens");
	// The press that dismisses it is the click on the next button: pointerdown outside closes, the click still lands.
	next.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }));
	expect(press(next)).toBe(true);
	await until(() => !content(), "the popover closes");
	expect(clicks).toBe(1);
});

// A closing menu, popover or tooltip does not make the page inert, but its exit fade still takes presses over its own box.
test("app.css gives a closing kit floating layer pointer-events: none (menus, popovers, tooltips)", () => {
	const css = readFileSync(new URL("../../app.css", import.meta.url), "utf8");
	expect(css).toMatch(/\[popover\]\[data-state="closed"\]\s*\{[^}]*pointer-events:\s*none/);
});
