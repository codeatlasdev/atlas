// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { describe, expect, test } from "bun:test";

// Dynamic import on purpose: the Svelte loader registered by test/setup.ts must exist before this module loads.
const { readMode } = await import("./mode.svelte.ts");

describe("the saved microphone key mode", () => {
	test("hold to talk unless the person chose tap; a stray stored value is hold", () => {
		const store = (v: string | null) => ({ getItem: () => v });
		expect(readMode(store(null))).toBe("hold");
		expect(readMode(store("tap"))).toBe("tap");
		expect(readMode(store("hold"))).toBe("hold");
		expect(readMode(store("toggle"))).toBe("hold");
	});
});
