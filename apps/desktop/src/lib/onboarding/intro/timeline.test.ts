// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { TILES } from "./timeline.ts";

describe("tiles come from the real Atlas mark", () => {
	test("the animated pieces are the 12 sheets plus the rim pieces, in a 428x427 box (sub-pixel trace specks stay still)", () => {
		const sheets = TILES.filter((t) => t.kind === "sheet");
		const rim = TILES.filter((t) => t.kind === "rim");
		expect(sheets.length).toBe(12);
		expect(rim.length).toBeGreaterThanOrEqual(10);
		for (const t of TILES) {
			expect(t.cx).toBeGreaterThanOrEqual(0);
			expect(t.cx).toBeLessThanOrEqual(428);
			expect(t.cy).toBeGreaterThanOrEqual(0);
			expect(t.cy).toBeLessThanOrEqual(427);
			expect(t.w * t.h).toBeGreaterThan(100);
		}
	});
	test("ids are unique and index the SVG paths (so the renderer can fetch each path by id)", () => {
		expect(new Set(TILES.map((t) => t.id)).size).toBe(TILES.length);
		for (const t of TILES) expect(Number.isInteger(t.id)).toBe(true);
	});
});
