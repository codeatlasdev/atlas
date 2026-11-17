// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterAll, beforeAll, beforeEach, expect, mock, test } from "bun:test";

// Issue #499: Quick Open lists a worktree's files from a cache. For a plain folder the source is a bounded walk
// (up to two seconds), so the cache has to survive between ⌘P presses yet drop the moment the watcher reports a change
// under the folder; before, it only expired by time (5 s). The real store runs against a fake `git.files` and the
// host-event stream, which the test drives the way hostd's `fs_event` batches arrive.
let handler: ((e: { payload: unknown }) => void) | undefined;
let listens = 0;
mock.module("@tauri-apps/api/event", () => ({
	listen: async (_name: string, fn: typeof handler) => {
		listens++;
		handler = fn;
		return () => {};
	},
}));
await (await import("../../../test/uiKit.ts")).installUiKit();
const { git } = await import("#lib/api.ts");
const { ws } = await import("#lib/stores/workspace.svelte.ts");
const { worktreeFiles, touchesFiles } = await import("./files.svelte.ts");

let calls: string[];
let listing: string[];
const realFiles = git.files;
beforeAll(() => {
	git.files = async (id: string) => {
		calls.push(id);
		return [...listing];
	};
});
afterAll(() => (git.files = realFiles));

let n = 0;
let id: string;
beforeEach(() => {
	calls = [];
	listing = ["a.md"];
	id = `w${++n}`;
	ws.doc.worktrees = [{ id, path: `/Users/m/${id}` }];
});

const fsEvent = (...paths: string[]) => handler!({ payload: { ev: "fs_event", paths } });

test("a second Quick Open reads the cache, a change under the folder drops it", async () => {
	expect(await worktreeFiles(id)).toEqual(["a.md"]);
	expect(await worktreeFiles(id)).toEqual(["a.md"]);
	expect(calls).toEqual([id]);

	listing = ["a.md", "b.md"];
	fsEvent(`/Users/m/${id}/b.md`);
	expect(await worktreeFiles(id)).toEqual(["a.md", "b.md"]);
	expect(calls).toEqual([id, id]);
});

test("events from other folders, other kinds of event and .git noise leave the cache alone", async () => {
	await worktreeFiles(id);
	fsEvent("/Users/m/elsewhere/x.md");
	fsEvent(`/Users/m/${id}-two/x.md`);
	fsEvent(`/Users/m/${id}/.git/index`);
	handler!({ payload: { ev: "pty_data", paths: [`/Users/m/${id}/x.md`] } });
	await worktreeFiles(id);
	expect(calls).toEqual([id]);
});

test("one listener serves every worktree, and only the touched worktree is refetched", async () => {
	const other = `${id}b`;
	ws.doc.worktrees = [
		{ id, path: `/Users/m/${id}` },
		{ id: other, path: `/Users/m/${other}` },
	];
	await worktreeFiles(id);
	await worktreeFiles(other);
	fsEvent(`/Users/m/${other}/new.txt`);
	await worktreeFiles(id);
	await worktreeFiles(other);
	expect(calls).toEqual([id, other, other]);
	expect(listens).toBe(1);
});

test("touchesFiles: inside the folder yes, siblings with its prefix and .git no", () => {
	expect(touchesFiles("/p/app", ["/p/app/src/a.ts"])).toBe(true);
	expect(touchesFiles("/p/app/", ["/p/app/a.ts"])).toBe(true);
	expect(touchesFiles("/p/app", ["/p/app-two/a.ts", "/p/app/.git/HEAD"])).toBe(false);
	expect(touchesFiles("/p/app", undefined)).toBe(false);
	expect(touchesFiles("", ["/p/app/a.ts"])).toBe(false);
});
