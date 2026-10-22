// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, it } from "bun:test";
import { typingLabel } from "./botFace.ts";

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
