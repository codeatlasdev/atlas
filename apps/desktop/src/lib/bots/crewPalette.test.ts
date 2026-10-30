// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, test, expect } from "bun:test";
import { filterRows, cmdKeyIndex, keycapLabel } from "./crewPalette.ts";
import type { PaletteRow } from "./crewPalette.ts";

const bots: PaletteRow[] = [
	{ kind: "bot", id: "b1", hostKey: "ssh:atlas-bot@ship", name: "Carlos", subtitle: "Engineer: reviews code" },
	{ kind: "bot", id: "b2", hostKey: "ssh:atlas-bot@ship", name: "Amélie", subtitle: "Designer: figma stuff" },
	{ kind: "bot", id: "b3", hostKey: "ssh:atlas-bot@ship", name: "Søren", subtitle: "DevOps" },
];

const cmds: PaletteRow[] = [
	{ kind: "command", id: "new", name: "New crewmate" },
	{ kind: "command", id: "integrations", name: "Integrations" },
];

const all = [...bots, ...cmds];

describe("filterRows", () => {
	test("empty query returns all rows", () => {
		expect(filterRows(all, "")).toEqual(all);
		expect(filterRows(all, "   ")).toEqual(all);
	});

	test("matches by bot name (case-insensitive)", () => {
		expect(filterRows(all, "carlos")).toEqual([bots[0]]);
		expect(filterRows(all, "CARLOS")).toEqual([bots[0]]);
	});

	test("matches by subtitle", () => {
		expect(filterRows(all, "figma")).toEqual([bots[1]]);
	});

	test("matches accents insensitively (Amélie → amelie)", () => {
		expect(filterRows(all, "amelie")).toEqual([bots[1]]);
		expect(filterRows(all, "Amelie")).toEqual([bots[1]]);
	});

	test("matches names with special chars (Søren → soren)", () => {
		expect(filterRows(all, "soren")).toEqual([bots[2]]);
	});

	test("matches command names", () => {
		expect(filterRows(all, "integrations")).toEqual([cmds[1]]);
	});

	test("no match returns empty array", () => {
		expect(filterRows(all, "zzznomatch")).toEqual([]);
	});

	test("partial match returns subset", () => {
		const result = filterRows(all, "crew");
		expect(result).toEqual([cmds[0]]);
	});
});

describe("cmdKeyIndex", () => {
	test("'1' maps to index 0", () => {
		expect(cmdKeyIndex("1")).toBe(0);
	});

	test("'9' maps to index 8", () => {
		expect(cmdKeyIndex("9")).toBe(8);
	});

	test("'0' returns -1", () => {
		expect(cmdKeyIndex("0")).toBe(-1);
	});

	test("non-digit returns -1", () => {
		expect(cmdKeyIndex("a")).toBe(-1);
		expect(cmdKeyIndex("")).toBe(-1);
	});
});

describe("keycapLabel", () => {
	test("index 0 → '1'", () => {
		expect(keycapLabel(0)).toBe("1");
	});

	test("index 8 → '9'", () => {
		expect(keycapLabel(8)).toBe("9");
	});

	test("index 9 → ''", () => {
		expect(keycapLabel(9)).toBe("");
	});

	test("negative index → ''", () => {
		expect(keycapLabel(-1)).toBe("");
	});
});
