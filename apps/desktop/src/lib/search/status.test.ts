// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../test/locale.ts";

const status = (matches: number, files: number, more = "") =>
	m.search_form_status({ matches, files, matchesLabel: `${matches}${more}`, filesLabel: `${files}${more}` });

test("the result count is a plural with an exact zero and counts files by their own plural", () => {
	expect(status(0, 0)).toBe("No results");
	expect(status(1, 1)).toBe("1 result in 1 file");
	expect(status(12, 1)).toBe("12 results in 1 file");
	expect(status(12, 4)).toBe("12 results in 4 files");
	expect(status(100, 12, "+")).toBe("100+ results in 12+ files");
});

test("in pt-BR zero is its own sentence, never '0 resultado', and the plus sign survives", async () => {
	await inLocale("pt-BR", () => {
		expect(status(0, 0)).not.toMatch(/\b0\b/);
		expect(status(1, 1)).not.toBe(status(2, 1));
		expect(status(100, 12, "+")).toContain("100+");
		expect(status(100, 12, "+")).toContain("12+");
	});
});
