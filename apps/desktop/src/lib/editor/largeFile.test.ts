// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { classifyFile, largeNotice } from "./largeFile.ts";

const small = { bytes: 1000, lines: 50, longestLine: 80 };

test("small files are normal: nothing disabled, no notice", () => {
	const c = classifyFile(small);
	expect(c.tier).toBe("normal");
	expect(c.reasons).toEqual([]);
	expect(largeNotice(c.tier)).toBeNull();
	expect(Object.values(c.disable).filter((v) => v === true)).toEqual([]);
	expect(c.disable.syntax).toBe("full");
});

test("byte boundary: 2 MB is large, just below is normal", () => {
	expect(classifyFile({ ...small, bytes: 1_999_999 }).tier).toBe("normal");
	expect(classifyFile({ ...small, bytes: 2_000_000 }).tier).toBe("large");
});

test("byte boundary: 5x is huge, just below is large", () => {
	expect(classifyFile({ ...small, bytes: 9_999_999 }).tier).toBe("large");
	expect(classifyFile({ ...small, bytes: 10_000_000 }).tier).toBe("huge");
});

test("the threshold comes from the pref", () => {
	const t = { largeBytes: 100_000 };
	expect(classifyFile({ ...small, bytes: 99_999 }, t).tier).toBe("normal");
	expect(classifyFile({ ...small, bytes: 100_000 }, t).tier).toBe("large");
	expect(classifyFile({ ...small, bytes: 500_000 }, t).tier).toBe("huge");
});

test("long-line boundaries: 10_000 stays normal, 10_001 large, 200_000 large, 200_001 huge", () => {
	expect(classifyFile({ ...small, longestLine: 10_000 }).tier).toBe("normal");
	expect(classifyFile({ ...small, longestLine: 10_001 }).tier).toBe("large");
	expect(classifyFile({ ...small, longestLine: 200_000 }).tier).toBe("large");
	expect(classifyFile({ ...small, longestLine: 200_001 }).tier).toBe("huge");
});

test("a small minified file is large by its line", () => {
	const c = classifyFile({ bytes: 50_000, lines: 1, longestLine: 50_000 });
	expect(c.tier).toBe("large");
	expect(c.reasons).toEqual(["a line has 50,000 characters"]);
});

test("the worst rule wins and every reason is listed", () => {
	const c = classifyFile({ bytes: 3_000_000, lines: 2, longestLine: 300_000 });
	expect(c.tier).toBe("huge");
	expect(c.reasons).toEqual(["3.0 MB file", "a line has 300,000 characters"]);
	const d = classifyFile({ bytes: 20_000_000, lines: 2, longestLine: 20_000 });
	expect(d.tier).toBe("huge");
	expect(d.reasons).toHaveLength(2);
});

test("large tier: heavy features off, viewport syntax, fold gutter kept", () => {
	const c = classifyFile({ ...small, bytes: 3_000_000 });
	expect(c.disable).toEqual({
		wrap: true,
		bracketColors: true,
		indentGuides: true,
		minimap: true,
		stickyScroll: true,
		lsp: true,
		foldGutter: false,
		syntax: "viewport",
		inlayHints: true,
		selectionMatches: true,
	});
	expect(largeNotice(c.tier)).toBe(m.workbench_editor_notice_large());
});

test("huge tier: no syntax, no fold gutter, its own notice", () => {
	const c = classifyFile({ ...small, bytes: 50_000_000 });
	expect(c.disable.syntax).toBe("none");
	expect(c.disable.foldGutter).toBe(true);
	expect(c.disable.lsp).toBe(true);
	expect(largeNotice(c.tier)).toBe(m.workbench_editor_notice_huge());
});

test("results do not share mutable state", () => {
	const a = classifyFile({ ...small, bytes: 3_000_000 });
	a.disable.lsp = false;
	expect(classifyFile({ ...small, bytes: 3_000_000 }).disable.lsp).toBe(true);
});
