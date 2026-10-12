// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { javascript } from "@codemirror/lang-javascript";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { EditorState } from "@codemirror/state";
import {
	outline,
	registerEditor,
	saveAllEditors,
	saveEditor,
	savers,
	unregisterEditor,
	editors,
} from "./symbols.ts";

const symbols = (doc: string, extension: unknown) =>
	outline(EditorState.create({ doc, extensions: [extension] })).map((s) => [s.kind, s.name, s.line]);

test("TypeScript: functions, classes, interfaces and type aliases; imports are skipped", () => {
	const doc = [
		'import { x } from "y";',
		"export function run(a: number) {}",
		"class Box {}",
		"interface Shape { w: number }",
		"type Id = string;",
	].join("\n");
	expect(symbols(doc, javascript({ typescript: true }))).toEqual([
		["Function", "run", 2],
		["Class", "Box", 3],
		["Interface", "Shape", 4],
		["TypeAlias", "Id", 5],
	]);
});

test("Rust: fn, struct, enum and trait items; use is skipped", () => {
	const doc = [
		"use std::fmt;",
		"fn main() {}",
		"struct Point { x: i32 }",
		"enum Kind { A }",
		"trait Draw {}",
	].join("\n");
	expect(symbols(doc, rust())).toEqual([
		["Function", "main", 2],
		["Struct", "Point", 3],
		["Enum", "Kind", 4],
		["Trait", "Draw", 5],
	]);
});

test("Python: functions, async functions and classes", () => {
	const doc = ["import os", "def run():", "    pass", "class Box:", "    def go(self): ..."].join("\n");
	const got = symbols(doc, python());
	expect(got.map((s) => s[1])).toEqual(["run", "Box", "go"]);
	expect(got[0]).toEqual(["Function", "run", 2]);
});

test("Markdown: ATX headings by level", () => {
	const got = symbols("# Title\ntext\n## Part\n", markdown());
	expect(got).toEqual([
		["H1", "Title", 1],
		["H2", "Part", 3],
	]);
});

test("⌘S saves the focused editor, ⌥⌘S every one, and a missing editor reports false", async () => {
	const saved: string[] = [];
	const a = {} as never;
	const b = {} as never;
	registerEditor("w:a.ts", a, async () => void saved.push("a"));
	registerEditor("w:b.ts", b, async () => void saved.push("b"));
	expect(await saveEditor("w:a.ts")).toBe(true);
	expect(saved).toEqual(["a"]);
	expect(await saveEditor("w:none.ts")).toBe(false);
	expect(await saveEditor(undefined)).toBe(false);
	expect(await saveAllEditors()).toBe(2);
	expect(saved.sort()).toEqual(["a", "a", "b"]);
	// Closing an editor drops its saver, but a newer view under the same key is left alone.
	unregisterEditor("w:a.ts", {} as never);
	expect(savers.has("w:a.ts")).toBe(true);
	unregisterEditor("w:a.ts", a);
	unregisterEditor("w:b.ts", b);
	expect(savers.size).toBe(0);
	expect(editors.size).toBe(0);
	expect(await saveAllEditors()).toBe(0);
	// The atlas.yaml tab registers a saver without an editor entry: ⌘S reaches it, ⌥⌘S skips it.
	let reviewed = 0;
	savers.set("config:p1", async () => void reviewed++);
	expect(await saveEditor("config:p1")).toBe(true);
	expect(await saveAllEditors()).toBe(0);
	expect(reviewed).toBe(1);
	savers.delete("config:p1");
});
