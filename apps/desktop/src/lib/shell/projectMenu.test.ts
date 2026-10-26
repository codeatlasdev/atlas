// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, mock, test } from "bun:test";
import { resolve } from "node:path";

// Issue #98: the space dots (the main project switcher) and the project header had no "Remove from Atlas…"; a right-click
// on a dot went straight to the icon editor. They now share the project row's actions. Dynamic imports on purpose:
// the Svelte loader registered by test/setup.ts and the module mocks must exist before these modules load
// (static imports are hoisted above them).
await (await import("../../../test/uiKit.ts")).installUiKit();
const removed: string[] = [];
mock.module(resolve(import.meta.dir, "../components/sidebar/projects.ts"), () => ({
	removeProject: async (id: string) => void removed.push(id),
}));
const calls: [string, ...unknown[]][] = [];
mock.module(resolve(import.meta.dir, "../api.ts"), () => ({
	api: {
		pin: async (...a: unknown[]) => void calls.push(["pin", ...a]),
		reveal: async (...a: unknown[]) => void calls.push(["reveal", ...a]),
	},
}));
mock.module(resolve(import.meta.dir, "../stores/projects.svelte.ts"), () => ({
	store: { run: (_key: string, f: () => Promise<unknown>) => f() },
}));
const identities = { editing: null as string | null };
mock.module(resolve(import.meta.dir, "../stores/identity.svelte.ts"), () => ({ identities }));
const { projectMenuItems } = await import("./projectMenu.ts");

const repo = (over = {}) => ({ id: "a", name: "a", root: "/r/a", parent: null, pinned: false, ...over });
const labels = (p) => projectMenuItems(p).map((i) => i.label);
const item = (p, label) => projectMenuItems(p).find((i) => i.label === label);

test("a top-level project offers change icon, pin, reveal and Remove from Atlas…", () => {
	expect(labels(repo())).toEqual([
		"Change icon and color…",
		"Pin to top",
		"Reveal in Finder",
		"Remove from Atlas…",
	]);
	expect(labels(repo({ pinned: true }))).toContain("Unpin");
});

test("Remove is the destructive item, last, in its own group", () => {
	const items = projectMenuItems(repo());
	const last = items.at(-1);
	expect(last.destructive).toBe(true);
	expect(last.group).not.toBe(items.at(-2).group);
});

test("a worktree child never offers Remove or the icon editor (it has no identity of its own)", () => {
	const l = labels(repo({ id: "a-wt", parent: "a" }));
	expect(l).not.toContain("Remove from Atlas…");
	expect(l).not.toContain("Change icon and color…");
});

test("a missing folder offers Remove but not Reveal", () => {
	const l = labels(repo({ missing: true }));
	expect(l).toContain("Remove from Atlas…");
	expect(l).not.toContain("Reveal in Finder");
});

test("choosing Remove runs the shared remove flow for that project", () => {
	removed.length = 0;
	item(repo({ id: "zed" }), "Remove from Atlas…").run();
	expect(removed).toEqual(["zed"]);
});

test("choosing Change icon opens the identity editor on that project", () => {
	item(repo({ id: "zed" }), "Change icon and color…").run();
	expect(identities.editing).toBe("zed");
	identities.editing = null;
});

test("pin and reveal go through the same commands as the row menu", async () => {
	calls.length = 0;
	item(repo({ id: "a", pinned: true }), "Unpin").run();
	item(repo({ id: "a" }), "Reveal in Finder").run();
	await Promise.resolve();
	expect(calls).toEqual([
		["pin", "a", false],
		["reveal", "a"],
	]);
});

// Wiring: the surfaces of #98 must go through the shared items / remove flow, not their own copies.
import { readFileSync } from "node:fs";
const src = (rel: string) => readFileSync(resolve(import.meta.dir, rel), "utf8");

test("the space dots right-click opens the project menu instead of jumping to the icon editor", () => {
	const dots = src("SpaceDots.svelte");
	expect(dots).toContain("projectMenuItems");
	expect(dots).toContain("<PointerMenu");
	expect(dots).not.toContain("identities.editing");
});

test("the project header removes through the shared flow, for top-level projects only", () => {
	const header = src("../../routes/(app)/ProjectHeader.svelte");
	expect(header).toMatch(
		/\{#if !p\.parent\}\s*<DropdownMenuItem[^>]*onclick=\{\(\) => removeProject\(p\.id\)\}/,
	);
	expect(header).not.toContain("api.remove");
});
