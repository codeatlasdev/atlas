// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { MARK, MARK_GRADIENT, MARK_VIEW } from "./mark.ts";

const svg = readFileSync(join(import.meta.dir, "../../../../../../assets/logo.svg"), "utf8");

// Incident-in-waiting: mark.ts is generated from the logo by `bun run icons` (scripts/icons.ts). If the logo is redrawn
// and the generator is not re-run, the intro would animate the OLD mark. This pins the generated data to the real file.
test("the generated tiles are exactly the paths of assets/logo.svg, in order, with its viewBox", () => {
	const paths = [...svg.matchAll(/<path d="([^"]+)"/g)].map((m) => m[1]);
	expect(MARK.length).toBe(paths.length);
	MARK.forEach((m, i) => {
		expect(m.id).toBe(i);
		expect(m.d).toBe(paths[i]);
	});
	const v = /viewBox="0 0 (\d+) (\d+)"/.exec(svg);
	expect([MARK_VIEW.w, MARK_VIEW.h]).toEqual([Number(v[1]), Number(v[2])]);
});

test("every bounding box lies inside the viewBox", () => {
	for (const m of MARK) {
		expect(m.cx - m.w / 2).toBeGreaterThanOrEqual(-1);
		expect(m.cx + m.w / 2).toBeLessThanOrEqual(MARK_VIEW.w + 1);
		expect(m.cy - m.h / 2).toBeGreaterThanOrEqual(-1);
		expect(m.cy + m.h / 2).toBeLessThanOrEqual(MARK_VIEW.h + 1);
	}
});

// Incident (v1.5.0-alpha.7): the menu-bar popover header drew its own purple gradient tile, the OLD brand, because the
// mark's colours lived only in assets/logo.svg and the tray page hand-kept a CSS gradient. The gradient is now part of
// the generated module, so every consumer of the mark gets the colours from the single source.
test("the generated mark carries the logo's gradient (axis and stops, in order) from assets/logo.svg", () => {
	const stops = [...svg.matchAll(/<stop offset="([^"]+)" stop-color="([^"]+)"\/>/g)].map((m) => ({
		offset: Number(m[1]),
		color: m[2],
	}));
	expect(stops.length).toBeGreaterThanOrEqual(2);
	expect(MARK_GRADIENT.stops).toEqual(stops);
	const g = /<linearGradient[^>]*x1="([^"]+)" y1="([^"]+)" x2="([^"]+)" y2="([^"]+)"/.exec(svg);
	expect([MARK_GRADIENT.x1, MARK_GRADIENT.y1, MARK_GRADIENT.x2, MARK_GRADIENT.y2]).toEqual(
		g.slice(1).map(Number),
	);
});
