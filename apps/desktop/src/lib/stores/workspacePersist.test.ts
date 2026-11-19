// @ts-nocheck bun test runtime types are not part of the app tsconfig
// A workspace.json that cannot be used must never keep the app from opening. Incident: `workspace_load` rejected on a
// file that was not valid JSON (a crash mid-write on an old build, a full disk, a hand edit), `ws.loaded` was never set,
// the ready gate never opened and the window sat on its loading state for good.
import { beforeEach, expect, mock, test } from "bun:test";

const calls: [string, unknown][] = [];
let handlers: Record<string, (args: unknown) => unknown> = {};
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args: unknown) => {
		calls.push([cmd, args]);
		const h = handlers[cmd];
		if (!h) throw new Error(`unexpected invoke ${cmd}`);
		return h(args);
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
const toasts: string[] = [];
mock.module("@codeatlasdev/ui", () => ({
	toast: { error: (t: string) => toasts.push(t), info() {}, success() {} },
}));
mock.module("#lib/api.ts", () => ({ errText: String }));
mock.module("#lib/errors/errors.svelte.ts", () => ({ errors: { captureHost() {}, hostReachable() {} } }));
const { WorkspacePersist } = await import("./workspacePersist.svelte.ts");
const { docShapeError, emptyDoc } = await import("./workspaceTypes.ts");

const target = () => ({
	doc: emptyDoc(),
	loaded: false,
	titles: {},
	exited: {},
	hostError: null,
	syncPtys: async () => {},
});
const KEPT = "/home/u/.atlas/desktop/workspace.json.corrupt-1763500000000";
// The keys of `workspace::WorkspaceLoad` / `Recovered` (crates/atlas-desktop/src/workspace.rs pins the same shapes).
const recovered = (o = {}) => ({
	path: "/home/u/.atlas/desktop/workspace.json",
	kept: KEPT,
	error: null,
	...o,
});
const load = (doc, rec = null) => ({ doc, recovered: rec });

beforeEach(() => {
	calls.length = 0;
	toasts.length = 0;
	handlers = {};
});

async function boot(result: unknown) {
	handlers.workspace_load = () => result;
	handlers.workspace_set_aside = () => recovered();
	handlers.workspace_save = () => null;
	const ws = target();
	const persist = new WorkspacePersist(ws);
	await persist.init();
	return Object.assign(ws, { saving: persist.saving });
}

test("a file Rust set aside (invalid JSON, an empty file): the store loads empty, saves, and says where the old file is", async () => {
	const ws = await boot(load(null, recovered()));
	expect(ws.loaded).toBe(true);
	expect(ws.doc).toEqual(emptyDoc());
	expect(ws.saving).toBe(true);
	expect(toasts).toHaveLength(1);
	expect(toasts[0]).toContain(KEPT);
	expect(calls.map((c) => c[0])).not.toContain("workspace_set_aside");
});

test("a missing file is a first run: loaded, empty, no notice, nothing set aside", async () => {
	const ws = await boot(load(null));
	expect(ws.loaded).toBe(true);
	expect(ws.doc).toEqual(emptyDoc());
	expect(ws.saving).toBe(true);
	expect(toasts).toEqual([]);
	expect(calls.map((c) => c[0])).not.toContain("workspace_set_aside");
});

test("a valid document is used as it is", async () => {
	const saved = {
		...emptyDoc(),
		selected: "w1",
		worktrees: [{ id: "w1", repoId: "r", path: "/p", name: "p", isMain: true }],
	};
	const ws = await boot(load(saved));
	expect(ws.doc).toEqual(saved);
	expect(toasts).toEqual([]);
});

// Valid JSON that is not a workspace: the schema is the page's, so the page asks Rust to set the file aside.
for (const [what, doc] of [
	["a list", []],
	["a number", 7],
	["tabs that are not a list", { ...emptyDoc(), tabs: "x" }],
	["a worktree that is not an object", { ...emptyDoc(), worktrees: [5] }],
	["layouts that are a list", { ...emptyDoc(), layouts: [] }],
	["a tab without an id", { ...emptyDoc(), tabs: [{ worktreeId: "w" }] }],
	["a selected that is a number", { ...emptyDoc(), selected: 3 }],
]) {
	test(`valid JSON of the wrong shape (${what}) is set aside: the store loads empty and says where the file went`, async () => {
		const ws = await boot(load(doc));
		expect(ws.loaded).toBe(true);
		expect(ws.doc).toEqual(emptyDoc());
		expect(calls.map((c) => c[0])).toContain("workspace_set_aside");
		expect(ws.saving).toBe(true);
		expect(toasts).toHaveLength(1);
		expect(toasts[0]).toContain(KEPT);
	});
}

// The old file is still where it was (a permissions or I/O failure): never save over it.
test("a file that could not be set aside is reported, and this session does not save over it", async () => {
	const ws = await boot(load(null, recovered({ kept: null, error: "Permission denied (os error 13)" })));
	expect(ws.loaded).toBe(true);
	expect(ws.doc).toEqual(emptyDoc());
	expect(ws.saving).toBe(false);
	expect(toasts).toHaveLength(1);
	expect(toasts[0]).toContain("Permission denied");
	expect(toasts[0]).toContain("/home/u/.atlas/desktop/workspace.json");
});

test("a load that fails outright (the command itself rejects) still opens the app, without saving", async () => {
	handlers.workspace_save = () => null;
	handlers.workspace_load = () => {
		throw new Error("ipc down");
	};
	const ws = target();
	const persist = new WorkspacePersist(ws);
	await persist.init();
	expect(ws.loaded).toBe(true);
	expect(persist.saving).toBe(false);
	expect(toasts).toHaveLength(1);
	expect(toasts[0]).toContain("ipc down");
});

test("docShapeError names the first thing that is wrong and accepts what the app writes", () => {
	expect(docShapeError(null)).toBeNull();
	expect(docShapeError(emptyDoc())).toBeNull();
	expect(docShapeError({})).toBeNull();
	expect(docShapeError({ version: 1, extraKeyFromANewerApp: [1] })).toBeNull();
	expect(docShapeError({ tabs: "x" })).toContain("tabs");
	expect(docShapeError([])).toContain("object");
	expect(docShapeError({ closed: { w: "x" } })).toContain("closed");
});
