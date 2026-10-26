// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const styles = new URL("../../node_modules/@codeatlasdev/ui/dist/styles/", import.meta.url);
const read = (f: string) => readFileSync(new URL(f, styles), "utf8");
const strip = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, "");
const css = strip(readFileSync(new URL("../app.css", import.meta.url), "utf8"));

// Incident (atlas#116, #117; ui#549): the project avatars (SpaceDots, `<button role="tab">` tiles) showed the arrow on hover.
// `html, body { cursor: default }` is inherited, and kit 4.5.0's native-feel base (`html[data-ca-native] :is(button,
// [role=tab], …) { cursor: default }`, @layer ca.base, a LATER layer than ca.reset) reset every control to it, so Atlas kept
// a shared `:where(button, [role=tab], …) { cursor: pointer }` in app.css. Kit 4.6.0 gives every interactive shape the hand in
// `@layer ca.reset` and its native-feel block no longer sets `cursor`, so the app rule is gone. Real computed cursor
// (Chromium, `data-ca-native` on, no app rule): tile `pointer`, disabled tab `not-allowed`; the same probe on 4.5.0: `default`.
test("kit reset gives role=tab and button the hand, and a disabled one not-allowed", () => {
	const reset = strip(read("reset.css"));
	const rule = reset.match(/:is\(([^)]*)\)\s*\{\s*cursor:\s*pointer\s*\}/)?.[1] ?? "";
	for (const needle of ["button", "[role=tab]", "[role=button]", "[role=option]"])
		expect(rule).toContain(needle);
	expect(reset).toMatch(/:disabled[^{]*\{\s*cursor:\s*not-allowed/);
});

test("kit native-feel base (ca.base, later than ca.reset) no longer resets controls to the arrow", () => {
	expect(strip(read("base.css"))).not.toMatch(/cursor\s*:/);
});

test("app.css does not carry its own pointer rule for controls (the kit owns it)", () => {
	expect(css).not.toMatch(/:where\([^{}]*(button|\[role=)[^{}]*\)\s*\{[^{}]*cursor/);
});
