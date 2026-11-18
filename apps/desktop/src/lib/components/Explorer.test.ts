// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterAll, afterEach, beforeAll, beforeEach, expect, mock, test } from "bun:test";
import { prefsMock } from "../../../test/prefsMock.ts";
import { stubLayout } from "../../../test/viewport.ts";

// Regression #308: an expanded folder rendered every child (a 5000-entry folder: 5000 rows with an inline icon each).
// The tree now windows its rows; the keyboard model (cursor by path, Home/End/arrows/fold) must reach rows that are
// not in the DOM.
let folders: Record<string, { name: string; kind: string; size: number; mtime: number }[]> = {};
const file = (name) => ({ name, kind: "file", size: 1, mtime: 0 });
const dir = (name) => ({ name, kind: "dir", size: 0, mtime: 0 });

// `mock.module` is process-wide and the first mock of `@tauri-apps/api/core` any suite installs is the one `host` links
// to, so the folder listing is served by swapping `host`'s own methods (a plain object) for the length of this file.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
mock.module(
	"#lib/shell/ProjectIcon.svelte",
	async () => await import("../../../test/ProjectIconStub.svelte"),
);
// The tree reads the panel preferences (indent, icons); the real store loads themes through Vite-only globs.
mock.module("#lib/stores/prefs.svelte.ts", () =>
	prefsMock({ project_panel: { indent_size: 12, folder_icons: true, file_icons: true } }),
);
// happy-dom does no layout (and another suite leaves a global `offsetHeight` behind): a 440px panel of 22px rows.
let restoreLayout;
beforeEach(() => (restoreLayout = stubLayout(440, 22)));
afterEach(() => restoreLayout());
globalThis.ResizeObserver ??= class {
	observe() {}
	unobserve() {}
	disconnect() {}
};
globalThis.requestAnimationFrame = (f) => setTimeout(() => f(0), 0);

const { host } = await import("#lib/host.ts");
const fake = {
	fsList: async (path) => folders[path.replace(/^\//, "")] ?? [],
	fsWatch: async () => {},
	fsUnwatch: async () => {},
};
const real = {};
beforeAll(() => {
	for (const k of Object.keys(fake)) real[k] = host[k];
	Object.assign(host, fake);
});
afterAll(() => Object.assign(host, real));
const { flushSync, mount, tick, unmount } = await import("svelte");
const { default: Explorer } = await import("./Explorer.svelte");

let done;
afterEach(() => done?.());

async function open(root) {
	folders = root;
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(Explorer, { target, props: { worktreeId: "w" } }));
	done = () => (unmount(app), target.remove());
	for (let i = 0; i < 6; i++) await tick();
	await new Promise((r) => setTimeout(r, 20));
	return target;
}
const rows = (t) => [...t.querySelectorAll('[role="treeitem"]')];
const names = (t) => rows(t).map((r) => r.dataset.rel);
const press = (t, key, init = {}) => {
	const el = t.querySelector('[role="tree"]');
	(document.activeElement?.closest('[role="tree"]')
		? document.activeElement
		: (rows(t)[0] ?? el)
	).dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
	flushSync();
};

const many = (n) => Array.from({ length: n }, (_, i) => file(`file${String(i).padStart(5, "0")}.ts`));

test("a 5000-entry folder keeps fewer than 150 rows in the DOM", async () => {
	const t = await open({ "": many(5000) });
	expect(rows(t).length).toBeGreaterThan(10);
	expect(rows(t).length).toBeLessThan(150);
	expect(names(t)[0]).toBe("file00000.ts");
});

test("End reaches the last entry, which was not rendered, and Home comes back", async () => {
	const t = await open({ "": many(5000) });
	rows(t)[0].focus();
	press(t, "End");
	for (let i = 0; i < 4; i++) await tick();
	expect(names(t)).toContain("file04999.ts");
	expect(document.activeElement?.dataset.rel).toBe("file04999.ts");
	expect(rows(t).length).toBeLessThan(150);
	press(t, "Home");
	for (let i = 0; i < 4; i++) await tick();
	expect(document.activeElement?.dataset.rel).toBe("file00000.ts");
});

test("an expanded folder's children follow it, indented, and fold away again", async () => {
	const t = await open({ "": [dir("src"), file("z.md")], src: many(3000) });
	expect(names(t)).toEqual(["src", "z.md"]);
	rows(t)[0].focus();
	press(t, "ArrowRight"); // unfold the focused folder
	for (let i = 0; i < 6; i++) await tick();
	await new Promise((r) => setTimeout(r, 20));
	expect(names(t)[0]).toBe("src");
	expect(names(t)[1]).toBe("src/file00000.ts");
	expect(rows(t).length).toBeLessThan(150);
	const pad = (el) => parseInt(el.style.paddingLeft);
	expect(pad(rows(t)[1])).toBeGreaterThan(pad(rows(t)[0]));
	press(t, "ArrowLeft");
	for (let i = 0; i < 4; i++) await tick();
	expect(names(t)).toEqual(["src", "z.md"]);
});
