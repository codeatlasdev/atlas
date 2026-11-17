// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { describe, expect, test } from "bun:test";
import { history, undo } from "@codemirror/commands";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import {
	cycleHeading,
	headingLevel,
	insertLink,
	setHeading,
	toggleBold,
	toggleBullets,
	toggleItalic,
	toggleNumbered,
} from "./markdownFormat.ts";

// The light editor's Markdown bar (#499) edits the real CodeMirror document, so these run the real commands on a real
// view: the text, the selection after the edit and undo are what the user gets.

/** `|` marks the caret; `[` and `]` the selection. One range only; see `multi` for more. */
function open(marked: string) {
	const from = marked.indexOf("[");
	const caret = marked.indexOf("|");
	let doc = marked;
	let sel;
	if (from >= 0) {
		const to = marked.indexOf("]");
		doc = marked.replace("[", "").replace("]", "");
		sel = EditorSelection.range(from, to - 1);
	} else {
		doc = marked.replace("|", "");
		sel = EditorSelection.cursor(caret < 0 ? 0 : caret);
	}
	return new EditorView({
		parent: document.createElement("div"),
		state: EditorState.create({
			doc,
			selection: sel,
			extensions: [history(), EditorState.allowMultipleSelections.of(true)],
		}),
	});
}
const text = (v) => v.state.doc.toString();
const picked = (v) => v.state.sliceDoc(v.state.selection.main.from, v.state.selection.main.to);

describe("bold and italic", () => {
	test("wrap the selection, keep it selected, and a second press takes the marks off", () => {
		const v = open("say [hello] now");
		toggleBold(v);
		expect(text(v)).toBe("say **hello** now");
		expect(picked(v)).toBe("hello");
		toggleBold(v);
		expect(text(v)).toBe("say hello now");
		expect(picked(v)).toBe("hello");
	});

	test("take the marks off whether the selection is inside them or includes them", () => {
		const whole = open("[**hi**]");
		toggleBold(whole);
		expect(text(whole)).toBe("hi");
		const underscores = open("a __[hi]__ b");
		toggleBold(underscores);
		expect(text(underscores)).toBe("a hi b");
	});

	test("an empty selection inserts a pair with the caret between, and a pair is removed from inside", () => {
		const v = open("a | b");
		toggleBold(v);
		expect(text(v)).toBe("a **** b");
		expect(v.state.selection.main.head).toBe(4);
		toggleBold(v);
		expect(text(v)).toBe("a  b");
	});

	test("italic is not fooled by bold, nor bold by italic", () => {
		const v = open("**[x]**");
		toggleItalic(v);
		expect(text(v)).toBe("**_x_**");
		const w = open("*[x]*");
		toggleBold(w);
		expect(text(w)).toBe("***x***");
	});

	test("a file's own *italic* is recognised and removed", () => {
		const v = open("an *[idea]* here");
		toggleItalic(v);
		expect(text(v)).toBe("an idea here");
	});

	test("every selection range is formatted in one transaction, undone in one step", () => {
		const v = open("a b");
		v.dispatch({
			selection: EditorSelection.create([EditorSelection.range(0, 1), EditorSelection.range(2, 3)]),
		});
		toggleBold(v);
		expect(text(v)).toBe("**a** **b**");
		undo(v);
		expect(text(v)).toBe("a b");
	});
});

describe("headings", () => {
	test("the button cycles plain, H1, H2, H3 and back to plain", () => {
		const v = open("Title|");
		const seen = [];
		for (let i = 0; i < 5; i++) {
			cycleHeading(v);
			seen.push(text(v));
		}
		expect(seen).toEqual(["# Title", "## Title", "### Title", "Title", "# Title"]);
	});

	test("an explicit level replaces the old one and level 0 removes it", () => {
		const v = open("#### Deep|");
		setHeading(v, 2);
		expect(text(v)).toBe("## Deep");
		setHeading(v, 0);
		expect(text(v)).toBe("Deep");
		expect(headingLevel("##### x")).toBe(5);
		expect(headingLevel("#nospace")).toBe(0);
	});

	test("a heading replaces a list marker instead of stacking on it", () => {
		const v = open("- item|");
		setHeading(v, 1);
		expect(text(v)).toBe("# item");
	});
});

describe("lists", () => {
	test("bullets go on every non-blank line of the selection, skipping blank ones, and come off again", () => {
		const v = open("[one\n\ntwo\nthree]");
		toggleBullets(v);
		expect(text(v)).toBe("- one\n\n- two\n- three");
		toggleBullets(v);
		expect(text(v)).toBe("one\n\ntwo\nthree");
	});

	test("numbers count the lines they were put on, not the blank ones between", () => {
		const v = open("[a\n\nb\nc]");
		toggleNumbered(v);
		expect(text(v)).toBe("1. a\n\n2. b\n3. c");
		toggleNumbered(v);
		expect(text(v)).toBe("a\n\nb\nc");
	});

	test("a bullet list becomes a numbered one, and a half-bulleted selection gets all bullets", () => {
		const v = open("[- a\n- b]");
		toggleNumbered(v);
		expect(text(v)).toBe("1. a\n2. b");
		const half = open("[- a\nb]");
		toggleBullets(half);
		expect(text(half)).toBe("- a\n- b");
	});

	test("indentation is kept and an empty line becomes the first item", () => {
		const nested = open("[  a\n  b]");
		toggleBullets(nested);
		expect(text(nested)).toBe("  - a\n  - b");
		const empty = open("|");
		toggleBullets(empty);
		expect(text(empty)).toBe("- ");
	});

	test("a range ending at the start of the next line leaves that line alone", () => {
		const v = open("[a\n]b");
		toggleBullets(v);
		expect(text(v)).toBe("- a\nb");
	});
});

describe("links", () => {
	test("selected text becomes the label with the address selected to type over", () => {
		const v = open("see [docs] now");
		insertLink(v);
		expect(text(v)).toBe("see [docs](https://) now");
		expect(picked(v)).toBe("https://");
	});

	test("a selected URL becomes the address with the caret in the empty label", () => {
		const v = open("[https://atlas.dev/x]");
		insertLink(v);
		expect(text(v)).toBe("[](https://atlas.dev/x)");
		expect(v.state.selection.main.head).toBe(1);
	});

	test("nothing selected inserts a placeholder label, selected", () => {
		const v = open("a |b");
		insertLink(v);
		expect(text(v)).toBe("a [link text](https://)b");
		expect(picked(v)).toBe("link text");
	});
});
