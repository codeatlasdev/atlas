// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Incident (user, 2026-10-13): the "Enable omp integration" card sat on top of the "Getting started" card; seven
 * independent `position: fixed` pieces each hard-coded their own `right`/`bottom`. They live in one column now, and
 * (2026-11-15) every card in it is one `CornerCard`: same width, radius, elevation and motion, placed by the plan.
 */
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { CORNER_ID, corner } from "./cornerStack.ts";

const src = (p) => readFileSync(join(import.meta.dir, "../../", p), "utf8");

describe("the corner is one container and one card shell, not seven fixed boxes", () => {
	// Every piece that used to anchor itself at the bottom-right corner (`position: fixed` + `right` + `bottom`).
	const PIECES = [
		"lib/onboarding/Checklist.svelte",
		"routes/(app)/SpaceView.svelte",
		"lib/settings/theme/GhosttyOffer.svelte",
		"lib/components/UpdatedCard.svelte",
		"lib/errors/ErrorToasts.svelte",
		"lib/panel/ActionResultToast.svelte",
		"lib/shell/KitToasts.svelte",
	];
	for (const p of PIECES)
		test(`${p} is a corner card instead of anchoring itself`, () => {
			const s = src(p);
			expect(s).toMatch(/<Corner(Card|Notice)\b/);
			expect(s).not.toMatch(/position:\s*fixed[^}]*\bbottom:/);
			expect(s).not.toContain("use:corner");
		});

	test("the kit's own Toaster is not mounted: its stack ignores the coordinator", () => {
		expect(src("lib/shell/CornerStack.svelte")).not.toContain("<Toaster");
		expect(src("lib/shell/CornerStack.svelte")).toContain("<KitToasts");
		expect(src("routes/(app)/+layout.svelte")).not.toContain("<Toaster");
	});

	test("the corner action moves a node into #atlas-corner and takes it out again", () => {
		document.body.innerHTML = `<div id="${CORNER_ID}"></div><section id="home"><p id="p">x</p></section>`;
		const node = document.getElementById("p");
		const act = corner(node);
		expect(node.parentElement.id).toBe(CORNER_ID);
		act.destroy();
		expect(document.getElementById("p")).toBeNull();
	});

	test("without a corner (tray page) the node stays where it is", () => {
		document.body.innerHTML = `<section id="home"><p id="p">x</p></section>`;
		const node = document.getElementById("p");
		expect(corner(node)).toBeUndefined();
		expect(node.parentElement.id).toBe("home");
	});

	test("the shell mounts the corner column once, fixed bottom-right, a full-width sheet when narrow", () => {
		const s = src("lib/shell/CornerStack.svelte");
		expect(s).toContain("id={CORNER_ID}");
		expect(CORNER_ID).toBe("atlas-corner");
		expect(s).toMatch(/position:\s*fixed/);
		expect(s).toContain("var(--corner-margin)");
		expect(s).toContain("var(--corner-gap)");
		expect(s).toMatch(/@media \(max-width: 520px\)[\s\S]*left:/);
		expect(src("routes/(app)/+layout.svelte")).toContain("<CornerStack");
	});

	test("every card is as wide as the column and takes its radius and elevation from the kit", () => {
		const s = src("lib/shell/CornerCard.svelte");
		expect(s).toMatch(/\.card\s*\{[^}]*width:\s*100%/);
		expect(s).toContain("var(--ca-radius-lg");
		expect(s).toContain("var(--ca-shadow-md)");
		expect(s).toContain("var(--ca-surface-raised)");
	});
});
