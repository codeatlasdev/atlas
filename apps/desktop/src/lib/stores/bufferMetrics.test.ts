// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";
import { flushSync } from "svelte";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

mock.module("@tauri-apps/api/core", () => ({ invoke: async () => ({}) }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

const { prefs } = await import("./prefs.svelte.ts");
const { editors } = await import("../symbols.ts");
const { DEFAULT_PREFS } = await import("../prefs.ts");

// Incident: change "Code font size" or "Line spacing" in Settings with an editor open and the gutter, the caret and
// the active-line band disagree (the gutter marks line 19 while the highlight sits on line 18). Both settings are
// applied as CSS variables on `:root` (prefs.svelte.ts), so the editor's line boxes change height with no CodeMirror
// transaction; CodeMirror caches the line height, the gutter column and the caret layer until its next measure
// cycle, and nothing asked for one. A real Chromium run reproduced it: after 12.5px/1.55 -> 15px/1.7 the active
// line moved to y=463 while the gutter element stayed at y=352.75; `requestMeasure()` made them agree again.
// Contract: whenever the buffer font family, size or line spacing changes, every live editor re-measures.
const views: EditorView[] = [];
function liveEditor(key: string) {
	const parent = document.createElement("div");
	document.body.append(parent);
	const view = new EditorView({ parent, state: EditorState.create({ doc: "a\nb\nc" }) });
	let measures = 0;
	const real = view.requestMeasure.bind(view);
	view.requestMeasure = (...a) => (measures++, real(...a));
	editors.set(key, view);
	views.push(view);
	return () => measures;
}
const set = (patch: Partial<typeof DEFAULT_PREFS>) => {
	prefs.value = { ...prefs.value, ...patch };
	flushSync();
};

afterEach(() => {
	for (const v of views.splice(0)) v.destroy();
	editors.clear();
	prefs.value = structuredClone(DEFAULT_PREFS);
	flushSync();
});

test("changing the code font size re-measures every open editor", () => {
	const a = liveEditor("w1:a.ts");
	const b = liveEditor("w1:b.ts");
	set({ buffer_font_size: 15 });
	expect(a()).toBeGreaterThan(0);
	expect(b()).toBeGreaterThan(0);
});

test("changing the line spacing re-measures", () => {
	const a = liveEditor("w1:a.ts");
	set({ buffer_line_height: 1.7 });
	expect(a()).toBeGreaterThan(0);
});

test("changing the code font family re-measures", () => {
	const a = liveEditor("w1:a.ts");
	set({ buffer_font_family: "Menlo, monospace" });
	expect(a()).toBeGreaterThan(0);
});

test("an unrelated preference does not make editors measure", () => {
	const a = liveEditor("w1:a.ts");
	set({ ui_font_size: 15 });
	expect(a()).toBe(0);
});

test("the CSS variables are written before the editors measure (they read the new line height)", () => {
	liveEditor("w1:a.ts");
	const view = editors.get("w1:a.ts");
	const measure = view.requestMeasure;
	let seen = "";
	view.requestMeasure = (...x) => {
		seen = document.documentElement.style.getPropertyValue("--atlas-buffer-line-height");
		return measure(...x);
	};
	set({ buffer_line_height: 1.9 });
	expect(seen).toBe("1.9");
});
