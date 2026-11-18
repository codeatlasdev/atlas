// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { EditorState } from "@codemirror/state";
import {
	NavHistory,
	hoverLink,
	isGoToClick,
	isGoToModifier,
	resolveTarget,
	toTargets,
} from "./navigation.ts";
import { selectionOf } from "./navigationCm.ts";

const createState = (doc) => EditorState.create({ doc });
const WORKTREES = [
	{ id: "w-atlas", path: "/Users/me/atlas" },
	{ id: "w-nested", path: "/Users/me/atlas/crates/atlas-core" },
	{ id: "w-trail", path: "/Users/me/other/" },
];

// --- modifier detection ----------------------------------------------------------------------

test("⌘ is the go-to modifier on macOS, Ctrl elsewhere; never with another modifier", () => {
	expect(isGoToModifier({ metaKey: true, ctrlKey: false, altKey: false, shiftKey: false }, true)).toBe(true);
	expect(isGoToModifier({ metaKey: false, ctrlKey: true, altKey: false, shiftKey: false }, true)).toBe(false);
	expect(isGoToModifier({ metaKey: false, ctrlKey: true, altKey: false, shiftKey: false }, false)).toBe(true);
	expect(isGoToModifier({ metaKey: true, ctrlKey: false, altKey: false, shiftKey: false }, false)).toBe(
		false,
	);
	// ⌥⌘-click adds a cursor, ⇧⌘-click extends the selection: neither is go to definition.
	expect(isGoToModifier({ metaKey: true, ctrlKey: false, altKey: true, shiftKey: false }, true)).toBe(false);
	expect(isGoToModifier({ metaKey: true, ctrlKey: false, altKey: false, shiftKey: true }, true)).toBe(false);
});

test("a go-to click is the primary button with the modifier held", () => {
	const e = { metaKey: true, ctrlKey: false, altKey: false, shiftKey: false, button: 0 };
	expect(isGoToClick(e, true)).toBe(true);
	expect(isGoToClick({ ...e, button: 2 }, true)).toBe(false);
	expect(isGoToClick({ ...e, button: 1 }, true)).toBe(false);
	expect(isGoToClick({ ...e, metaKey: false }, true)).toBe(false);
});

// --- location → target ----------------------------------------------------------------------

test("Location and LocationLink both keep the exact target range", () => {
	const loc = {
		uri: "file:///w/a.ts",
		range: { start: { line: 4, character: 2 }, end: { line: 4, character: 9 } },
	};
	const link = {
		targetUri: "file:///w/b.ts",
		targetRange: { start: { line: 1, character: 0 }, end: { line: 8, character: 1 } },
		targetSelectionRange: { start: { line: 1, character: 7 }, end: { line: 1, character: 10 } },
	};
	expect(toTargets(loc)).toEqual([
		{ path: "/w/a.ts", start: { line: 4, character: 2 }, end: { line: 4, character: 9 } },
	]);
	// A link selects the *name* (selection range), not the whole declaration body.
	expect(toTargets([link])).toEqual([
		{ path: "/w/b.ts", start: { line: 1, character: 7 }, end: { line: 1, character: 10 } },
	]);
});

test("null, empty and malformed replies are no targets, never a throw", () => {
	expect(toTargets(null)).toEqual([]);
	expect(toTargets([])).toEqual([]);
	expect(toTargets([{}, null, { uri: 5 }])).toEqual([]);
});

test("percent-encoded URIs decode to real paths (spaces, unicode, node_modules scopes)", () => {
	const loc = {
		uri: "file:///Users/me/My%20App/node_modules/@types/node/index.d.ts",
		range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
	};
	expect(toTargets(loc)[0].path).toBe("/Users/me/My App/node_modules/@types/node/index.d.ts");
});

test("a non-file URI (jar:, zipfile:, untitled:) is not a target we can open", () => {
	const loc = {
		uri: "zipfile:///x.zip::a.ts",
		range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
	};
	expect(toTargets(loc)).toEqual([]);
});

test("selection of a target clamps to the document and is exact when it fits", () => {
	const state = EditorState.create({ doc: "export function add(a) {}\nconst x = add(1);\n" });
	const sel = selectionOf(state.doc, { line: 0, character: 16 }, { line: 0, character: 19 });
	expect(state.sliceDoc(sel.anchor, sel.head)).toBe("add");
	// Past the end of the line / document: clamped, never throws, never negative.
	const far = selectionOf(state.doc, { line: 99, character: 99 }, { line: 120, character: 0 });
	expect(far.anchor).toBe(state.doc.length);
	expect(far.head).toBe(state.doc.length);
});

// --- worktree vs external --------------------------------------------------------------------

test("a file inside a worktree opens there; the deepest root wins", () => {
	expect(resolveTarget("/Users/me/atlas/apps/desktop/a.ts", WORKTREES)).toEqual({
		kind: "worktree",
		worktreeId: "w-atlas",
		rel: "apps/desktop/a.ts",
	});
	expect(resolveTarget("/Users/me/atlas/crates/atlas-core/src/lib.rs", WORKTREES)).toEqual({
		kind: "worktree",
		worktreeId: "w-nested",
		rel: "src/lib.rs",
	});
	expect(resolveTarget("/Users/me/other/x.go", WORKTREES)).toEqual({
		kind: "worktree",
		worktreeId: "w-trail",
		rel: "x.go",
	});
});

test("`..` segments are collapsed before matching (symlinked package stores)", () => {
	expect(resolveTarget("/Users/me/atlas/apps/../crates/x.rs", WORKTREES)).toEqual({
		kind: "worktree",
		worktreeId: "w-atlas",
		rel: "crates/x.rs",
	});
	// An escape out of the worktree through `..` is outside, not inside.
	expect(resolveTarget("/Users/me/atlas/../secret/x", WORKTREES).kind).toBe("external");
});

test("a root is not a prefix match: /Users/me/atlas-old is outside /Users/me/atlas", () => {
	expect(resolveTarget("/Users/me/atlas-old/a.ts", WORKTREES).kind).toBe("external");
});

test("node_modules, rustup and registry sources are external and read-only", () => {
	for (const p of [
		"/Users/me/elsewhere/node_modules/typescript/lib/lib.dom.d.ts",
		"/Users/me/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/option.rs",
		"/Users/me/.cargo/registry/src/index.crates.io-6f17d22bba15001f/serde-1.0.200/src/lib.rs",
	]) {
		expect(resolveTarget(p, WORKTREES)).toEqual({ kind: "external", abs: p });
	}
});

test("dependency sources inside a worktree (node_modules) still open in that worktree, read-only", () => {
	const t = resolveTarget("/Users/me/atlas/node_modules/typescript/lib/lib.d.ts", WORKTREES);
	expect(t).toEqual({
		kind: "worktree",
		worktreeId: "w-atlas",
		rel: "node_modules/typescript/lib/lib.d.ts",
		readOnly: true,
	});
});

// --- Cmd+hover link --------------------------------------------------------------------------

test("hover link: the identifier under the pointer, nothing on whitespace or punctuation", () => {
	const state = EditorState.create({ doc: "const x = add(1);" });
	expect(hoverLink(state, 11)).toEqual({ from: 10, to: 13 });
	expect(hoverLink(state, 10)).toEqual({ from: 10, to: 13 });
	expect(hoverLink(state, 13)).toEqual({ from: 10, to: 13 });
	expect(hoverLink(createState("a  b"), 2)).toBeNull(); // between two spaces: no word touches it
	expect(hoverLink(createState("f(  )"), 3)).toBeNull(); // inside empty parens, no word touches
	expect(hoverLink(state, null)).toBeNull();
});

// --- position history ------------------------------------------------------------------------

const P = (path, line, character = 0) => ({ path, line, character });

test("back returns to where the jump started, forward returns to the target", () => {
	const h = new NavHistory();
	h.push(P("/a.ts", 3, 4)); // jumping away from a.ts:3:4
	expect(h.back(P("/b.ts", 10, 0))).toEqual(P("/a.ts", 3, 4));
	expect(h.forward(P("/a.ts", 3, 4))).toEqual(P("/b.ts", 10, 0));
});

test("nothing to go back to is null, not an exception; same for forward", () => {
	const h = new NavHistory();
	expect(h.back(P("/a.ts", 0))).toBeNull();
	expect(h.forward(P("/a.ts", 0))).toBeNull();
	expect(h.canBack()).toBe(false);
	expect(h.canForward()).toBe(false);
});

test("a new jump after going back drops the forward entries", () => {
	const h = new NavHistory();
	h.push(P("/a.ts", 1));
	h.back(P("/b.ts", 2));
	expect(h.canForward()).toBe(true);
	h.push(P("/a.ts", 1));
	expect(h.canForward()).toBe(false);
});

test("jumping to where you already are is not a history entry", () => {
	const h = new NavHistory();
	h.push(P("/a.ts", 5, 1));
	h.push(P("/a.ts", 5, 1));
	expect(h.depth).toBe(1);
});

test("history is bounded and keeps the most recent entries", () => {
	const h = new NavHistory(3);
	for (let i = 0; i < 10; i++) h.push(P("/a.ts", i));
	expect(h.depth).toBe(3);
	expect(h.back(P("/z.ts", 0))).toEqual(P("/a.ts", 9));
	expect(h.back(P("/a.ts", 9))).toEqual(P("/a.ts", 8));
});

test("back then back then forward walks the chain in order", () => {
	const h = new NavHistory();
	h.push(P("/a.ts", 1));
	h.push(P("/b.ts", 2));
	// now at c.ts:3
	expect(h.back(P("/c.ts", 3))).toEqual(P("/b.ts", 2));
	expect(h.back(P("/b.ts", 2))).toEqual(P("/a.ts", 1));
	expect(h.forward(P("/a.ts", 1))).toEqual(P("/b.ts", 2));
	expect(h.forward(P("/b.ts", 2))).toEqual(P("/c.ts", 3));
	expect(h.forward(P("/c.ts", 3))).toBeNull();
});

// --- how an editor tab addresses its file --------------------------------------------------------

import { editorKey, fileOfTab, tabFor } from "./navigation.ts";

test("a worktree file is `rel` under the root; an external file is its absolute path, read-only", () => {
	expect(fileOfTab("/Users/me/atlas", "src/a.ts")).toEqual({
		abs: "/Users/me/atlas/src/a.ts",
		external: false,
	});
	expect(fileOfTab("/Users/me/atlas", "/Users/me/.cargo/registry/src/x/lib.rs")).toEqual({
		abs: "/Users/me/.cargo/registry/src/x/lib.rs",
		external: true,
	});
	// A worktree-relative path is never absolute, so there is no ambiguity.
	expect(fileOfTab("", "/etc/hosts").external).toBe(true);
});

test("the registry key of an external editor is stable and distinct from any worktree file", () => {
	expect(editorKey("w1", "src/a.ts")).toBe("w1:src/a.ts");
	expect(editorKey("w1", "/abs/b.rs")).toBe("w1:/abs/b.rs");
});

test("tabFor maps a resolved target to the tab to reveal (path, label) and its read-only flag", () => {
	expect(tabFor({ kind: "worktree", worktreeId: "w1", rel: "src/a.ts" })).toEqual({
		worktreeId: "w1",
		path: "src/a.ts",
		readOnly: false,
	});
	expect(
		tabFor({ kind: "worktree", worktreeId: "w1", rel: "node_modules/x/a.d.ts", readOnly: true }),
	).toEqual({
		worktreeId: "w1",
		path: "node_modules/x/a.d.ts",
		readOnly: true,
	});
	expect(tabFor({ kind: "external", abs: "/x/y.rs" }, "w9")).toEqual({
		worktreeId: "w9",
		path: "/x/y.rs",
		readOnly: true,
	});
});

test("an external target with no worktree to host the tab cannot open (caller says so)", () => {
	expect(tabFor({ kind: "external", abs: "/x/y.rs" })).toBeNull();
});

// --- read-only sources ---------------------------------------------------------------------------

import { readOnlyExtension } from "./navigationCm.ts";

test("a read-only source rejects edits but still moves the caret and selects", () => {
	const state = EditorState.create({ doc: "fn main() {}", extensions: [readOnlyExtension()] });
	expect(state.readOnly).toBe(true);
	// CodeMirror's own commands (`insertChar`, `deleteLine`) honour `state.readOnly`; a raw dispatch does not,
	// so the extension must also filter transactions: nothing may change the document.
	const edited = state.update({ changes: { from: 0, insert: "x" } }).state;
	expect(edited.doc.toString()).toBe("fn main() {}");
	const moved = state.update({ selection: { anchor: 3, head: 7 } }).state;
	expect(moved.selection.main.from).toBe(3);
	expect(moved.selection.main.to).toBe(7);
});
