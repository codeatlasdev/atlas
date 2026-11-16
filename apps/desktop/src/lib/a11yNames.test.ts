// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { afterAll, afterEach, expect, mock, test } from "bun:test";

// First-run pass on 1.5.0-alpha.26 (`atlas desktop debug a11y`): the "…" of the project header and the "+" of a tab strip
// were a kit menu button wrapping a `<span aria-label>` (the button had no name of its own), and the sidebar's
// "Filter worktrees" and "Search sessions" fields had only a placeholder. Real kit, real components, and the app's own
// `a11yTree` (what `debug a11y` prints): no button or link may carry a `problem`, and the fields have a name.
// RightSidebar reads sessions through the Tauri bridge; a global stub (not a module mock, which would outlive this file
// for every suite in the run) that `afterAll` removes.
window.__TAURI_INTERNALS__ = { invoke: async () => [], transformCallback: () => 0 };
// The real `@tauri-apps/api/window` links `SERIALIZE_TO_IPC_FN` from `api/core`, which any other suite's `core` mock
// lacks (module mocks are process-wide): neither these components nor this test touch the window (DiffView.test.ts).
mock.module("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }),
}));
await (await import("../../test/uiKit.ts")).useRealKit("DropdownMenu", "DropdownMenu/DropdownMenuItem");
const { flushSync, mount, unmount, tick } = await import("svelte");
const { a11yTree } = await import("./debug/agent.ts");

afterAll(() => {
	delete window.__TAURI_INTERNALS__;
});

let done: (() => void) | undefined;
afterEach(() => {
	done?.();
	done = undefined;
});

function render(Component, props = {}) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(Component, { target, props }));
	done = () => (unmount(app), target.remove());
	return target;
}
const problems = (el) => {
	const out = [];
	const walk = (nodes) => nodes.forEach((n) => (n.problem && out.push(n.problem), walk(n.children ?? [])));
	walk(a11yTree(el));
	return out;
};

test("a search field with only a placeholder is still named by it; an explicit label wins", async () => {
	const { default: SearchField } = await import("./components/SearchField.svelte");
	const t = render(SearchField, { placeholder: "Filter worktrees" });
	expect(t.querySelector("input").getAttribute("aria-label")).toBe("Filter worktrees");
	done();
	const t2 = render(SearchField, { placeholder: "Search", "aria-label": "Search commands" });
	expect(t2.querySelector("input").getAttribute("aria-label")).toBe("Search commands");
});

test("the tab strip's + menu trigger is a button with its own name", async () => {
	const { default: NewTabMenu } = await import("./components/panes/NewTabMenu.svelte");
	const t = render(NewTabMenu, { groupId: "g", onrename: () => {} });
	await tick();
	const trigger = t.querySelector("[aria-haspopup=menu]");
	expect(trigger.tagName).toBe("BUTTON");
	expect(trigger.getAttribute("aria-label")).toBe("New tab");
	expect(problems(t)).toEqual([]);
});

// The project header's import graph reaches the theme store (Vite-only `import.meta.glob`), which bun cannot load, so
// the class of bug is pinned at its source: a kit menu's `trigger` snippet is wrapped in the kit's own button, so an
// `aria-label` on what it renders names a span inside an unnamed button. The name goes on a `triggerChild` button.
test("no kit menu trigger snippet carries its accessible name on an inner span", async () => {
	const { Glob } = await import("bun");
	const { readFileSync } = await import("node:fs");
	const offenders = [];
	for (const f of new Glob("src/**/*.svelte").scanSync(".")) {
		const src = readFileSync(f, "utf8");
		for (const m of src.matchAll(/\{#snippet trigger\(\)\}([\s\S]*?)\{\/snippet\}/g))
			if (/aria-label=/.test(m[1])) offenders.push(f);
	}
	expect(offenders).toEqual([]);
});

test("the sessions search and scope controls are named", async () => {
	const { default: RightSidebar } = await import("./components/RightSidebar.svelte");
	const t = render(RightSidebar);
	await tick();
	expect(t.querySelector("input").getAttribute("aria-label")).toBe("Search sessions");
	expect(problems(t)).toEqual([]);
});
