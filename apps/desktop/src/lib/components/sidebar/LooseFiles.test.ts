// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";

// Visual check of #513: every loose file stayed its own top-level row of the sidebar (12 opened files pushed the
// projects out of sight). They live in ONE collapsible group, newest first, capped, each closable. The real component
// runs over the real `loose` store and workspace.
mock.module("@tauri-apps/api/core", () => ({
	invoke: async () => null,
	SERIALIZE_TO_IPC_FN: Symbol("ipc"),
	Channel: class {},
	Resource: class {},
	convertFileSrc: (p: string) => p,
	transformCallback: () => 0,
}));
mock.module("@tauri-apps/api/event", () => ({
	listen: async () => () => {},
	once: async () => () => {},
	emit: async () => {},
	emitTo: async () => {},
	TauriEvent: {},
}));
await (await import("../../../../test/uiKit.ts")).installUiKit();
const { flushSync, mount, unmount, tick } = await import("svelte");
const { loose } = await import("#lib/stores/loose.svelte.ts");
const { ws } = await import("#lib/stores/workspace.svelte.ts");
const { default: LooseFiles } = await import("./LooseFiles.svelte");

let done: (() => void) | undefined;
const file = (i: number) => `/Users/m/Downloads/loose${i}.md`;

beforeEach(() => {
	loose.files.splice(0);
	loose.recent.splice(0);
	loose.active = null;
	ws.view = "worktree";
});
afterEach(() => {
	done?.();
	done = undefined;
});

function render() {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(LooseFiles, { target }));
	done = () => (unmount(app), target.remove());
	return target;
}
const settle = async () => (flushSync(), await tick());
const rows = (t: Element) => [...t.querySelectorAll("[data-loose-row]")].map((r) => r.textContent.trim());
const button = (t: Element, label: string) =>
	[...t.querySelectorAll("button")].find(
		(b) => b.getAttribute("aria-label") === label || b.textContent.trim().startsWith(label),
	);

test("no loose file, no group", () => {
	expect(render().querySelector("[data-loose-group]")).toBeNull();
});

test("twelve open files are one group of five rows, newest first, with Show all", async () => {
	for (let i = 1; i <= 12; i++) loose.open(file(i));
	const t = render();
	expect(t.querySelectorAll("[data-loose-group]")).toHaveLength(1);
	expect(rows(t)).toEqual(["loose12.md", "loose11.md", "loose10.md", "loose9.md", "loose8.md"]);
	expect(t.textContent).toContain("12");
	button(t, "Show all").click();
	await settle();
	expect(rows(t)).toHaveLength(12);
	expect(rows(t)[11]).toBe("loose1.md");
	button(t, "Show fewer").click();
	await settle();
	expect(rows(t)).toHaveLength(5);
});

test("opening a file again moves it to the top and never duplicates it", async () => {
	for (let i = 1; i <= 3; i++) loose.open(file(i));
	const t = render();
	loose.open(file(1));
	await settle();
	expect(rows(t)).toEqual(["loose1.md", "loose3.md", "loose2.md"]);
	expect(loose.files).toHaveLength(3);
});

test("a row opens its file in the light editor", async () => {
	loose.open(file(1));
	loose.open(file(2));
	ws.view = "worktree";
	const t = render();
	t.querySelectorAll("[data-loose-row]")[1].querySelector(".name").click();
	await settle();
	expect(loose.active).toBe(file(1));
	expect(ws.view).toBe("file");
});

test("the x and a middle click close a file; closing the last one removes the group", async () => {
	for (let i = 1; i <= 3; i++) loose.open(file(i));
	const t = render();
	button(t, "Close loose3.md").click();
	await settle();
	expect(rows(t)).toEqual(["loose2.md", "loose1.md"]);
	expect(loose.files).toEqual([file(1), file(2)]);
	t.querySelectorAll("[data-loose-row]")[0].dispatchEvent(
		new MouseEvent("auxclick", { button: 1, bubbles: true }),
	);
	await settle();
	expect(rows(t)).toEqual(["loose1.md"]);
	button(t, "Close loose1.md").click();
	await settle();
	expect(t.querySelector("[data-loose-group]")).toBeNull();
	expect(loose.files).toEqual([]);
});

test("closing from the light editor's own strip removes the row too", async () => {
	loose.open(file(1));
	loose.open(file(2));
	const t = render();
	loose.close(file(2));
	await settle();
	expect(rows(t)).toEqual(["loose1.md"]);
});

test("the group folds, and the fold keeps the count", async () => {
	for (let i = 1; i <= 3; i++) loose.open(file(i));
	const t = render();
	t.querySelector("[data-loose-group] > button").click();
	await settle();
	expect(rows(t)).toEqual([]);
	expect(t.querySelector("[data-loose-group]").textContent).toContain("3");
});
