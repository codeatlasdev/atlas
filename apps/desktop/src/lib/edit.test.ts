// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { history } from "@codemirror/commands";
import { EditorState } from "@codemirror/state";
import { EDIT_COMMANDS, runEdit, targetOf } from "./edit.ts";

/** A headless view: CodeMirror's state and commands do not need a DOM, only `dispatch`. */
function headless(doc: string) {
	const view = {
		state: EditorState.create({ doc, extensions: [history()] }),
		// Commands destructure `{ state, dispatch }`: no `this`.
		dispatch: (...specs) => {
			view.state = view.state.update(...specs).state;
		},
	};
	return view as unknown as EditorView;
}

const type = (v, text) =>
	v.dispatch({ changes: { from: v.state.doc.length, insert: text }, userEvent: "input.type" });

const none = { exec: () => {}, terminalSelectAll: () => {} };

test("⌘Z / ⇧⌘Z undo and redo CodeMirror's own history", () => {
	const v = headless("a");
	type(v, "b");
	expect(v.state.doc.toString()).toBe("ab");
	expect(runEdit("undo", { kind: "codemirror", view: v }, none)).toBe(true);
	expect(v.state.doc.toString()).toBe("a");
	expect(runEdit("redo", { kind: "codemirror", view: v }, none)).toBe(true);
	expect(v.state.doc.toString()).toBe("ab");
});

test("undo with nothing to undo reports unhandled and leaves the document alone", () => {
	const v = headless("a");
	expect(runEdit("undo", { kind: "codemirror", view: v }, none)).toBe(false);
	expect(v.state.doc.toString()).toBe("a");
});

test("select all in CodeMirror selects the whole document", () => {
	const v = headless("one\ntwo");
	expect(runEdit("selectAll", { kind: "codemirror", view: v }, none)).toBe(true);
	expect(v.state.selection.main.from).toBe(0);
	expect(v.state.selection.main.to).toBe(7);
});

test("native fields get execCommand with the matching name", () => {
	const seen = [];
	const run = { exec: (n) => seen.push(n), terminalSelectAll: () => {} };
	for (const a of ["undo", "redo", "selectAll"]) expect(runEdit(a, { kind: "field" }, run)).toBe(true);
	expect(seen).toEqual(["undo", "redo", "selectAll"]);
});

test("a terminal selects its own text on ⌘A and ignores undo/redo (nothing reaches the shell)", () => {
	const tabs = [];
	const run = { exec: () => expect.unreachable(), terminalSelectAll: (t) => tabs.push(t) };
	const t = { kind: "terminal", tab: "t1" };
	expect(runEdit("undo", t, run)).toBe(false);
	expect(runEdit("redo", t, run)).toBe(false);
	expect(runEdit("selectAll", t, run)).toBe(true);
	expect(tabs).toEqual(["t1"]);
});

test("no focused target does nothing", () => {
	const run = { exec: () => expect.unreachable(), terminalSelectAll: () => expect.unreachable() };
	for (const a of ["undo", "redo", "selectAll"]) expect(runEdit(a, { kind: "none" }, run)).toBe(false);
});

test("the focused element picks the target: editor, then terminal sink, then field", () => {
	const view = headless("");
	const el = (name) => ({ name });
	const env = {
		viewOf: (e) => (e.name === "cm" ? view : undefined),
		terminalOf: (e) => (e.name === "sink" ? "t9" : undefined),
		isField: (e) => e.name === "input",
	};
	expect(targetOf(el("cm"), env)).toEqual({ kind: "codemirror", view });
	expect(targetOf(el("sink"), env)).toEqual({ kind: "terminal", tab: "t9" });
	expect(targetOf(el("input"), env)).toEqual({ kind: "field" });
	expect(targetOf(el("div"), env)).toEqual({ kind: "none" });
	expect(targetOf(null, env)).toEqual({ kind: "none" });
});

test("menu command ids map to the three actions", () => {
	expect(EDIT_COMMANDS).toEqual({
		"edit::Undo": "undo",
		"edit::Redo": "redo",
		"edit::SelectAll": "selectAll",
	});
});
