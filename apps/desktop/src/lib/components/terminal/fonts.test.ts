// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { fontCandidates, insetRect } from "./fonts.ts";

test("splits a CSS list, strips quotes and whitespace, drops generic keywords", () => {
	expect(fontCandidates("\"Fira Code\", 'JetBrains Mono' ,Menlo, ui-monospace, monospace")).toEqual([
		"Fira Code",
		"JetBrains Mono",
		"Menlo",
	]);
	expect(fontCandidates("SF Mono, Menlo, ui-monospace, monospace")).toEqual(["SF Mono", "Menlo"]);
});

test("commas inside quotes stay in the name; generic names are case-insensitive", () => {
	expect(fontCandidates('"Foo, Bar", Monospace, SERIF, System-UI, sans-serif')).toEqual(["Foo, Bar"]);
});

test("only generics or blanks yield no candidates", () => {
	expect(fontCandidates("monospace")).toEqual([]);
	expect(fontCandidates(" , ,")).toEqual([]);
	expect(fontCandidates("")).toEqual([]);
});

test("a quoted generic keyword is dropped like the bare one", () => {
	expect(fontCandidates('"monospace", Menlo')).toEqual(["Menlo"]);
});

test("insetRect shrinks by the padding on every side and never goes below 1", () => {
	expect(insetRect({ left: 10, top: 20, width: 200, height: 100 }, 8, 4)).toEqual({
		x: 18,
		y: 24,
		w: 184,
		h: 92,
	});
	expect(insetRect({ left: 0, top: 0, width: 10, height: 6 }, 64, 64)).toEqual({ x: 64, y: 64, w: 1, h: 1 });
	expect(insetRect({ left: 5, top: 5, width: 50, height: 50 }, 0, 0)).toEqual({ x: 5, y: 5, w: 50, h: 50 });
});
