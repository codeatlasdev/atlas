// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("../app.css", import.meta.url), "utf8");
const kitBase = readFileSync(
	new URL("../../node_modules/@codeatlasdev/ui/dist/styles/base.css", import.meta.url),
	"utf8",
);

// Incident: the project avatars (the SpaceDots strip, `<button role="tab">` tiles with a tooltip) showed the arrow on
// hover although a click switches project. Cause: `html, body { cursor: default }` (app.css) is inherited by every
// element, and the kit's native-feel base (`html[data-ca-native] :is(button, [role=tab], …) { cursor: default }`,
// @layer ca.base) resets buttons too, so a custom-styled control only got a hand if it wrote `cursor: pointer` itself
// and most did not. The kit's layer loses to any unlayered rule, so ONE shared rule in app.css gives every enabled
// control the hand; a per-component `cursor` is only for the exceptions.
test("the kit still resets controls to the arrow (the reason the shared rule exists)", () => {
	expect(kitBase).toMatch(/html\[data-ca-native\] :is\([^)]*button[^)]*\)\s*\{[^}]*cursor:\s*default/);
});

/** The rule of app.css that sets `cursor: pointer` for buttons. */
const pointerRule = css.match(/:where\([^{}]*\)\s*\{[^{}]*cursor:\s*pointer[^{}]*\}/)?.[0] ?? "";

test("app.css gives every enabled control a pointer in one shared rule", () => {
	expect(pointerRule).not.toBe("");
	const selector = pointerRule.slice(0, pointerRule.lastIndexOf("{"));
	for (const needle of ["button", '[role="tab"]', '[role="button"]', '[role="menuitem"]', '[role="option"]'])
		expect(selector).toContain(needle);
	// A disabled control keeps the arrow: nothing happens on click.
	expect(selector).toMatch(/button:not\([^)]*:disabled/);
	expect(selector).toContain('[role="tab"]:not([aria-disabled="true"])');
});

// Zero specificity is the contract: components that set their own `cursor` (text fields, resize handles, a grab
// handle, a `default` for a read-only row) must keep winning with a single class.
test("the shared rule has zero specificity (everything is inside one :where)", () => {
	expect(pointerRule.startsWith(":where(")).toBe(true);
	const close = pointerRule.lastIndexOf(")", pointerRule.lastIndexOf("{"));
	expect(pointerRule.slice(close + 1, pointerRule.lastIndexOf("{")).trim()).toBe("");
});

test("the shared rule is top level in app.css (unlayered, so it outranks the kit's @layer ca.base reset)", () => {
	const before = css.slice(0, css.indexOf(pointerRule));
	const depth = [...before.replace(/\/\*[\s\S]*?\*\//g, "")].reduce(
		(d, c) => d + (c === "{" ? 1 : c === "}" ? -1 : 0),
		0,
	);
	expect(depth).toBe(0);
	expect(css.replace(/\/\*[\s\S]*?\*\//g, "")).not.toMatch(/@layer/);
});
