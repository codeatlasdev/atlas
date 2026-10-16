// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const css = readFileSync(new URL("../app.css", import.meta.url), "utf8");

// Incident (kit 0.7.1 -> 3.x): the kit's component CSS carried a Svelte hash class, so app.css doubled the selector
// (`.ca-btn.ca-btn:disabled`, `.ca-tf__wrap.ca-tf__wrap`, `.glass-overlay.glass-overlay`, …) to win. Since 2.2.0 it lives
// in `@layer ca.components`, which any unlayered rule beats at one class, and the colours come from tokens the theme sets
// (theme.test.ts, "kit tokens"). A doubled kit selector is a patch the kit no longer needs: it must not come back.
test("app.css does not double a kit selector to out-rank it", () => {
	const doubled = css.match(/(\.[a-z][\w-]*)\1(?![\w-])/g) ?? [];
	expect(doubled).toEqual([]);
});

// Incident: `.ca-dialog .ca-dialog__body { flex: 0 1 auto }` fixed a Dialog body collapsing to 0 in WKWebView. The kit
// ships that exact rule since 1.0.0 (dialog.css), so the copy is dead weight that can only drift.
test("app.css does not restyle the kit's dialog body", () => {
	expect(css).not.toMatch(/\.ca-dialog__body/);
});

// The kit's disabled look is `--ca-text-disabled` on `--ca-action-disabled-bg`; the theme sets both, so no rule here.
test("app.css has no per-component disabled rule for the kit's buttons", () => {
	expect(css).not.toMatch(/\.ca-btn[^{]*:disabled/);
	expect(css).not.toMatch(/\.ca-btn[^{]*aria-disabled/);
});

// `nativeFeel()` (kit) sets `html[data-ca-native]`, and base.css then owns: no outline without `data-kbd`, `cursor: default`
// on chrome, no link/image drag, no overscroll. app.css kept its own copies from before the kit had them; only the
// keyboard ring's look (Atlas's offset) is ours.
test("app.css leaves the native-feel rules the kit now owns to the kit", () => {
	expect(css).not.toMatch(/html:not\(\[data-kbd\]\)/);
	expect(css).not.toMatch(/-webkit-user-drag/);
	expect(css).not.toMatch(/^button,\s*\n\[role="button"\],/m);
	expect(css).not.toMatch(/overscroll-behavior/);
});

test("the keyboard ring is the one rule app.css still owns", () => {
	expect(css).toMatch(/html\[data-kbd\] :focus-visible\s*\{[^}]*outline:\s*2px solid var\(--ca-focus-ring\)/);
});

// Incident (ui#136, fixed in kit 4.0.0): `nativeFeel()` filtered the context menu by `.selectable` while its CSS opted
// text selection in with `.ca-selectable`, so Atlas kept its own `.selectable` rule. The kit now has one opt-out class,
// `.ca-selectable` (menu + selection); the local class and its rule must not come back.
test("selectable text opts in with the kit's .ca-selectable, not a local .selectable", () => {
	expect(css).not.toMatch(/(^|[^\w-])\.selectable\b/m);
	const stale: string[] = [];
	const walk = (dir: string) => {
		for (const name of readdirSync(dir)) {
			const path = join(dir, name);
			if (statSync(path).isDirectory()) walk(path);
			else if (
				/\.svelte$/.test(name) &&
				/class="[^"]*(^|[\s"])selectable[\s"]/.test(readFileSync(path, "utf8"))
			)
				stale.push(path);
		}
	};
	walk(new URL("..", import.meta.url).pathname);
	expect(stale).toEqual([]);
});

// Incident (ui#135, ui#137, fixed in kit 4.0.0): the scrim blur and the tooltip blur had no token of their own, so Atlas
// zeroed the shared `--ca-blur-ultra-thin` (also read by Tabs, Pagination, `.glass-ultra-thin`, uploads) to un-blur them.
// The window is transparent and these surfaces float over moving content: a backdrop blur only costs frames.
test("app.css turns the scrim and tooltip blur off through their own kit tokens, not the shared ultra-thin one", () => {
	expect(css).toMatch(/--ca-scrim-blur:\s*none/);
	expect(css).toMatch(/--ca-tooltip-blur:\s*none/);
	expect(css).not.toMatch(/--ca-blur-ultra-thin\s*:/);
});
