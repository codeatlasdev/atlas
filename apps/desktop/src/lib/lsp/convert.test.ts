// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { Text } from "@codemirror/state";
import { offsetToPos, pathToUri, planWorkspaceEdit, posToOffset, uriToPath } from "./convert.ts";

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
