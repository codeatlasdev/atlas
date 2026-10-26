// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { fillStep, mountedTabs } from "./tabMount.ts";

// Incident (space switch froze the page for 1-4 s once a space had a Changes tab): `TabGroupView` mounted a space's
// other tabs only while it was current (`rest = space.current ? extra : 0`) and `extra` was never reset. Leaving a
// space unmounted every tab that was not the active one; coming back mounted ALL of them in one synchronous pass
// inside the switch frame (extra was still N), each a full editor/diff/logs tree (15k nodes for a 400-line diff).

const tabs = ["a", "b", "c", "d"];

test("a tab that was mounted stays mounted when the space stops being current and another tab is active", () => {
	// `current` is not an input at all: nothing a switch changes can drop a tab, so nothing unmounts during the slide
	const seen = new Set(["a", "b", "c", "d"]);
	expect(mountedTabs({ tabs, active: "a", seen })).toEqual(new Set(tabs));
	expect(mountedTabs({ tabs, active: "d", seen })).toEqual(new Set(tabs));
});

test("only the active tab is mounted until the space has been current", () => {
	expect(mountedTabs({ tabs, active: "b", seen: new Set() })).toEqual(new Set(["b"]));
});

test("a neighbour that was never current mounts nothing beyond what it already showed", () => {
	// the pager's pre-mounted neighbours cost one tab each until they have been current
	expect(mountedTabs({ tabs, active: "a", seen: new Set(["a"]) })).toEqual(new Set(["a"]));
});

test("fillStep: stops once every tab is mounted, so no timer is armed for a finished space", () => {
	expect(fillStep({ tabs, mounted: new Set(["a", "b"]), current: true })).toBe(true);
	expect(fillStep({ tabs, mounted: new Set(tabs), current: true })).toBe(false);
});

test("fillStep: a space that is not current never schedules mounts", () => {
	expect(fillStep({ tabs, mounted: new Set(["a"]), current: false })).toBe(false);
});

test("fillStep: counts tabs that are mounted, not a counter (a stale counter cannot finish early)", () => {
	// the old code compared `extra >= tabs.length`: a counter left at N by an earlier visit hid tabs that were new since
	expect(fillStep({ tabs: [...tabs, "e"], mounted: new Set(tabs), current: true })).toBe(true);
});

// The component must use the shared rule, not its own copy of it (the copy is what had the bug).
test("TabGroupView decides what is mounted through mountedTabs, never from space.current directly", async () => {
	const { readFileSync } = await import("node:fs");
	const { join } = await import("node:path");
	const src = readFileSync(join(import.meta.dir, "TabGroupView.svelte"), "utf8");
	expect(src).toContain("mountedTabs(");
	expect(src).not.toMatch(/space\.current\s*\?\s*extra/);
});
