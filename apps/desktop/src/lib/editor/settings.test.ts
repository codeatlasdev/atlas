// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, mock, test } from "bun:test";
import { prefsMock } from "../../../test/prefsMock.ts";

// The adapter imports the rune store (`$state`), which bun cannot compile. The mock must be
// registered before the adapter loads, so a static import cannot work here.
mock.module("#lib/stores/prefs.svelte.ts", () => prefsMock({ editor: { tab_size: 2 } }));
const { EDITOR_PREF_DEFAULTS, resolveEditorPrefs, editorPrefs } = await import("./settings.ts");

test("non-objects resolve to the defaults", () => {
	for (const raw of [undefined, null, 5, "x", [], true]) {
		expect(resolveEditorPrefs(raw)).toEqual(EDITOR_PREF_DEFAULTS);
	}
});

test("defaults match the documented contract", () => {
	expect(EDITOR_PREF_DEFAULTS).toMatchObject({
		tab_size: 4,
		insert_spaces: true,
		word_wrap: false,
		render_whitespace: "none",
		large_file_threshold_bytes: 2_000_000,
		rulers: [],
		auto_reload: true,
	});
});

test("valid values pass through", () => {
	const raw = {
		tab_size: 2,
		insert_spaces: false,
		word_wrap: true,
		render_whitespace: "all",
		minimap: true,
		rulers: [100, 80],
		large_file_threshold_bytes: 5_000_000,
		auto_reload: false,
	};
	expect(resolveEditorPrefs(raw)).toMatchObject({ ...raw, rulers: [80, 100] });
});

test("tab_size is range-checked and floored", () => {
	expect(resolveEditorPrefs({ tab_size: 0 }).tab_size).toBe(4);
	expect(resolveEditorPrefs({ tab_size: 17 }).tab_size).toBe(4);
	expect(resolveEditorPrefs({ tab_size: 16 }).tab_size).toBe(16);
	expect(resolveEditorPrefs({ tab_size: 1 }).tab_size).toBe(1);
	expect(resolveEditorPrefs({ tab_size: 2.9 }).tab_size).toBe(2);
	expect(resolveEditorPrefs({ tab_size: Number.NaN }).tab_size).toBe(4);
	expect(resolveEditorPrefs({ tab_size: "2" }).tab_size).toBe(4);
});

test("threshold is clamped to its allowed window", () => {
	expect(resolveEditorPrefs({ large_file_threshold_bytes: 10 }).large_file_threshold_bytes).toBe(2_000_000);
	expect(resolveEditorPrefs({ large_file_threshold_bytes: 1e12 }).large_file_threshold_bytes).toBe(2_000_000);
	expect(resolveEditorPrefs({ large_file_threshold_bytes: 100_000 }).large_file_threshold_bytes).toBe(
		100_000,
	);
});

test("wrong-typed booleans and bad enums fall back per field", () => {
	const p = resolveEditorPrefs({
		word_wrap: "yes",
		minimap: 1,
		render_whitespace: "boundary",
		line_numbers: false,
	});
	expect(p.word_wrap).toBe(false);
	expect(p.minimap).toBe(false);
	expect(p.render_whitespace).toBe("none");
	expect(p.line_numbers).toBe(false);
});

test("rulers keep whole columns 1..500, sorted and unique", () => {
	expect(resolveEditorPrefs({ rulers: [120, 80, 80, 0, -3, 501, 1.5, "9", 500, 1] }).rulers).toEqual([
		1, 80, 120, 500,
	]);
	expect(resolveEditorPrefs({ rulers: "80" }).rulers).toEqual([]);
});

test("editorPrefs reads the live store through the validator", () => {
	expect(editorPrefs()).toEqual({ ...EDITOR_PREF_DEFAULTS, tab_size: 2 });
});
