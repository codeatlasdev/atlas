// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";

// Incident (atlas#66, ui#225, fixed in kit 4.2.0): a menu or popover that was already open when a modal opened (a shortcut or
// an async action opening Settings over it) stayed outside the <dialog>. `showModal()` makes everything outside the dialog
// inert, so the menu was visible but dead, and it painted under the modal. Kit 4.2.0 moves it into the topmost modal and hands
// it on (or back home) when that modal closes first. This drives the REAL layer engine with real DOM, not a mock of it.
// Dynamic import on purpose: the Svelte loader registered by test/setup.ts must exist before the kit loads.
const { realKitModule } = await import("../../../test/uiKit.ts");
const { openLayer } = await realKitModule("primitives/layer.js");

// happy-dom has no <dialog>.showModal(); the kit only needs it to mark the dialog open.
HTMLDialogElement.prototype.showModal ??= function () {
	this.setAttribute("open", "");
};
HTMLDialogElement.prototype.close ??= function () {
	this.removeAttribute("open");
};

const opened: Array<{ close(): void }> = [];
afterEach(() => {
	while (opened.length) opened.pop()?.close();
	document.body.innerHTML = "";
});

function scene() {
	document.body.innerHTML = `
		<div id="home"><button id="anchor">menu</button><div id="menu" role="menu"><span class="item">Copy path</span></div></div>
		<dialog id="modal"><div id="modal-body"><button id="in">in</button></div></dialog>
		<dialog id="second"><button id="in2">in2</button></dialog>`;
	const $ = (id: string) => document.getElementById(id);
	const menu = openLayer($("menu"), { kind: "popover", anchor: $("anchor") });
	opened.push(menu);
	return { $, menu };
}
const open = (el: Element) => {
	const layer = openLayer(el, { kind: "modal" });
	opened.push(layer);
	return layer;
};

test("a menu open BEFORE a modal moves into it, so it stays interactive above the modal", () => {
	const { $ } = scene();
	expect($("menu").closest("dialog")).toBeNull();
	open($("modal"));
	expect($("menu").closest("dialog")).toBe($("modal"));
	expect($("menu").matches(":popover-open")).toBe(true);
});

test("a menu opened AFTER the modal also lives inside it (the original #66 order)", () => {
	document.body.innerHTML = `<div id="home"><button id="anchor">a</button><div id="menu"></div></div><dialog id="modal"></dialog>`;
	const $ = (id: string) => document.getElementById(id);
	open($("modal"));
	opened.push(openLayer($("menu"), { kind: "popover", anchor: $("anchor") }));
	expect($("menu").closest("dialog")).toBe($("modal"));
});

test("the modal closing first puts the menu back where its owner declared it instead of deleting it with the modal", () => {
	const { $ } = scene();
	const modal = open($("modal"));
	expect($("menu").closest("dialog")).not.toBeNull();
	modal.close();
	expect($("menu").isConnected).toBe(true);
	expect($("menu").closest("dialog")).toBeNull();
	expect($("menu").parentElement).toBe($("home"));
	expect($("menu").matches(":popover-open")).toBe(true);
});

test("with two modals stacked, closing the top one hands the menu to the one below, not home", () => {
	const { $ } = scene();
	open($("modal"));
	const top = open($("second"));
	expect($("menu").closest("dialog")).toBe($("second"));
	top.close();
	expect($("menu").closest("dialog")).toBe($("modal"));
});

test("closing the menu while hosted in a modal removes the host marker and leaves no stray node in the modal", () => {
	const { $, menu } = scene();
	open($("modal"));
	menu.close();
	expect($("modal").querySelector("#menu")).toBeNull();
	expect($("home").querySelectorAll("#menu").length).toBe(1);
});
