// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { ancestorsOf, visibleRows } from "./tree.ts";

const e = (name, kind = "file") => ({ name, kind, size: 0, mtime: 0 });

test("visible rows descend only into expanded, loaded folders", () => {
	const children = { "": [e("src", "dir"), e("a.md")], src: [e("x.ts")], lib: [e("y.ts")] };
	expect(visibleRows(children, {}).map((r) => r.rel)).toEqual(["src", "a.md"]);
	expect(visibleRows(children, { src: true }).map((r) => r.rel)).toEqual(["src", "src/x.ts", "a.md"]);
	expect(visibleRows({ "": [e("src", "dir")] }, { src: true }).map((r) => r.rel)).toEqual(["src"]);
});

test("ancestors run outermost first and are empty at the root", () => {
	expect(ancestorsOf("a/b/c.ts")).toEqual(["a", "a/b"]);
	expect(ancestorsOf("c.ts")).toEqual([]);
});
