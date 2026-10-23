// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, it } from "bun:test";
import { BOT_COLORS, defaultLook, normalizeLook, randomLook, shade } from "./look.ts";

describe("normalizeLook", () => {
	it("keeps each valid field and replaces only the invalid ones", () => {
		const d = defaultLook("bot-1");
		expect(normalizeLook({ color: "#ABCDEF", eyes: "lasers", hat: "crown" }, "bot-1")).toEqual({
			color: "#abcdef",
			eyes: d.eyes,
			hat: "crown",
		});
		expect(normalizeLook({ color: "red", eyes: "visor", hat: 7 }, "bot-1")).toEqual({
			color: d.color,
			eyes: "visor",
			hat: d.hat,
		});
	});

	it("rejects colours that are not #rrggbb (CSS injection, short hex)", () => {
		for (const color of ["#abc", "#abcdefff", "url(x)", "#12345g", " #123456"]) {
			expect(normalizeLook({ color }, "s").color).toBe(defaultLook("s").color);
		}
	});

	it("falls back to the seed default for non-objects", () => {
		for (const raw of [null, undefined, "crown", 3])
			expect(normalizeLook(raw, "s")).toEqual(defaultLook("s"));
	});

	it("accepts straw hat", () => {
		expect(normalizeLook({ color: "#3d5bff", eyes: "round", hat: "straw" }, "s")).toEqual({
			color: "#3d5bff",
			eyes: "round",
			hat: "straw",
		});
	});
});

describe("defaultLook", () => {
	it("is stable per seed and spreads over the palette", () => {
		expect(defaultLook("abc")).toEqual(defaultLook("abc"));
		const colors = new Set(Array.from({ length: 50 }, (_, i) => defaultLook(`bot-${i}`).color));
		expect(colors.size).toBe(BOT_COLORS.length);
	});

	it("default hat is straw", () => {
		expect(defaultLook("any-seed").hat).toBe("straw");
	});
});

describe("randomLook", () => {
	it("stays in range at the rand() boundaries", () => {
		for (const r of [0, 0.999999, 1])
			expect(
				normalizeLook(
					randomLook(() => r),
					"x",
				),
			).toEqual(randomLook(() => r));
	});
});

describe("shade", () => {
	it("reaches white and black at the extremes and clamps beyond", () => {
		expect(shade("#3d5bff", 1)).toBe("#ffffff");
		expect(shade("#3d5bff", -1)).toBe("#000000");
		expect(shade("#3d5bff", 5)).toBe("#ffffff");
		expect(shade("#3d5bff", 0)).toBe("#3d5bff");
		expect(shade("#000000", 0.5)).toBe("#808080");
	});
});
