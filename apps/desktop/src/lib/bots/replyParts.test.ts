// @ts-nocheck
import { describe, expect, test } from "bun:test";
import { replyParts } from "./replyParts.ts";

// A long crewmate reply reads like a document in one bubble. It is shown as a few chat messages instead, split at its
// own sections, never inside a code block, list or table.
describe("replyParts", () => {
	test("a short reply stays one message", () => {
		expect(replyParts("Feito. A branch é a main.")).toEqual(["Feito. A branch é a main."]);
	});

	test("splits at section headers (bold line or #) and keeps each header with its body", () => {
		const r = "Intro curta.\n\n**Raiz**\n- a\n- b\n\n**src/**\n- c\n\nSe quiser, detalho.";
		expect(replyParts(r, 0)).toEqual(["Intro curta.", "**Raiz**\n- a\n- b", "**src/**\n- c", "Se quiser, detalho."]);
	});

	test("never cuts a fenced code block, even with blank lines inside", () => {
		const r = "Rode:\n\n```sh\nbun i\n\nbun test\n```\n\n**Depois**\nPronto.";
		const parts = replyParts(r, 0);
		expect(parts.some((p) => p.includes("bun i\n\nbun test"))).toBe(true);
		expect(parts.every((p) => (p.match(/```/g) ?? []).length % 2 === 0)).toBe(true);
	});

	test("keeps a table whole", () => {
		const r = "**Arquivos**\n| a | b |\n|---|---|\n| 1 | 2 |\n\n| 3 | 4 |";
		expect(replyParts(r, 0)).toEqual([r]);
	});

	test("merges tiny pieces so it never becomes a flood of one-liners", () => {
		const r = "A.\n\nB.\n\nC.\n\nD.\n\nE.\n\nF.\n\nG.";
		const parts = replyParts(r, 0);
		expect(parts.length).toBeLessThanOrEqual(3);
		expect(parts.join("\n\n")).toBe(r);
	});

	test("nothing is lost or reordered", () => {
		const r = "Intro.\n\n**Um**\nx\n\n**Dois**\ny\n\nFim.";
		expect(replyParts(r, 0).join("\n\n")).toBe(r);
	});
});
