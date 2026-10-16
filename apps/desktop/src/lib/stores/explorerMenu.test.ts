// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { duplicateName, menuItems } from "./explorerMenu.ts";

const actions = (t) => menuItems(t).map((i) => i.action);

// Incident: right-click on an Explorer row opened no menu (#64); the same list now backs `…` and right-click.
test("a file offers Open and no New entries; a folder offers New file/folder and no Open", () => {
	const file = actions({ kind: "file", rel: "a.ts" });
	expect(file).toContain("open");
	expect(file).not.toContain("new-file");
	expect(file).not.toContain("new-folder");
	const dir = actions({ kind: "dir", rel: "src" });
	expect(dir).toEqual(expect.arrayContaining(["new-file", "new-folder"]));
	expect(dir).not.toContain("open");
});

// The issue asks for "Open to the side" and "Open in terminal here": a file opens beside, a folder (or the root, or a
// file's folder) is where the terminal starts.
test("files open to the side; every target but a bare file offers a terminal in its folder", () => {
	expect(actions({ kind: "file", rel: "a.ts" })).toContain("open-side");
	expect(actions({ kind: "dir", rel: "d" })).not.toContain("open-side");
	expect(actions({ kind: "root" })).not.toContain("open-side");
	for (const t of [{ kind: "file", rel: "a.ts" }, { kind: "dir", rel: "d" }, { kind: "root" }]) {
		expect(actions(t)).toContain("terminal");
	}
});

test("every row can be renamed, duplicated, copied, revealed and deleted; delete is last and destructive", () => {
	for (const kind of ["file", "dir"]) {
		const items = menuItems({ kind, rel: "x" });
		expect(items.map((i) => i.action)).toEqual(
			expect.arrayContaining(["rename", "duplicate", "copy-path", "copy-relative-path", "reveal", "delete"]),
		);
		const last = items.at(-1);
		expect(last.action).toBe("delete");
		expect(last.destructive).toBe(true);
	}
});

test("the empty area (root) can only create, reveal and copy the path: never rename or delete the worktree", () => {
	expect(actions({ kind: "root" })).toEqual(["new-file", "new-folder", "terminal", "reveal", "copy-path"]);
});

test("git items follow the status: untracked can only be staged, modified can also be discarded, ignored and folders get none", () => {
	expect(actions({ kind: "file", rel: "a", status: "untracked" })).toContain("stage");
	expect(actions({ kind: "file", rel: "a", status: "untracked" })).not.toContain("discard");
	expect(actions({ kind: "file", rel: "a", status: "modified" })).toEqual(
		expect.arrayContaining(["stage", "discard"]),
	);
	expect(actions({ kind: "file", rel: "a", status: "ignored" })).not.toContain("stage");
	expect(actions({ kind: "file", rel: "a" })).not.toContain("stage");
	// A folder is `modified` when a file under it changed, but git add/checkout on a folder is not offered here.
	expect(actions({ kind: "dir", rel: "d", status: "modified" })).not.toContain("stage");
});

test("separator groups never decrease down the list, so a divider is never drawn between two items of one group", () => {
	for (const t of [
		{ kind: "file", rel: "a", status: "modified" },
		{ kind: "dir", rel: "d" },
		{ kind: "root" },
	]) {
		const groups = menuItems(t).map((i) => i.group);
		expect([...groups].sort((a, b) => a - b)).toEqual(groups);
	}
});

test("duplicate names keep the extension, skip taken names and number further copies", () => {
	expect(duplicateName("a.ts", new Set(["a.ts"]))).toBe("a copy.ts");
	expect(duplicateName("a.ts", new Set(["a.ts", "a copy.ts"]))).toBe("a copy 2.ts");
	expect(duplicateName("src", new Set(["src"]))).toBe("src copy");
	expect(duplicateName("archive.tar.gz", new Set())).toBe("archive.tar copy.gz");
	// A dotfile has no extension: `.env` → `.env copy`, not ` copy.env`.
	expect(duplicateName(".env", new Set([".env"]))).toBe(".env copy");
});
