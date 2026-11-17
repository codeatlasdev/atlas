// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterAll, expect, mock, test } from "bun:test";
import { resolve } from "node:path";
import { projectsMock } from "../../../test/projectsMock.ts";
import { showFirstProject } from "./flow.ts";

// Regression: 1.5.0-alpha.26 first-user pass (01-welcome): a window with no project and no first-run overlay (an agent
// instance, or a welcome dismissed without finishing) showed a blank main pane under the checklist.
test("a no-project window gets the first-project state unless the overlay or the standing hero owns it", () => {
	const open = { done: false };
	const finished = { done: true };
	expect(showFirstProject(open, true, 0, false)).toBe(true);
	expect(showFirstProject(open, true, 0, true)).toBe(false); // the first-run overlay is up
	expect(showFirstProject(finished, true, 0, false)).toBe(false); // the standing Hero draws it
	expect(showFirstProject(open, false, 0, false)).toBe(false); // still loading
	expect(showFirstProject(open, true, 2, false)).toBe(false); // has projects
});

const calls = [];
mock.module(resolve(import.meta.dir, "../components/sidebar/projects.ts"), () =>
	projectsMock({ addProject: () => calls.push("add"), showDiscover: () => calls.push("find") }),
);
// The standing Hero draws the globe with the theme store, which bun cannot load (Vite-only `import.meta.glob`).
mock.module(resolve(import.meta.dir, "./Hero.svelte"), () => ({ default: () => {} }));
mock.module("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
await (await import("../../../test/uiKit.ts")).useRealKit("EmptyState", "Button");
const { flushSync, mount, unmount, tick } = await import("svelte");
const { store } = await import("#lib/stores/projects.svelte.ts");
const { onboarding } = await import("./state.svelte.ts");
const { default: NoProject } = await import("../../routes/(app)/NoProject.svelte");

test("the pane offers Add project… and Find projects on this machine, and says what Atlas does", async () => {
	window.__ATLAS_AGENT__ = true; // no first-run overlay, exactly the instance in the pass
	store.loaded = true;
	store.projects = [];
	onboarding.loaded = true;
	onboarding.state = { ...onboarding.state, done: false };
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(NoProject, { target }));
	await tick();
	try {
		const buttons = [...target.querySelectorAll("button")];
		const add = buttons.find((b) => b.textContent.trim() === "Add project…");
		const find = buttons.find((b) => b.textContent.trim() === "Find projects on this machine");
		expect(add && find).toBeTruthy();
		expect(target.textContent).toMatch(/services, terminals and coding agents/);
		add.click();
		find.click();
		expect(calls).toEqual(["add", "find"]);
	} finally {
		unmount(app);
		target.remove();
		delete window.__ATLAS_AGENT__;
	}
});
