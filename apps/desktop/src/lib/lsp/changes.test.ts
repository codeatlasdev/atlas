// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { ChangeSet, EditorState, Text } from "@codemirror/state";
import { ChangeBuffer, applyTextEdits, toContentChanges } from "./changes.ts";
import { posToOffset, type ContentChange } from "./convert.ts";

const doc = (s: string) => Text.of(s.split("\n"));

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
