// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";

// Incident (#64): right-click on an Explorer row opened no menu. The kit positions a menu against its trigger, so the
// tree's one menu is a zero-size trigger parked at the pointer. This runs the REAL kit DropdownMenu: if a kit upgrade
// changes how a menu is opened or anchored, right-click goes dead again and this goes red.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
await (
	await import("../../../test/uiKit.ts")
).useRealKit("DropdownMenu", "DropdownMenu/DropdownMenuItem", "DropdownMenu/DropdownMenuSeparator");
const { flushSync, mount, tick, unmount } = await import("svelte");
const { default: PointerMenu } = await import("./PointerMenu.svelte");

// PointerMenu imports the kit by package entry; the real DropdownMenu family is what a build uses.
async function setup(items) {
	const target = document.createElement("div");
	document.body.append(target);

	const app = flushSync(() => mount(PointerMenu, { target, props: { items } }));

	return { api: app, done: () => (unmount(app), target.remove()) };
}
const menu = () => document.querySelector("[role=menu]");
const labels = () => [...document.querySelectorAll("[role=menuitem]")].map((e) => e.textContent.trim());
const frame = () => new Promise((r) => requestAnimationFrame(() => r(null)));

test("openAt shows the items in order, with a separator only where the group changes", async () => {
	const { api, done } = await setup([
		{ label: "Open", group: 0, run() {} },
		{ label: "Rename", group: 1, run() {} },
		{ label: "Copy path", group: 1, run() {} },
		{ label: "Delete", group: 2, destructive: true, run() {} },
	]);
	expect(menu()).toBeNull();
	await api.openAt(120, 80);
	await tick();
	expect(labels()).toEqual(["Open", "Rename", "Copy path", "Delete"]);
	expect(document.querySelectorAll("[role=separator]").length).toBe(2);
	expect(document.querySelector(".ca-dropdown-item--destructive")?.textContent.trim()).toBe("Delete");
	done();
});

test("the menu is anchored at the requested point, not at the window corner", async () => {
	const { api, done } = await setup([{ label: "Open", run() {} }]);
	await api.openAt(120, 80);
	await tick();
	const a = document.querySelector(".pointer-anchor");
	expect(a.style.left).toBe("120px");
	expect(a.style.top).toBe("80px");
	await api.openAt(300, 40);
	await tick();
	expect(a.style.left).toBe("300px");
	expect(a.style.top).toBe("40px");
	done();
});

test("choosing an item runs it once and closes the menu", async () => {
	let ran = 0;
	const { api, done } = await setup([{ label: "Open", run: () => ran++ }]);
	await api.openAt(10, 10);
	await tick();
	document.querySelector("[role=menuitem]").click();
	await tick();
	expect(ran).toBe(1);
	await frame();
	await frame();
	expect(document.querySelector("[role=menu][data-state=open]")).toBeNull();
	done();
});

test("a second right-click on another row reopens the menu with the new items", async () => {
	const props = { items: [{ label: "A", run() {} }] };
	const target = document.createElement("div");
	document.body.append(target);
	const { proxy } = await import("svelte/internal/client");
	const p = proxy(props);
	const app = flushSync(() => mount(PointerMenu, { target, props: p }));
	await app.openAt(5, 5);
	await tick();
	expect(labels()).toEqual(["A"]);
	document.querySelector("[role=menuitem]").click();
	await tick();
	p.items = [{ label: "B", run() {} }];
	await app.openAt(9, 9);
	await tick();
	expect(labels()).toEqual(["B"]);
	unmount(app);
	target.remove();
});

// Incident (agent instance on the alpha.13 integration): a right-click while the menu was already open closed it,
// because `openAt` clicked the trigger and a trigger click toggles. Right-clicking another row must move the menu.
test("openAt while the menu is open keeps it open, at the new point", async () => {
	const { api, done } = await setup([{ label: "Open", run() {} }]);
	await api.openAt(10, 10);
	await tick();
	await frame();
	expect(menu()).not.toBeNull();
	await api.openAt(200, 120);
	await tick();
	await frame();
	expect(document.querySelector("[role=menu]"), "second right-click closed the menu").not.toBeNull();
	const a = document.querySelector(".pointer-anchor");
	expect([a.style.left, a.style.top]).toEqual(["200px", "120px"]);
	done();
});

// Incident (alpha.13 integration, agent instance, WKWebView): the Explorer context menu was `open` in the DOM but never
// painted. The kit's anchored menus carry `position-visibility: anchors-visible`, and WebKit treats a 0x0 anchor as
// not visible, so it hid the menu. Separately, `.space-view` has `will-change: transform`, which makes it the
// containing block of `position: fixed`. That geometry needs real layout and was checked in the agent instance;
// happy-dom has no layout, so this pins the box the stylesheet gives the anchor.
test("the anchor has a box: an empty anchor hides an anchors-visible menu", async () => {
	const { api, done } = await setup([{ label: "Open", run() {} }]);
	await api.openAt(10, 10);
	await tick();
	const a = document.querySelector(".pointer-anchor");
	expect([a.style.width, a.style.height], "0x0 anchor").toEqual(["1px", "1px"]);
	done();
});

// Incident (alpha.13 integration): right-click, right-click on another row, Escape left a `data-state=closed` menu
// mounted forever, painted in page flow at the Explorer's bottom-left ("ghost menu"). Closing and reopening in one
// flush left the kit's exit presence of the first menu stuck. After any sequence, at most one menu exists, and none
// once closed.
test("reopening and then closing leaves no menu mounted", async () => {
	const { api, done } = await setup([{ label: "Open", run() {} }]);
	await api.openAt(10, 10);
	await tick();
	await frame();
	await api.openAt(40, 60);
	await tick();
	await frame();
	expect(document.querySelectorAll("[role=menu]").length).toBe(1);
	document.querySelector(".pointer-anchor").click();
	for (let i = 0; i < 4; i++) await frame();
	await tick();
	expect(document.querySelectorAll("[role=menu]").length, "a closed menu stayed mounted").toBe(0);
	done();
});
