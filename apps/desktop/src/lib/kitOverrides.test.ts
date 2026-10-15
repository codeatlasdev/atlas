// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

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
