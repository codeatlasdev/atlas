// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	ancestorsOf,
	join,
	navMove,
	parentOf,
	statusGroups,
	statusMap,
	statusOf,
	visibleRows,
} from "./tree.ts";
import gitStatus from "../__fixtures__/git-status.json";

const e = (name, kind = "file") => ({ name, kind, size: 0, mtime: 0 });

test("visible rows descend only into expanded, loaded folders", () => {
	const children = { "": [e("src", "dir"), e("a.md")], src: [e("x.ts")], lib: [e("y.ts")] };
	expect(visibleRows(children, {}).map((r) => r.rel)).toEqual(["src", "a.md"]);
	expect(visibleRows(children, { src: true }).map((r) => r.rel)).toEqual(["src", "src/x.ts", "a.md"]);
	expect(visibleRows({ "": [e("src", "dir")] }, { src: true }).map((r) => r.rel)).toEqual(["src"]);
});

test("each visible row carries its name and how many folders are above it (the indent)", () => {
	const children = { "": [e("src", "dir")], src: [e("lib", "dir"), e("a.ts")], "src/lib": [e("b.ts")] };
	expect(visibleRows(children, { src: true, "src/lib": true })).toEqual([
		{ rel: "src", name: "src", dir: true, depth: 0 },
		{ rel: "src/lib", name: "lib", dir: true, depth: 1 },
		{ rel: "src/lib/b.ts", name: "b.ts", dir: false, depth: 2 },
		{ rel: "src/a.ts", name: "a.ts", dir: false, depth: 1 },
	]);
});

test("ancestors run outermost first and are empty at the root", () => {
	expect(ancestorsOf("a/b/c.ts")).toEqual(["a", "a/b"]);
	expect(ancestorsOf("c.ts")).toEqual([]);
});

test("join and parentOf treat the root as the empty path", () => {
	expect(join("", "a.ts")).toBe("a.ts");
	expect(join("src", "a.ts")).toBe("src/a.ts");
	expect(parentOf("src/lib/a.ts")).toBe("src/lib");
	expect(parentOf("a.ts")).toBe("");
});

test("statusMap classifies entries and marks the folders above a change", () => {
	const m = statusMap([
		{ path: "src/lib/a.ts", x: " ", y: "M" },
		{ path: "src/new.ts", x: "?", y: "?" },
		{ path: "src/gone.ts", x: " ", y: "D" },
	]);
	expect(m["src/lib/a.ts"]).toBe("modified");
	expect(m["src/new.ts"]).toBe("untracked");
	expect(m["src/gone.ts"]).toBe("deleted");
	expect(m["src"]).toBe("modified");
	expect(m["src/lib"]).toBe("modified");
});

// Incident: the explorer never dimmed gitignored files (git_status did not even ask for them). The input is the REAL
// serialization of the Rust payload (`__fixtures__/git-status.json`, written by `git::tests`), never a hand-made mock.
test("real git_status payload: ignored files and folders are `ignored`, distinct from untracked and modified", () => {
	const m = statusMap(gitStatus.entries);
	expect(m[".env"]).toBe("ignored");
	expect(m["node_modules"]).toBe("ignored"); // git reports `node_modules/`: the slash is not part of the path
	expect(m["target"]).toBe("ignored");
	expect(m["new.txt"]).toBe("untracked");
	expect(m["src/n.ts"]).toBe("untracked");
	expect(m["src/m.ts"]).toBe("modified");
	expect(m["src"]).toBe("modified");
});

test("everything inside an ignored folder is ignored, though git lists only the folder", () => {
	const m = statusMap(gitStatus.entries);
	expect(statusOf(m, "node_modules/x/i.js")).toBe("ignored");
	expect(statusOf(m, "target/debug/a.o")).toBe("ignored");
	expect(statusOf(m, "src/m.ts")).toBe("modified");
	expect(statusOf(m, "src/clean.ts")).toBeUndefined();
	// a sibling that merely shares a prefix with an ignored folder is not inside it
	expect(statusOf(m, "target-notes.md")).toBeUndefined();
});

test("an ignored entry never marks its parents modified; a nested ignored file keeps its folder clean", () => {
	const m = statusMap([
		{ path: "pkg/dist/", x: "!", y: "!" },
		{ path: "pkg/.env", x: "!", y: "!" },
	]);
	expect(m["pkg"]).toBeUndefined();
	expect(m["pkg/dist"]).toBe("ignored");
	expect(m["pkg/.env"]).toBe("ignored");
	// but a real change beside them still does
	const both = statusMap([
		{ path: "pkg/dist/", x: "!", y: "!" },
		{ path: "pkg/a.ts", x: ".", y: "M" },
	]);
	expect(both["pkg"]).toBe("modified");
	expect(both["pkg/dist"]).toBe("ignored");
});

test("a tracked change wins over an ignored ancestor entry (force-added file in an ignored folder)", () => {
	const m = statusMap([
		{ path: "build/keep.txt", x: ".", y: "M" },
		{ path: "build/", x: "!", y: "!" },
	]);
	expect(statusOf(m, "build/keep.txt")).toBe("modified");
});

// Incident guard: `!` is not "a staged change". Before the filter, SourceControl listed every ignored file under both
// Staged and Changes once git_status started returning them.
test("statusGroups leaves ignored entries out of staged, changes, untracked and conflicts", () => {
	const g = statusGroups(gitStatus.entries);
	expect(g.staged).toEqual([]);
	expect(g.changes.map((e) => e.path)).toEqual(["src/m.ts"]);
	expect(g.untracked.map((e) => e.path)).toEqual(["new.txt", "src/n.ts"]);
	expect(g.conflicts).toEqual([]);
	expect([...g.staged, ...g.changes, ...g.untracked, ...g.conflicts].some((e) => e.x === "!")).toBe(false);
});

test("statusGroups: a staged+modified file is in both groups, a conflict only in conflicts", () => {
	const g = statusGroups([
		{ path: "a.ts", x: "M", y: "M" },
		{ path: "b.ts", x: "U", y: "U", conflict: true },
	]);
	expect(g.staged.map((e) => e.path)).toEqual(["a.ts"]);
	expect(g.changes.map((e) => e.path)).toEqual(["a.ts"]);
	expect(g.conflicts.map((e) => e.path)).toEqual(["b.ts"]);
});

test("navMove walks rows, folds folders and ignores other keys", () => {
	const children = { "": [e("src", "dir"), e("a.md")], src: [e("x.ts")] };
	const closed = visibleRows(children, {});
	const open = visibleRows(children, { src: true });
	expect(navMove("ArrowDown", closed, "src", {})).toEqual({ focus: "a.md" });
	expect(navMove("ArrowDown", closed, "a.md", {})).toEqual({ focus: "a.md" });
	expect(navMove("ArrowUp", closed, "src", {})).toEqual({ focus: "src" });
	expect(navMove("End", closed, "src", {})).toEqual({ focus: "a.md" });
	expect(navMove("ArrowRight", closed, "src", {})).toEqual({ toggle: "src" });
	expect(navMove("ArrowRight", open, "src", { src: true })).toEqual({ focus: "src/x.ts" });
	expect(navMove("ArrowLeft", open, "src", { src: true })).toEqual({ toggle: "src" });
	expect(navMove("ArrowLeft", open, "src/x.ts", { src: true })).toEqual({ focus: "src" });
	expect(navMove("ArrowLeft", closed, "a.md", {})).toEqual({ focus: undefined });
	expect(navMove("Enter", closed, "a.md", {})).toEqual({
		open: { rel: "a.md", name: "a.md", dir: false, depth: 0 },
	});
	expect(navMove("F2", closed, "a.md", {})).toEqual({ rename: "a.md" });
	expect(navMove("x", closed, "a.md", {})).toBeNull();
});
