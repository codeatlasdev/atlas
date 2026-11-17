// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import plans from "./__fixtures__/open-plans.json";

// Issue #499: opening one `.md` registered its folder as a project and showed the whole project UI. `openPath` is the
// one door for `atlas <path>`, Finder's "Open With", a drop and `path_open`: for a loose text file it must open the
// light editor and register NOTHING; for a file of a project, or a folder, it opens the project as before. The
// decision comes from the backend's REAL `path_locate` reply (fixture written by `cmds/projects.rs` tests), and the
// fake `api` records what the page asks the registry to do.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts and the mocks must exist before these load.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
await (await import("../../test/uiKit.ts")).installUiKit();
const { openPath, openLooseInFolder } = await import("./ctl.ts");
const { loose } = await import("#lib/stores/loose.svelte.ts");
const { ws } = await import("#lib/stores/workspace.svelte.ts");

let registry: { id: string; root: string; promoted: boolean }[];
let calls: string[];
let opened: [string, string][];
let located: Record<string, unknown>;

beforeEach(() => {
	registry = [];
	calls = [];
	opened = [];
	located = {};
	loose.files.splice(0);
	loose.active = null;
	ws.view = "worktree";
	const store = {
		selected: null,
		refresh: async () => calls.push("refresh"),
		loadConfig: async () => {},
	};
	window.__atlasCtlStores = {
		store,
		ws: { view: "worktree", select: () => {} },
		host: { fsStat: async () => ({ kind: "file", size: 1, mtime: 1 }) },
		api: {
			locate: async (path) => {
				calls.push(`locate ${path}`);
				return located[path];
			},
			add: async (path) => {
				calls.push(`add ${path}`);
				const p = { id: `p${registry.length}`, root: path, promoted: false };
				registry.push(p);
				return p;
			},
			promote: async (id) => {
				calls.push(`promote ${id}`);
				registry.find((p) => p.id === id).promoted = true;
			},
		},
	};
});
afterEach(() => delete window.__atlasCtlStores);

test("a loose Markdown file opens in the light editor and registers no project", async () => {
	const file = "/Users/m/Downloads/notes.md";
	located[file] = { ...plans.loose, root: "/Users/m/Downloads" };
	const r = await openPath(file, 7);
	expect(r).toEqual({ id: null, root: "/Users/m/Downloads", path: file, standalone: true });
	expect(registry).toEqual([]);
	expect(calls).toEqual([`locate ${file}`]);
	expect(loose.files).toEqual([file]);
	expect(loose.active).toBe(file);
	expect(ws.view).toBe("file");
	expect(loose.takeLine(file)).toBe(7);
});

test("opening the same loose file twice shows one entry; closing the last returns to the project view", async () => {
	const file = "/Users/m/Downloads/notes.md";
	located[file] = plans.loose;
	await openPath(file);
	await openPath(file);
	expect(loose.files).toEqual([file]);
	loose.close(file);
	expect(loose.files).toEqual([]);
	expect(loose.active).toBeNull();
	expect(ws.view).toBe("worktree");
});

test("a file of a project, a folder and a loose image all open through the project registry", async () => {
	located["/Users/m/repo/README.md"] = { standalone: false, root: "/Users/m/repo" };
	await openPath("/Users/m/repo/README.md");
	expect(registry.map((p) => p.root)).toEqual(["/Users/m/repo"]);
	expect(loose.files).toEqual([]);

	located["/Users/m/Downloads"] = { standalone: false, root: "/Users/m/Downloads" };
	await openPath("/Users/m/Downloads");
	// A picture has no light viewer: it opens in its folder, the plain-folder mode, with the full viewers.
	located["/Users/m/Pictures/a.png"] = { standalone: true, root: "/Users/m/Pictures" };
	await openPath("/Users/m/Pictures/a.png");
	expect(registry.map((p) => p.root)).toEqual(["/Users/m/repo", "/Users/m/Downloads", "/Users/m/Pictures"]);
	expect(loose.files).toEqual([]);
});

test("Open folder registers the file's folder and moves the file there; Open as project also promotes it", async () => {
	const file = "/Users/m/Downloads/sub/notes.md";
	loose.open(file);
	const plain = await openLooseInFolder(file, false);
	expect(registry.map((p) => [p.root, p.promoted])).toEqual([["/Users/m/Downloads/sub", false]]);
	expect(plain.root).toBe("/Users/m/Downloads/sub");
	expect(loose.files).toEqual([]);

	loose.open(file);
	await openLooseInFolder(file, true);
	expect(registry.at(-1).promoted).toBe(true);
	expect(calls).toContain(`promote ${registry.at(-1).id}`);
	expect(loose.files).toEqual([]);
});
