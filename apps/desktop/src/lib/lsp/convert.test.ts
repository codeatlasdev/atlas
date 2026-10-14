// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { ChangeSet, EditorState, Text } from "@codemirror/state";
import {
	ChangeBuffer,
	type ContentChange,
	applyTextEdits,
	offsetToPos,
	pathToUri,
	planWorkspaceEdit,
	posToOffset,
	toContentChanges,
	uriToPath,
} from "./convert.ts";

const doc = (s: string) => Text.of(s.split("\n"));

test("positions count UTF-16 units: emoji take two, CJK one", () => {
	const d = doc("a😀b\n日本語x");
	// "😀" is a surrogate pair: b sits at UTF-16 character 3
	expect(offsetToPos(d, 3)).toEqual({ line: 0, character: 3 });
	expect(posToOffset(d, { line: 0, character: 3 })).toBe(3);
	// CJK is BMP: x is character 3 on line 1
	expect(offsetToPos(d, d.line(2).from + 3)).toEqual({ line: 1, character: 3 });
	expect(posToOffset(d, { line: 1, character: 3 })).toBe(d.line(2).from + 3);
});

test("out-of-range positions clamp instead of throwing", () => {
	const d = doc("ab\ncd");
	expect(posToOffset(d, { line: 0, character: 99 })).toBe(2);
	expect(posToOffset(d, { line: 9, character: 0 })).toBe(d.length);
	expect(posToOffset(d, { line: -1, character: 0 })).toBe(0);
});

/** Replay LSP events the way a server does: sequentially against its own copy. */
function replay(base: string, events: ContentChange[]) {
	let d = doc(base);
	for (const e of events) {
		const from = posToOffset(d, e.range.start);
		const to = posToOffset(d, e.range.end);
		d = ChangeSet.of({ from, to, insert: e.text }, d.length).apply(d);
	}
	return d.toString();
}

test("a multi-change ChangeSet becomes sequential events a server can replay", () => {
	const base = "héllo 😀 world\nsecond line";
	const state = EditorState.create({ doc: base });
	const tr = state.update({
		changes: [
			{ from: 0, to: 1, insert: "HH" },
			{ from: 9, to: 14, insert: "there" },
			{ from: 15, insert: "NEW\n" },
		],
	});
	const events = toContentChanges(state.doc, tr.changes);
	expect(replay(base, events)).toBe(tr.newDoc.toString());
});

test("ChangeBuffer coalesces keystrokes into the minimal events against the last-sent text", () => {
	const base = "fn main() {}";
	let state = EditorState.create({ doc: base });
	const buf = new ChangeBuffer();
	for (const ch of ["a", "b", "c"]) {
		const tr = state.update({ changes: { from: 3 + state.doc.length - base.length, insert: ch } });
		buf.push(state.doc, tr.changes);
		state = tr.state;
	}
	expect(buf.pending).toBe(true);
	const events = buf.take();
	expect(events).toHaveLength(1);
	expect(events[0].text).toBe("abc");
	expect(replay(base, events)).toBe(state.doc.toString());
	expect(buf.pending).toBe(false);
	expect(buf.take()).toEqual([]);
});

test("ChangeBuffer: typing then deleting the same text cancels out", () => {
	const base = "x";
	let state = EditorState.create({ doc: base });
	const buf = new ChangeBuffer();
	let tr = state.update({ changes: { from: 1, insert: "yz" } });
	buf.push(state.doc, tr.changes);
	state = tr.state;
	tr = state.update({ changes: { from: 1, to: 3 } });
	buf.push(state.doc, tr.changes);
	expect(replay(base, buf.take())).toBe(base);
});

test("applyTextEdits applies edits that all refer to the original text (bottom-up safe)", () => {
	const text = "let a = 1;\nlet b = 2;\n";
	const out = applyTextEdits(text, [
		{ range: { start: { line: 0, character: 4 }, end: { line: 0, character: 5 } }, newText: "alpha" },
		{ range: { start: { line: 1, character: 4 }, end: { line: 1, character: 5 } }, newText: "beta" },
	]);
	expect(out).toBe("let alpha = 1;\nlet beta = 2;\n");
});

test("applyTextEdits handles emoji before the edit on the same line", () => {
	const out = applyTextEdits("😀 foo", [
		{ range: { start: { line: 0, character: 3 }, end: { line: 0, character: 6 } }, newText: "bar" },
	]);
	expect(out).toBe("😀 bar");
});

test("WorkspaceEdit: changes + documentChanges merge per file, file ops are kept", () => {
	const plan = planWorkspaceEdit({
		changes: { "file:///p/a.ts": [{ range: r(0, 0, 0, 1), newText: "x" }] },
		documentChanges: [
			{ textDocument: { uri: "file:///p/a.ts" }, edits: [{ range: r(1, 0, 1, 1), newText: "y" }] },
			{ textDocument: { uri: "file:///p/b%20c.ts" }, edits: [{ range: r(0, 0, 0, 0), newText: "z" }] },
			{ kind: "rename", oldUri: "file:///p/o.ts", newUri: "file:///p/n.ts" },
			{ kind: "delete", uri: "file:///p/gone.ts" },
		],
	});
	expect(plan.files.get("/p/a.ts")).toHaveLength(2);
	expect(plan.files.has("/p/b c.ts")).toBe(true);
	expect(plan.ops).toEqual([
		{ kind: "rename", from: "/p/o.ts", to: "/p/n.ts" },
		{ kind: "delete", path: "/p/gone.ts" },
	]);
});

test("uri <-> path round-trips spaces and unicode", () => {
	const p = "/Users/me/My Proj/ação/a#b.ts";
	expect(uriToPath(pathToUri(p))).toBe(p);
	expect(pathToUri("/a b/c.ts")).toBe("file:///a%20b/c.ts");
});

// Windows: the app keeps drive paths with forward slashes (C:/Users/…); servers send file:///C:/… (or c%3A).
test("windows drive paths become three-slash file URIs and come back without the slash", () => {
	const p = "C:/Users/me/My Proj/src/main.rs";
	expect(pathToUri(p)).toBe("file:///C:/Users/me/My%20Proj/src/main.rs");
	expect(uriToPath(pathToUri(p))).toBe(p);
	expect(uriToPath("file:///c%3A/x/y.ts")).toBe("c:/x/y.ts");
	// A Unix path that merely contains a colon is untouched.
	expect(uriToPath(pathToUri("/a/C:/b"))).toBe("/a/C:/b");
});

function r(sl, sc, el, ec) {
	return { start: { line: sl, character: sc }, end: { line: el, character: ec } };
}
