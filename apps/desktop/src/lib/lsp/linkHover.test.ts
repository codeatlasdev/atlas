// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { lspEnv } from "./context.ts";
import { linkHover } from "./linkHover.ts";

// Incident: the ⌘-hover underline (`cm-lsp-link`) stayed on the identifier it was drawn on after the caret moved and the
// view scrolled, so the editor showed the caret, the gutter number and the active-line band on line 19 and an underlined
// `fp` on another line (reproduced in the real app: underline on `fp2`, line 3, with the caret on 19). The decoration was
// a snapshot of the pointer position taken on `mousemove`; scrolling or a caret move changes what is under a resting
// pointer without any mouse event, so nothing recomputed it.
// Contract: the underline always describes what is under the pointer NOW, and only while the go-to modifier is held.
const env = {
	doc: { open: false, caps: {} },
	openAt() {},
	showLocations() {},
	inlayHints: () => false,
	formatOnSave: () => false,
};
const text = Array.from({ length: 30 }, (_, i) => `const fp${i} = compute(${i});`).join("\n");
const views: EditorView[] = [];
function editor() {
	const parent = document.createElement("div");
	document.body.append(parent);
	const view = new EditorView({
		parent,
		state: EditorState.create({ doc: text, extensions: [lspEnv.of(env), linkHover] }),
	});
	views.push(view);
	// happy-dom has no layout: the line under the pointer is whatever the test says it is.
	let under = view.state.doc.line(3).from + 8;
	view.posAtCoords = () => under;
	return { view, point: (pos: number) => (under = pos) };
}
const links = (view: EditorView) => [...view.dom.querySelectorAll(".cm-lsp-link")].map((e) => e.textContent);
const move = (view: EditorView, meta = true) =>
	view.contentDOM.dispatchEvent(
		new MouseEvent("mousemove", { bubbles: true, clientX: 5, clientY: 5, metaKey: meta }),
	);

afterEach(() => {
	for (const v of views.splice(0)) v.destroy();
	document.body.innerHTML = "";
});

test("⌘-hover underlines the identifier under the pointer", () => {
	const { view } = editor();
	move(view);
	expect(links(view)).toEqual(["fp2"]);
});

test("without ⌘ nothing is underlined", () => {
	const { view } = editor();
	move(view, false);
	expect(links(view)).toEqual([]);
});

test("the underline follows what is under a resting pointer when the content moves (scroll, caret-driven scroll)", () => {
	const { view, point } = editor();
	move(view);
	expect(links(view)).toEqual(["fp2"]);
	// the view scrolls: another identifier is now under the same pointer. No mouse event.
	point(view.state.doc.line(19).from + 8);
	view.dispatch({ selection: { anchor: view.state.doc.line(19).from + 4 }, scrollIntoView: true });
	view.scrollDOM.dispatchEvent(new Event("scroll"));
	expect(links(view)).toEqual(["fp18"]);
});

test("a caret move drops an underline that no longer has a pointer over it (pointer left the content)", () => {
	const { view, point } = editor();
	move(view);
	view.contentDOM.dispatchEvent(new MouseEvent("mouseleave"));
	expect(links(view)).toEqual([]);
	point(view.state.doc.line(19).from + 8);
	view.dispatch({ selection: { anchor: view.state.doc.line(19).from + 4 } });
	expect(links(view)).toEqual([]);
});

test("releasing ⌘ clears it, and a later scroll does not bring it back", () => {
	const { view, point } = editor();
	move(view);
	window.dispatchEvent(new KeyboardEvent("keyup", { key: "Meta" }));
	view.contentDOM.dispatchEvent(new KeyboardEvent("keyup", { key: "Meta", bubbles: true }));
	expect(links(view)).toEqual([]);
	point(view.state.doc.line(19).from + 8);
	view.scrollDOM.dispatchEvent(new Event("scroll"));
	expect(links(view)).toEqual([]);
});

test("an edit clears the underline (its range is stale)", () => {
	const { view } = editor();
	move(view);
	view.dispatch({ changes: { from: 0, insert: "// hi\n" } });
	expect(links(view)).toEqual([]);
});
