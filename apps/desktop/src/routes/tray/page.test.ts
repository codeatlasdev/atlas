// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const page = readFileSync(join(import.meta.dir, "+page.svelte"), "utf8");
const kit = readFileSync(
	join(import.meta.dir, "../../../node_modules/@codeatlasdev/ui/dist/styles/tokens.css"),
	"utf8",
);

// Incident (v1.5.0-alpha.7): the popover was "pure black, not Atlas Dark". The kit declares its WHOLE token set on any
// element with the class `dark` (`.dark, [data-theme-scope="dark"] { --ca-surface-*, --ca-brand-* … }`, tokens.css; the preset files repeat it), not only on <html>. The page
// put `class:dark` on its root, so inside it every `var(--ca-*)` was the kit's palette, never the applied theme's
// (which lives on <html>). The scheme flag must stay on <html>; no element of the page may carry `dark`.
test("the kit scopes its tokens on a bare .dark class (why no element may carry it)", () => {
	expect(kit).toMatch(/^\.dark,\s*\n\[data-theme-scope="dark"\]\s*\{/m);
});

test("no element of the tray page carries the kit's `dark` class, which would replace the applied theme", () => {
	expect(page).not.toMatch(/class:dark/);
	expect(page).not.toMatch(/class="[^"]*\bdark\b[^"]*"/);
	expect(page).not.toMatch(/\.pop\.dark|\.dark\b/);
});

test("accent colours still follow the resolved scheme without a class on the page", () => {
	expect(page).toMatch(/colorScheme\.resolved === "dark"/);
});

// Incident: the sparkline under the project card was a broken, clipped line. `@codeatlasdev/ui`'s Sparkline is an
// ECharts CANVAS, which rejects `var(--c)` and `oklch()` (the popover passed both) and was boxed in 22px. The popover
// draws an inline SVG instead (tray.ts `sparkPaths`), which takes CSS colours and stretches to its card.
test("the popover does not use the canvas Sparkline, which cannot take var()/oklch() colours", () => {
	expect(page).not.toMatch(/Sparkline\b/);
	expect(page).not.toMatch(/@codeatlasdev\/ui\/charts/);
	expect(page).toMatch(/sparkPaths\(/);
});

// Incident: the header showed the old purple tile. The mark must come from the generated module (assets/logo.svg via
// `bun run icons`), never from a hand-kept colour in the page.
test("the header draws the generated mark, with no hand-kept brand colours", () => {
	expect(page).toMatch(/from "#lib\/onboarding\/intro\/mark\.ts"/);
	expect(page).not.toMatch(/#c4b5fd|#8b5cf6|#3b82f6/i);
});

// Incident: the Agents/Terminals cards left an empty grid cell next to the lone project card. Agents and Terminals are
// one two-column row of their own, always two cells, independent of how many projects run.
test("agents and terminals are a balanced two-cell row, not cells of the project grid", () => {
	const tiles = /<section class="tiles"[\s\S]*?<\/section>/.exec(page)?.[0] ?? "";
	expect((tiles.match(/class="card tile-stat"/g) ?? []).length).toBe(2);
	expect(page).toMatch(/\.tiles\s*\{[^}]*grid-template-columns:\s*1fr 1fr/);
});

// Incident: the list was cut off at the bottom with no scroll. The list is its own scroll region between a pinned
// header and footer, so a tall list scrolls inside the window instead of overflowing it.
test("the project list is a scroll region of its own, header and footer stay pinned", () => {
	expect(page).toMatch(/\.scroll\s*\{[^}]*overflow-y:\s*auto/);
	expect(page).toMatch(/\.scroll\s*\{[^}]*min-height:\s*0/);
	expect(page).toMatch(/\.pop\s*\{[^}]*overflow:\s*hidden/);
});
