// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, test, expect } from "bun:test";
import { CATALOG, CATEGORIES, filterCatalog, connectedIntegrationIds, canConnect } from "./integrations.ts";

describe("filterCatalog", () => {
	test("empty query + All returns full catalog", () => {
		expect(filterCatalog(CATALOG, "", "All")).toHaveLength(CATALOG.length);
	});

	test("query filters by name (case-insensitive)", () => {
		const result = filterCatalog(CATALOG, "github", "All");
		expect(result).toHaveLength(1);
		expect(result[0].id).toBe("github");
	});

	test("query filters by description", () => {
		const result = filterCatalog(CATALOG, "private repositories", "All");
		expect(result.some((i) => i.id === "github")).toBe(true);
	});

	test("category filter restricts to that category", () => {
		const result = filterCatalog(CATALOG, "", "Code");
		expect(result.every((i) => i.category === "Code")).toBe(true);
		expect(result.length).toBeGreaterThan(0);
	});

	test("category filter excludes other categories", () => {
		const result = filterCatalog(CATALOG, "", "Communication");
		expect(result.some((i) => i.category === "Code")).toBe(false);
	});

	test("query + category narrows both axes", () => {
		const result = filterCatalog(CATALOG, "repos", "Code");
		expect(result.every((i) => i.category === "Code")).toBe(true);
		expect(result.length).toBeGreaterThan(0);
	});

	test("query with no match returns empty array", () => {
		expect(filterCatalog(CATALOG, "zzznomatch", "All")).toHaveLength(0);
	});

	test("All category with whitespace-only query returns full catalog", () => {
		expect(filterCatalog(CATALOG, "   ", "All")).toHaveLength(CATALOG.length);
	});
});

describe("connectedIntegrationIds", () => {
	test("returns empty set when GitHub not connected", () => {
		expect(connectedIntegrationIds(false).size).toBe(0);
	});

	test("includes github id when GitHub connected", () => {
		const ids = connectedIntegrationIds(true);
		expect(ids.has("github")).toBe(true);
		expect(ids.size).toBe(1);
	});

	test("connected count equals 1 when GitHub connected", () => {
		expect(connectedIntegrationIds(true).size).toBe(1);
	});

	test("connected count equals 0 when GitHub not connected", () => {
		expect(connectedIntegrationIds(false).size).toBe(0);
	});
});

describe("canConnect", () => {
	test("GitHub (real) can be connected", () => {
		const github = CATALOG.find((i) => i.id === "github")!;
		expect(canConnect(github)).toBe(true);
	});

	test("all coming-soon integrations cannot be connected", () => {
		const comingSoon = CATALOG.filter((i) => i.status === "coming-soon");
		expect(comingSoon.length).toBeGreaterThan(0);
		for (const i of comingSoon) {
			expect(canConnect(i)).toBe(false);
		}
	});
});

describe("CATALOG integrity", () => {
	test("GitHub is the only real integration", () => {
		const real = CATALOG.filter((i) => i.status === "real");
		expect(real).toHaveLength(1);
		expect(real[0].id).toBe("github");
	});

	test("all categories in CATALOG are known", () => {
		const known = new Set(CATEGORIES.filter((c) => c !== "All"));
		for (const i of CATALOG) {
			expect(known.has(i.category)).toBe(true);
		}
	});

	test("coming-soon entries have no skills listed", () => {
		const comingSoon = CATALOG.filter((i) => i.status === "coming-soon");
		for (const i of comingSoon) {
			expect(i.skills).toHaveLength(0);
		}
	});

	test("GitHub skills include at least one available skill", () => {
		const github = CATALOG.find((i) => i.id === "github")!;
		expect(github.skills.some((s) => s.available)).toBe(true);
	});

	test("GitHub has a sourceUrl", () => {
		const github = CATALOG.find((i) => i.id === "github")!;
		expect(typeof github.sourceUrl).toBe("string");
	});
});
