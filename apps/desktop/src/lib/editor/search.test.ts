// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { SearchQuery } from "@codemirror/search";
import { EditorState } from "@codemirror/state";
import { MATCH_CAP, counterLabel, countMatches, replacementPreview } from "./search.ts";
import { m } from "#lib/i18n/m.ts";

const st = (doc: string) => EditorState.create({ doc });

test("countMatches counts and locates the current match", () => {
	const s = st("a b a b a");
	const q = new SearchQuery({ search: "a" });
	expect(countMatches(q, s)).toEqual({ total: 3, capped: false, index: 0 });
	expect(countMatches(q, s, { from: 4, to: 5 })).toEqual({ total: 3, capped: false, index: 2 });
});

test("case, whole word and regex flags change the count", () => {
	const s = st("Foo foo foobar");
	expect(countMatches(new SearchQuery({ search: "foo" }), s).total).toBe(3);
	expect(countMatches(new SearchQuery({ search: "foo", caseSensitive: true }), s).total).toBe(2);
	expect(countMatches(new SearchQuery({ search: "foo", wholeWord: true }), s).total).toBe(2);
	expect(countMatches(new SearchQuery({ search: "fo+", regexp: true }), s).total).toBe(3);
});

test("an invalid regex or empty query counts nothing", () => {
	expect(countMatches(new SearchQuery({ search: "(", regexp: true }), st("((")).total).toBe(0);
	expect(countMatches(new SearchQuery({ search: "" }), st("a")).total).toBe(0);
});

test("the scan stops at the cap", () => {
	const s = st("x ".repeat(MATCH_CAP + 500));
	const c = countMatches(new SearchQuery({ search: "x" }), s);
	expect(c).toMatchObject({ total: MATCH_CAP, capped: true });
	expect(counterLabel(c, true, true)).toBe(m.workbench_editor_find_found_capped({ count: MATCH_CAP }));
});

test("in-selection scope limits the scan", () => {
	const s = st("a a a a");
	expect(countMatches(new SearchQuery({ search: "a" }), s, undefined, { from: 2, to: 5 }).total).toBe(2);
});

test("counterLabel", () => {
	expect(counterLabel({ total: 3, capped: false, index: 2 }, true, true)).toBe(
		m.workbench_editor_find_position({ index: "2", total: "3" }),
	);
	expect(counterLabel({ total: 3, capped: false, index: 0 }, true, true)).toBe(
		m.workbench_editor_find_found({ count: 3 }),
	);
	expect(counterLabel({ total: 0, capped: false, index: 0 }, true, true)).toBe(
		m.workbench_editor_find_no_results(),
	);
	expect(counterLabel({ total: 0, capped: false, index: 0 }, false, true)).toBe(m.logs_search_bar_invalid());
	expect(counterLabel({ total: 0, capped: false, index: 0 }, true, false)).toBe("");
});

test("replacementPreview resolves regex groups of the selected match", () => {
	const s = st("john smith");
	const q = new SearchQuery({ search: "(\\w+) (\\w+)", regexp: true, replace: "$2, $1 [$&] $$" });
	expect(replacementPreview(q, s, { from: 0, to: 10 })).toBe("smith, john [john smith] $");
});

test("replacementPreview is literal for plain queries and null when nothing matches", () => {
	const s = st("a.b a.b");
	const q = new SearchQuery({ search: "a.b", replace: "$1\\tX" });
	expect(replacementPreview(q, s, { from: 0, to: 3 })).toBe("$1\tX");
	expect(replacementPreview(q, s, { from: 1, to: 3 })).toBeNull();
	expect(replacementPreview(new SearchQuery({ search: "a" }), s, { from: 0, to: 1 })).toBeNull();
});
