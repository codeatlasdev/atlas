// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

// Incident (selection stall): the four `--space-*` custom properties were written on `.app`, so every switch restyled the
// whole page (~1700 nodes, 8 ms p50; 2 ms once a layer carries its own). `--space-accent-text` and `--space-on-accent` left
// `.app` because nothing outside a space layer reads them. This pins that: a new reader outside the layer subtree would
// silently fall back to the brand colour, so it must show up here and be placed deliberately.
const src = join(import.meta.dir, "..", "..");
const walk = (d: string): string[] =>
	readdirSync(d).flatMap((f) => {
		const p = join(d, f);
		return statSync(p).isDirectory() ? walk(p) : [p];
	});
const files = walk(src).filter((f) => /\.(svelte|css|ts)$/.test(f) && !/\.test\.ts$/.test(f));
const readers = (name: string) =>
	files
		.filter((f) => new RegExp(`var\\(\\s*--${name}\\b`).test(readFileSync(f, "utf8")))
		.map((f) => relative(src, f).replaceAll("\\", "/"))
		.sort();

/** Files that render only below a space layer (SpaceView → ProjectHeader / Workbench → panes, panel, viewers). */
const BELOW_A_LAYER = [
	"lib/components/Editor.svelte", // TabGroupView → editor tab
	"lib/panel/PanelHeader.svelte", // Panel tab
	"lib/panel/PanelOps.svelte", // ProjectHeader
	"lib/search/SearchForm.svelte", // left dock → SearchPanel
	"lib/shell/RightToggle.svelte", // ProjectHeader
	"lib/viewers/ImageViewer.svelte", // editor tab viewers
	"lib/viewers/ViewerActions.svelte",
	"lib/viewers/pdf/PdfToolbar.svelte",
	"lib/onboarding/Tour.svelte", // NOT below a layer: see the test below
	"routes/(app)/ProjectHeader.svelte",
];

test("--space-accent-text and --space-on-accent are read only by layer descendants (and the tour's fallback)", () => {
	const all = [...new Set([...readers("space-accent-text"), ...readers("space-on-accent")])];
	expect(all.sort()).toEqual([...BELOW_A_LAYER].sort());
});

// #452: `--space-accent` and `--space-tint` on `.app` still cost ~2 ms of style recalculation per switch (a custom property
// inherits, so the whole chrome restyles). The few readers outside a layer carry the value on their own element.
test("no --space-* custom property is written on `.app` or `.content`; the chrome's readers carry their own", () => {
	const layout = readFileSync(join(src, "routes/(app)/+layout.svelte"), "utf8");
	const open = (cls: string) => {
		const at = layout.indexOf(`class="${cls}"`);
		const start = layout.lastIndexOf("<", at);
		return layout.slice(start, layout.indexOf(">", at));
	};
	expect(open("app")).not.toContain("--space-");
	expect(
		layout.slice(layout.indexOf('<main class="content"'), layout.indexOf(">", layout.indexOf("<main"))),
	).not.toContain("--space-");
	expect(open("space-wash")).toContain("style:--space-tint=");
	expect(open("nav-chip nav-chip-prev")).toContain("style:--space-accent=");
	expect(open("nav-chip nav-chip-next")).toContain("style:--space-accent=");
});

test("the edge peek's glass and the selected worktree row carry --space-accent themselves", () => {
	const peek = readFileSync(join(src, "lib/shell/EdgePeek.svelte"), "utf8");
	const row = readFileSync(join(src, "lib/components/sidebar/WorktreeRow.svelte"), "utf8");
	expect(peek).toMatch(/class="peek-glass"[^>]*style:--space-accent=/s);
	expect(row).toContain("style:--space-accent=");
});

test("every layer declares all four vars itself (the only source for the readers above)", () => {
	const layer = readFileSync(join(src, "routes/(app)/SpaceLayer.svelte"), "utf8");
	for (const v of ["--space-accent", "--space-accent-text", "--space-on-accent", "--space-tint"])
		expect(layer).toContain(`style:${v}=`);
});
