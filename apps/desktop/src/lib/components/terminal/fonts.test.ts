// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { fontCandidates } from "./fonts.ts";

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
