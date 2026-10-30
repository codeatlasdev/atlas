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

// Incident: the header showed the old purple tile. The mark must come from the generated module (assets/logo.svg via
// `bun run icons`), never from a hand-kept colour in the page.
test("the header draws the generated mark, with no hand-kept brand colours", () => {
	expect(page).toMatch(/from "#lib\/onboarding\/intro\/mark\.ts"/);
	expect(page).not.toMatch(/#c4b5fd|#8b5cf6|#3b82f6/i);
});

// Redesign: the two big Agents/Terminals stat cards wasted the top of a menu-bar popover. Their numbers live in the
// one-line header summary now (`headerSummary`), and the host version is a small footer.
test("no big stat cards: the header carries the counts, the host version is a footer", () => {
	expect(page).not.toMatch(/tile-stat|class="tiles"|class="big"/);
	expect(page).toMatch(/headerSummary\(/);
	expect(page).toMatch(/<footer>[\s\S]*host \{snap\.host\.version\}/);
});

// Redesign: stopped projects (27 in the screenshot) are one disclosure, not 27 rows; a search opens it.
test("stopped projects sit behind a disclosure that reports how many rows are inside", () => {
	expect(page).toMatch(/aria-expanded=\{stoppedShown\}/);
	expect(page).toMatch(/Stopped \(\{lists\.stoppedCount\}\)/);
});

// Redesign: attention first. Agents that need you come before everything else in the scroll region.
test("agents needing you are the first section of the list", () => {
	const at = (s) => page.indexOf(s);
	expect(at('aria-label="Needs you"')).toBeGreaterThan(0);
	expect(at('aria-label="Needs you"')).toBeLessThan(at('aria-label="Working"'));
	expect(at('aria-label="Working"')).toBeLessThan(at('aria-label="Running stacks"'));
	expect(at('aria-label="Running stacks"')).toBeLessThan(at('aria-label="Projects"'));
});

// Incident: the list was cut off at the bottom with no scroll. The list is its own scroll region between a pinned
// header and footer, so a tall list scrolls inside the window instead of overflowing it.
test("the project list is a scroll region of its own, header and footer stay pinned", () => {
	expect(page).toMatch(/\.scroll\s*\{[^}]*overflow-y:\s*auto/);
	expect(page).toMatch(/\.scroll\s*\{[^}]*min-height:\s*0/);
	expect(page).toMatch(/\.pop\s*\{[^}]*overflow:\s*hidden/);
});

// Redesign review: a clone of a repo showed as a second bare "atlas" row because the group's disambiguated name
// (`repoLabel`, tray.ts) was computed but the row printed `p.name`.
test("a repo row prints the group's disambiguated name, not the bare project name", () => {
	expect(page).toMatch(/label \?\? title/);
	expect(page).not.toMatch(/label \?\? p\.name/);
	expect(page).toMatch(/project\(g\.repo, null, false, g\.repoLabel\)/);
});
