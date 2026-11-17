// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { parsedState } from "../../../test/parsedState.ts";
import { javascript } from "@codemirror/lang-javascript";
import { isScopeNode, scopesAt, stickyChain } from "./sticky.ts";

const span = (line: number, endLine: number, text = `l${line}`) => ({ from: line * 10, line, endLine, text });

test("stickyChain pins scopes that start above the top line and still cover it", () => {
	const scopes = [span(1, 50), span(3, 40), span(10, 20), span(30, 40)];
	expect(stickyChain(scopes, 15).map((r) => r.line)).toEqual([1, 3, 10]);
	expect(stickyChain(scopes, 35).map((r) => r.line)).toEqual([1, 3, 30]);
	expect(stickyChain(scopes, 1)).toEqual([]);
});

test("stickyChain caps to the innermost scopes and collapses same-line nesting", () => {
	const scopes = [span(1, 90), span(2, 80), span(3, 70), span(4, 60), span(4, 55, "inner")];
	const out = stickyChain(scopes, 30);
	expect(out.map((r) => r.line)).toEqual([2, 3, 4]);
	expect(out[2].text).toBe("inner");
	expect(stickyChain(scopes, 30, 1).map((r) => r.line)).toEqual([4]);
});

test("one-line scopes are not pinned", () => {
	expect(stickyChain([span(5, 5)], 6)).toEqual([]);
});

test("isScopeNode accepts declarations and skips imports/variables", () => {
	expect(isScopeNode("FunctionDeclaration")).toBe(true);
	expect(isScopeNode("ClassDeclaration")).toBe(true);
	expect(isScopeNode("ImplItem")).toBe(true);
	expect(isScopeNode("FunctionItem")).toBe(true);
	expect(isScopeNode("ImportDeclaration")).toBe(false);
	expect(isScopeNode("VariableDeclaration")).toBe(false);
	expect(isScopeNode("Block")).toBe(false);
});

test("scopesAt walks the Lezer ancestors of a position", () => {
	const doc = ["class A {", "  m() {", "    a;", "    b;", "  }", "}"].join("\n");
	const state = parsedState({ doc, extensions: [javascript()] });
	const pos = state.doc.line(3).from + 4;
	const scopes = scopesAt(state, pos);
	expect(scopes.map((s) => s.text)).toEqual(["class A {", "m() {"]);
	expect(stickyChain(scopes, 3).map((s) => s.line)).toEqual([1, 2]);
});
