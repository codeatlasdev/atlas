// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { afterEach, beforeEach, expect, jest, test } from "bun:test";
import { FileProbe } from "./termPathExists.ts";

const root = "/w/proj";
let stats: string[] = [];
let kinds: Record<string, string> = {};
const stat = async (abs: string) => {
	stats.push(abs);
	const kind = kinds[abs];
	if (!kind) throw new Error("not found");
	return { kind };
};

beforeEach(() => {
	jest.useFakeTimers();
	stats = [];
	kinds = { "/w/proj/src/a.ts": "file", "/w/proj/src": "dir" };
});
// Fake timers left on hang every later file's real timeouts (bun runs all files in one process).
afterEach(() => jest.useRealTimers());

// ⌘-hover must not light up `example.com` or `foo.bar` just because they look like `name.ext`: a path
// underlines only when the file is on disk. `fileTarget` checks the shape and the worktree boundary,
// this checks the disk.
test("a path underlines only when it is an existing file of the worktree", async () => {
	const p = new FileProbe(stat);
	expect(await p.isFile("src/a.ts:12:3", null, root)).toBe(true);
	expect(await p.isFile("example.com", null, root)).toBe(false);
	expect(await p.isFile("src", null, root)).toBe(false);
	expect(await p.isFile("../../etc/passwd", null, root)).toBe(false);
	expect(stats).toEqual(["/w/proj/src/a.ts", "/w/proj/example.com"]);
});

test("a relative path resolves against the terminal's cwd", async () => {
	kinds["/w/proj/apps/web/a.ts"] = "file";
	const p = new FileProbe(stat);
	expect(await p.isFile("a.ts", "/w/proj/apps/web", root)).toBe(true);
});

// Moving the pointer across a path crosses many cells: one stat, not one per cell.
test("the answer is cached per file for a few seconds, then asked again", async () => {
	const p = new FileProbe(stat);
	await p.isFile("src/a.ts:1", null, root);
	await p.isFile("src/a.ts:9:9", null, root);
	expect(stats).toHaveLength(1);
	jest.advanceTimersByTime(6000);
	await p.isFile("src/a.ts", null, root);
	expect(stats).toHaveLength(2);
});

// A file created after a miss (a build wrote it) should light up soon, not stay a miss forever.
test("a miss is remembered only briefly", async () => {
	const p = new FileProbe(stat);
	expect(await p.isFile("new.ts", null, root)).toBe(false);
	kinds["/w/proj/new.ts"] = "file";
	expect(await p.isFile("new.ts", null, root)).toBe(false);
	jest.advanceTimersByTime(2500);
	expect(await p.isFile("new.ts", null, root)).toBe(true);
});

test("many files do not grow the cache without bound", async () => {
	const p = new FileProbe(stat);
	for (let i = 0; i < 600; i++) await p.isFile(`f${i}.ts`, null, root);
	expect(p.size).toBeLessThanOrEqual(256);
});
