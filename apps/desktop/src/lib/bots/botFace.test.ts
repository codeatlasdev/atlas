// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, it } from "bun:test";
import { botColor, botVariant, typingLabel } from "./botFace.ts";

describe("botColor", () => {
	it("returns a valid hex string", () => {
		expect(botColor("abc")).toMatch(/^#[0-9a-f]{6}$/i);
	});

	it("is stable for the same seed", () => {
		expect(botColor("my-bot-id")).toBe(botColor("my-bot-id"));
	});

	it("returns different colors for different seeds (probabilistic)", () => {
		// Not guaranteed for all pairs, but these seeds hash differently
		const colors = ["alpha", "beta", "gamma", "delta", "epsilon"].map(botColor);
		const unique = new Set(colors);
		expect(unique.size).toBeGreaterThan(1);
	});
});

describe("botVariant", () => {
	it("returns a value in 0–3", () => {
		for (const seed of ["a", "bb", "ccc", "dddd", "bot-1", "bot-2", "abc123"]) {
			const v = botVariant(seed);
			expect(v).toBeGreaterThanOrEqual(0);
			expect(v).toBeLessThanOrEqual(3);
		}
	});

	it("is stable for the same seed", () => {
		expect(botVariant("test-seed")).toBe(botVariant("test-seed"));
	});

	it("covers all 4 variants across a range of seeds", () => {
		const variants = new Set<number>();
		for (let i = 0; i < 100; i++) variants.add(botVariant(`seed-${i}`));
		expect(variants.size).toBe(4);
	});
});

describe("typingLabel", () => {
	it("returns empty string for empty array", () => {
		expect(typingLabel([])).toBe("");
	});

	it("single name → 'X is working'", () => {
		expect(typingLabel(["Atlas"])).toBe("Atlas is working");
	});

	it("two names → 'A and B are working'", () => {
		expect(typingLabel(["Alice", "Bob"])).toBe("Alice and Bob are working");
	});

	it("three names → '3 bots are working'", () => {
		expect(typingLabel(["A", "B", "C"])).toBe("3 bots are working");
	});

	it("many names → 'N bots are working'", () => {
		expect(typingLabel(["A", "B", "C", "D", "E"])).toBe("5 bots are working");
	});
});
