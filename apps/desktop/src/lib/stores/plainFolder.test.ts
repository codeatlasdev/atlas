// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { beforeEach, expect, mock, test } from "bun:test";
import views from "../__fixtures__/project-views.json";

// Visual check of #513: a plain folder opened on the Panel tab ("No services yet… Set up services") under a "main"
// crumb. The workspace is the real store, fed the REAL serialized views (fixture written by `cmds/projects.rs`): a
// folder has no Panel (it is the pinned services tab) and no worktree crumb, and "Use as project" brings the Panel back.
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
await (await import("../../../test/uiKit.ts")).installUiKit();
const { ws } = await import("#lib/stores/workspace.svelte.ts");
const { worktrees } = await import("#lib/stores/worktrees.svelte.ts");
const { docks, showExplorerOnce } = await import("#lib/stores/docks.svelte.ts");

const panels = (id: string) => ws.tabsOf(id).filter((t) => t.kind === "services");
const folder = () => views.folder;
const asProject = () => ({ ...views.folder, kind: "project", config_source: "detected" });

beforeEach(() => {
	ws.doc.worktrees = [];
	ws.doc.tabs = [];
	ws.doc.groups = [];
	ws.doc.layouts = {};
});

test("a plain folder has no Panel tab and is flagged plain for the crumb", () => {
	worktrees.sync([folder()]);
	ws.select("dl");
	expect(ws.worktree("dl").plain).toBe(true);
	expect(panels("dl")).toEqual([]);
	// Selecting again, or the session's ensure pass, must not bring it back.
	ws.ensurePanel("dl");
	expect(panels("dl")).toEqual([]);
});

test("a project keeps its pinned Panel, with the worktree not plain", () => {
	worktrees.sync([views.detected]);
	ws.select("shop");
	expect(ws.worktree("shop").plain).toBe(false);
	expect(panels("shop")).toHaveLength(1);
});

test("an old workspace that already holds a Panel for the folder loses it", () => {
	worktrees.sync([asProject()]);
	ws.select("dl");
	expect(panels("dl")).toHaveLength(1);
	worktrees.sync([folder()]);
	ws.ensurePanel("dl");
	expect(panels("dl")).toEqual([]);
});

test("Use as project brings the Panel back as soon as the project list reloads", () => {
	worktrees.sync([folder()]);
	ws.select("dl");
	expect(panels("dl")).toEqual([]);
	worktrees.sync([asProject()]);
	expect(ws.worktree("dl").plain).toBe(false);
	expect(panels("dl")).toHaveLength(1);
});

test("the explorer opens once for a plain folder, and never again after the user closed it", () => {
	docks.left = false;
	docks.leftPanel = "scm";
	showExplorerOnce("dl-first");
	expect([docks.left, docks.leftPanel]).toEqual([true, "explorer"]);
	docks.left = false; // the user closes it
	showExplorerOnce("dl-first");
	expect(docks.left).toBe(false);
	showExplorerOnce("dl-second");
	expect(docks.left).toBe(true);
});
