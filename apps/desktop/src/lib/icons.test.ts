// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { resolveFile, resolveFolder, fileIcon, folderIcon } from "./icons.ts";

const m = {
	file: "file",
	folder: "folder",
	folderOpen: "folder-open",
	fileNames: { "package.json": "nodejs", dockerfile: "docker" },
	fileExtensions: { ts: "typescript", "test.ts": "typescript-test", "d.ts": "typescript-def", json: "json" },
	folderNames: { src: "folder-src" },
	folderNamesExpanded: { src: "folder-src-open" },
};

describe("resolveFile", () => {
	test("exact file name wins over the extension", () => {
		expect(resolveFile(m, "package.json")).toBe("nodejs");
	});
	test("longest dotted suffix wins", () => {
		expect(resolveFile(m, "foo.test.ts")).toBe("typescript-test");
		expect(resolveFile(m, "index.d.ts")).toBe("typescript-def");
	});
	test("falls back to the shorter suffix", () => {
		expect(resolveFile(m, "foo.bar.ts")).toBe("typescript");
	});
	test("is case-insensitive", () => {
		expect(resolveFile(m, "Dockerfile")).toBe("docker");
		expect(resolveFile(m, "INDEX.D.TS")).toBe("typescript-def");
	});
	test("unknown names and dotfiles get the default", () => {
		expect(resolveFile(m, "README")).toBe("file");
		expect(resolveFile(m, "a.unknown")).toBe("file");
		expect(resolveFile(m, ".hidden")).toBe("file");
	});
});

describe("resolveFolder", () => {
	test("closed and expanded use their own tables", () => {
		expect(resolveFolder(m, "src", false)).toBe("folder-src");
		expect(resolveFolder(m, "src", true)).toBe("folder-src-open");
	});
	test("is case-insensitive", () => {
		expect(resolveFolder(m, "SRC", false)).toBe("folder-src");
	});
	test("unknown folders fall back by state", () => {
		expect(resolveFolder(m, "whatever", false)).toBe("folder");
		expect(resolveFolder(m, "whatever", true)).toBe("folder-open");
	});
});

describe("bundled manifest", () => {
	test("every resolved icon has an svg", () => {
		for (const f of ["a.ts", "index.d.ts", "package.json", "Cargo.toml", "noext"]) {
			expect(fileIcon(f)).toStartWith("<svg");
		}
		expect(folderIcon("src", true)).toStartWith("<svg");
		expect(folderIcon("nope", false)).toStartWith("<svg");
	});
});
