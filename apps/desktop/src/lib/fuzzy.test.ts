// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { fuzzy, highlight } from "./fuzzy.ts";

test("name matches outrank directory matches; non-matches score 0", () => {
	expect(fuzzy("xyz", "main.ts", "src")).toBe(0);
	expect(fuzzy("main", "main.ts", "src")).toBeGreaterThan(fuzzy("main", "index.ts", "main"));
	expect(fuzzy("ws", "workspace.svelte.ts")).toBeGreaterThan(0);
});

test("consecutive and word-start hits beat scattered ones", () => {
	expect(fuzzy("side", "Sidebar.svelte")).toBeGreaterThan(fuzzy("side", "StatusRingDiode.svelte"));
});

test("highlight flags the matched characters, merging adjacent ones", () => {
	expect(highlight("sbr", "Sidebar")).toEqual([
		{ text: "S", hit: true },
		{ text: "ide", hit: false },
		{ text: "b", hit: true },
		{ text: "a", hit: false },
		{ text: "r", hit: true },
	]);
	expect(highlight("side", "Sidebar")[0]).toEqual({ text: "Side", hit: true });
});

test("highlight returns the text untouched when there is no match or no query", () => {
	expect(highlight("xyz", "Sidebar")).toEqual([{ text: "Sidebar", hit: false }]);
	expect(highlight("", "Sidebar")).toEqual([{ text: "Sidebar", hit: false }]);
});
