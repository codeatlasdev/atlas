// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

// The macOS traffic lights live in the window's title band, which the page draws around: `.content` starts
// `--window-pad` below the window top and its first row (project header, empty states) is `--title-row` tall; the
// sidebar's first row (`.drag`) spans the same band. The lights must be centred on that band by construction.
//
// tao (`inset_traffic_lights`) resizes the title-bar container to `button + y` and leaves each 14 px button at its
// stock frame origin 9 px above the container's bottom, so the button's top edge sits at window y = `y - 9`
// (stock: container 32 px, top at 9; measured on macOS 27 with an NSWindow, buttons are 14x14 at origin.y 9).
// The lights' centre is therefore `y - 9 + 14 / 2 = y - 2`.
const BUTTON = 14;
const STOCK_BUTTON_ORIGIN_Y = 9;

const css = readFileSync(new URL("../app.css", import.meta.url), "utf8");
const conf = JSON.parse(
	readFileSync(new URL("../../../../crates/atlas-desktop/tauri.conf.json", import.meta.url), "utf8"),
);
const read = (name: string, src = css) => {
	const m = src.match(new RegExp(`${name}:\\s*(\\d+)px`));
	if (!m) throw new Error(`${name} missing`);
	return Number(m[1]);
};
const read1 = (file: string) => readFileSync(new URL(file, import.meta.url), "utf8");

const pad = read("--window-pad");
const row = read("--title-row");
const lightsY = conf.app.windows[0].trafficLightPosition.y;

test("traffic lights are centred on the title row (one constant, mirrored in tauri.conf.json)", () => {
	const rowCentre = pad + row / 2;
	const lightsCentre = lightsY - STOCK_BUTTON_ORIGIN_Y + BUTTON / 2;
	expect(lightsCentre).toBe(rowCentre);
});

test("every first row is built from the shared title band, not its own number", () => {
	// Sidebar top row: same band as the header, so the toggle/refresh buttons share the lights' centre.
	const sidebar = read1("./components/Sidebar.svelte");
	const drag = sidebar.slice(
		sidebar.indexOf("\t.drag {"),
		sidebar.indexOf("}", sidebar.indexOf("\t.drag {")),
	);
	expect(drag).toContain("height: calc(var(--window-pad) + var(--title-row))");
	expect(drag).toContain("padding-top: var(--window-pad)");
	// Project header and the empty-state drag strips: the title row itself.
	for (const f of [
		"../routes/(app)/ProjectHeader.svelte",
		"../routes/(app)/NoProject.svelte",
		"../routes/(app)/MissingProject.svelte",
	]) {
		expect(read1(f)).toContain("height: var(--title-row)");
	}
	// `.content` is inset by the window pad; the peek panel starts below the band.
	const content = css.slice(css.indexOf("\n.content {"), css.indexOf("}", css.indexOf("\n.content {")));
	expect(content).toContain("margin: var(--window-pad) var(--window-pad) var(--window-pad) 0");
	expect(css).toContain("top: calc(var(--window-pad) + var(--title-row))");
});
