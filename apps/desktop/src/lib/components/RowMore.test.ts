// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";

// Incident (codeatlasdev/atlas#67): the "…" of a project row only switched to the project. The row is a
// `role="button"` with `onclick={activate}`; the kit DropdownMenu trigger was a `<span>` inside the kit's own trigger
// button and nothing stopped its click from bubbling to the row, so `shell.switchTo()` re-rendered the sidebar and
// the menu never stayed open. The same markup lived in WorktreeRow and the Explorer row.
// Real kit, real rows: the contract is what a click on the trigger does to the row and to the menu.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
// `RowMore` and the harness import the kit from its package entry, so the entry is pointed at the real components first
// (test/uiKit.ts: a stub kit from another suite in the same process has no DropdownMenu).
await (await import("../../../test/uiKit.ts")).useRealKit("DropdownMenu", "DropdownMenu/DropdownMenuItem");
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: Harness } = await import("../../../test/RowHarness.svelte");

let done: (() => void) | undefined;
afterEach(() => {
	done?.();
	done = undefined;
});

function setup(legacy = false) {
	const target = document.createElement("div");
	document.body.append(target);
	const calls = { activate: 0, pick: 0 };
	const app = flushSync(() =>
		mount(Harness, {
			target,
			props: { legacy, onactivate: () => calls.activate++, onpick: () => calls.pick++ },
		}),
	);
	done = () => (unmount(app), target.remove());
	return { target, calls };
}
const menu = () => document.querySelector("[role=menu]");
const row = (t: Element) => t.querySelector(".row");
const trigger = (t: Element) => t.querySelector("[aria-haspopup=menu]");
const settle = async () => (flushSync(), await tick(), await new Promise((r) => setTimeout(r, 20)));

test("the bug: with the kit's bare trigger a click on the … also activates the row", async () => {
	const { target, calls } = setup(true);
	trigger(target).dispatchEvent(new MouseEvent("click", { bubbles: true }));
	await settle();
	expect(calls.activate).toBe(1);
});

test("clicking the … opens the menu and does not activate the row", async () => {
	const { target, calls } = setup();
	trigger(target).dispatchEvent(new MouseEvent("click", { bubbles: true }));
	await settle();
	expect(menu()).not.toBeNull();
	expect(calls.activate).toBe(0);
});

test("the … is a real button reachable by keyboard, not a span inside the row's role=button", () => {
	const { target } = setup();
	const t = trigger(target);
	expect(t.tagName).toBe("BUTTON");
	expect(t.getAttribute("aria-label")).toBe("Menu");
	expect(t.closest(".row")).not.toBeNull();
	expect(target.querySelectorAll("span[aria-label]").length).toBe(0);
});

test("Enter and Space on the … open the menu and never activate the row", async () => {
	for (const key of ["Enter", " "]) {
		const { target, calls } = setup();
		trigger(target).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
		await settle();
		expect(menu()).not.toBeNull();
		expect(calls.activate).toBe(0);
		done?.();
		done = undefined;
	}
});

test("right-click on the row opens the menu through the … button and does not activate the row", async () => {
	const { target, calls } = setup();
	row(target).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
	await settle();
	expect(menu()).not.toBeNull();
	expect(calls.activate).toBe(0);
});

test("right-click inside the open menu does not click the … again (the menu stays open)", async () => {
	const { target } = setup();
	trigger(target).dispatchEvent(new MouseEvent("click", { bubbles: true }));
	await settle();
	expect(menu()).not.toBeNull();
	menu()
		.querySelector("[role=menuitem]")
		.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
	await settle();
	expect(menu()?.getAttribute("data-state")).toBe("open");
	expect(trigger(target).getAttribute("aria-expanded")).toBe("true");
});

test("Enter on the row itself still activates it", async () => {
	const { target, calls } = setup();
	row(target).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
	await settle();
	expect(calls.activate).toBe(1);
});

test("picking an item runs it, closes the menu and still does not activate the row", async () => {
	const { target, calls } = setup();
	trigger(target).dispatchEvent(new MouseEvent("click", { bubbles: true }));
	await settle();
	menu().querySelector("[role=menuitem]").click();
	await settle();
	expect(calls.pick).toBe(1);
	expect(calls.activate).toBe(0);
});

test("a second click on the … closes the menu again without activating the row", async () => {
	const { target, calls } = setup();
	const t = trigger(target);
	t.dispatchEvent(new MouseEvent("click", { bubbles: true }));
	await settle();
	t.dispatchEvent(new MouseEvent("click", { bubbles: true }));
	await settle();
	expect(calls.activate).toBe(0);
	expect(menu()?.getAttribute("data-state") ?? "closed").toBe("closed");
});

// The real rows import the whole store graph (`import.meta.glob`, Tauri) and cannot be mounted under bun, so the
// two that carry a RowMore "…" are pinned by what their markup must contain. Verified red: restore the bare `trigger` span
// in any of them and this fails.
import { readFileSync } from "node:fs";
import { join } from "node:path";
const rows = {
	"sidebar/ProjectHead.svelte": "Project menu",
	"sidebar/WorktreeRow.svelte": "Worktree menu",
};
for (const [file, label] of Object.entries(rows)) {
	test(`${file}: the … is a RowMore button and the row ignores events from its menu`, () => {
		const src = readFileSync(join(import.meta.dir, file), "utf8");
		expect(src).toContain(`<RowMore label="${label}"`);
		expect(src).not.toMatch(/\{#snippet trigger\(\)\}/);
		expect(src).not.toMatch(/<DropdownMenu\b/);
		expect(src).toContain("fromMenu(");
		// the row's own activation: click handler and key handler both consult it
		expect(src.match(/fromMenu\(/g).length).toBeGreaterThanOrEqual(2);
	});
}

// ProjectHead keeps right-click on the row: it clicks the same RowMore button (bound as `menuTrigger`), and skips
// right-clicks that started inside the menu itself. The "Remove from Atlas…" item lives in the shared RowMenu.
test("ProjectHead.svelte: right-click opens the RowMore menu; RowMenu carries Remove from Atlas…", () => {
	const head = readFileSync(join(import.meta.dir, "sidebar/ProjectHead.svelte"), "utf8");
	expect(head).toContain("bind:el={menuTrigger}");
	expect(head).toMatch(/oncontextmenu=\{\(e\) => \{[^}]*fromMenu\(e\)[^}]*menuTrigger\?\.click\(\)/);
	expect(head).toContain("<RowMenu ");
	expect(readFileSync(join(import.meta.dir, "sidebar/RowMenu.svelte"), "utf8")).toContain(
		"Remove from Atlas",
	);
});

// The Explorer row has one menu mechanism: its "…" is a button that stops its own click and key and opens the tree's
// PointerMenu (the one item list right-click and Shift+F10 use) at its rect. The row and the tree both consult
// `fromMenu`, so nothing from a menu activates a row or moves the tree cursor.
test("Explorer.svelte: the … opens the shared PointerMenu and the row/tree ignore events from a menu", () => {
	const src = readFileSync(join(import.meta.dir, "Explorer.svelte"), "utf8");
	expect(src).not.toMatch(/<DropdownMenu\b/);
	expect(src).not.toMatch(/\{#snippet trigger\(\)\}/);
	const more = src.match(/<button[^>]*class="more"[\s\S]*?<\/button\s*>/)?.[0] ?? "";
	expect(more).toContain('aria-label="More actions"');
	expect(more).toContain("ev.stopPropagation()");
	expect(more).toContain("openMenu(menuOf(rel, isDir)");
	// row click, row Space key and the tree's key handler all consult it
	expect(src.match(/fromMenu\(/g).length).toBeGreaterThanOrEqual(3);
	expect(src).toMatch(/onclick=\{\(ev\) => fromMenu\(ev\) \|\| activate\(rel, isDir\)\}/);
	expect(src).toMatch(
		/function onTreeKey\(ev: KeyboardEvent\) \{\s*if \(editing \|\| fromMenu\(ev\)\) return;/,
	);
});

test("fromMenu is true only for events that started inside a menu", async () => {
	const { fromMenu } = await import("./rowEvent.ts");
	const host = document.createElement("div");
	host.innerHTML = `<div class="row"><b class="own"></b><div role="menu"><div role="menuitem" class="item"><i class="deep"></i></div></div></div>`;
	document.body.append(host);
	const hit = (sel: string) => {
		let r: boolean | undefined;
		host.addEventListener("click", (e) => (r = fromMenu(e)), { once: true });
		host.querySelector(sel).dispatchEvent(new MouseEvent("click", { bubbles: true }));
		return r;
	};
	expect(hit(".own")).toBe(false);
	expect(hit(".item")).toBe(true);
	expect(hit(".deep")).toBe(true);
	expect(fromMenu(new Event("click"))).toBe(false); // no target at all
	host.remove();
});
