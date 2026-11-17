// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { EditorSelection, EditorState } from "@codemirror/state";
import { indentUnit } from "@codemirror/language";
import { javascript } from "@codemirror/lang-javascript";
import { parsedState } from "../../../test/parsedState.ts";
import {
	convertIndentation,
	expandSelection,
	indentOrInsert,
	joinLines,
	lowercase,
	minimalChange,
	shrinkSelection,
	sortLines,
	trimWhitespace,
	uppercase,
} from "./commands.ts";

/** Run a command on `doc` with the selection and return the resulting state. */
function exec(cmd, doc: string, sel = EditorSelection.cursor(0), extra = []) {
	let state = EditorState.create({
		doc,
		selection: sel,
		extensions: [EditorState.allowMultipleSelections.of(true), ...extra],
	});
	const ran = cmd({ state, dispatch: (tr) => (state = tr.state) });
	return { ran, state, text: state.doc.toString() };
}

test("joinLines joins the caret line with the next, trimming the next indent", () => {
	expect(exec(joinLines, "a\n   b\nc").text).toBe("a b\nc");
	expect(exec(joinLines, "a\nb\nc", EditorSelection.range(0, 5)).text).toBe("a b c");
	expect(exec(joinLines, "only").ran).toBe(false);
});

test("sortLines sorts the selection or the whole document, naturally", () => {
	expect(exec(sortLines(false), "b\na\nc10\nc2").text).toBe("a\nb\nc2\nc10");
	expect(exec(sortLines(true), "b\na\nc").text).toBe("c\nb\na");
	expect(exec(sortLines(false), "z\nb\na\ny", EditorSelection.range(2, 5)).text).toBe("z\na\nb\ny");
	expect(exec(sortLines(false), "a\nb").ran).toBe(false);
});

test("case commands act on the selection or the word at the caret", () => {
	expect(exec(uppercase, "foo bar", EditorSelection.range(0, 3)).text).toBe("FOO bar");
	expect(exec(uppercase, "foo bar", EditorSelection.cursor(5)).text).toBe("foo BAR");
	expect(exec(lowercase, "FOO").text).toBe("foo");
	expect(exec(lowercase, "foo").ran).toBe(false);
});

test("convertIndentation switches leading whitespace and leaves the rest", () => {
	const four = [indentUnit.of("    "), EditorState.tabSize.of(4)];
	expect(exec(convertIndentation("space"), "\tx\n\t\ty\n a\tb", EditorSelection.cursor(0), four).text).toBe(
		"    x\n        y\n a\tb",
	);
	expect(exec(convertIndentation("tab"), "    x\n      y", EditorSelection.cursor(0), four).text).toBe(
		"\tx\n\t  y",
	);
	expect(exec(convertIndentation("tab"), "x", EditorSelection.cursor(0), four).ran).toBe(false);
});

test("trimWhitespace removes trailing whitespace and changes nothing else", () => {
	expect(exec(trimWhitespace, "a  \n\tb\t\n  \nc").text).toBe("a\n\tb\n\nc");
	expect(exec(trimWhitespace, "a\nb").ran).toBe(false);
});

test("minimalChange is the smallest single replacement", () => {
	expect(minimalChange("abcXdef", "abcdef")).toEqual({ from: 3, to: 4, insert: "" });
	expect(minimalChange("same", "same")).toEqual({ from: 4, to: 4, insert: "" });
});

test("Tab inserts spaces up to the next unit stop, or indents a multi-line selection", () => {
	const two = [indentUnit.of("  ")];
	expect(exec(indentOrInsert, "ab", EditorSelection.cursor(1), two).text).toBe("a b");
	expect(exec(indentOrInsert, "a\nb", EditorSelection.range(0, 3), two).text).toBe("  a\n  b");
	expect(exec(indentOrInsert, "x", EditorSelection.cursor(0), [indentUnit.of("\t")]).text).toBe("\tx");
});

test("expand then shrink selection restores the previous selection", () => {
	const doc = "f(a, b)";
	// A fully parsed state: `selectParentSyntax` reads the state's tree, which a loaded runner may not finish at creation.
	let state = parsedState({ doc, selection: EditorSelection.cursor(3), extensions: [javascript()] });
	const dispatch = (tr) => (state = tr.state);
	expect(expandSelection({ state, dispatch })).toBe(true);
	const widened = state.selection.main;
	expect(widened.to - widened.from).toBeGreaterThan(0);
	expect(shrinkSelection({ state, dispatch })).toBe(true);
	expect(state.selection.main.from).toBe(3);
	expect(state.selection.main.empty).toBe(true);
	expect(shrinkSelection({ state, dispatch })).toBe(false);
});
