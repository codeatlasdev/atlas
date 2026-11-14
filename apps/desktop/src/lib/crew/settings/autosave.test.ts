// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, it, expect } from "bun:test";
import { isDirty, updateSavedRef } from "./autosave.ts";

describe("isDirty", () => {
	it("returns false when current equals last saved", () => {
		expect(isDirty("hello", "hello")).toBe(false);
	});

	it("returns true when current differs from last saved", () => {
		expect(isDirty("hello", "world")).toBe(true);
	});

	it("returns true for empty vs non-empty", () => {
		expect(isDirty("", "saved")).toBe(true);
	});

	it("returns false when both are empty", () => {
		expect(isDirty("", "")).toBe(false);
	});

	it("does not treat whitespace as equal to empty", () => {
		expect(isDirty("", " ")).toBe(true);
	});
});

describe("updateSavedRef", () => {
	it("advances to attempted value on success", () => {
		expect(updateSavedRef("old", "new", true)).toBe("new");
	});

	it("keeps oldSaved on failure — field stays dirty for retry", () => {
		expect(updateSavedRef("old", "new", false)).toBe("old");
	});

	it("never loses the edit on error: isDirty remains true after failure", () => {
		// User edits "a" → "b", save fails; savedRef must stay "a" so retry fires
		const ref = updateSavedRef("a", "b", false);
		expect(isDirty("b", ref)).toBe(true);
	});

	it("marks field clean after success", () => {
		const ref = updateSavedRef("a", "b", true);
		expect(isDirty("b", ref)).toBe(false);
	});

	it("does not save unchanged value: isDirty guards the call site", () => {
		// Caller must check isDirty before calling save; helper just tracks ref
		expect(isDirty("same", "same")).toBe(false);
	});
});
