// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

// Regression: 1.5.0-alpha.26, a healthy service's ":4100" was red. The port link took the project's identity color
// (--space-accent-text), and orbit-shop's identity is red, so a running api looked like it was failing. On a service
// card only the state line may carry a status color; the link must not follow a user-chosen hue.
function ruleOf(css: string, selector: string) {
	return css.match(new RegExp(`(?:^|\\n)\\s*${selector.replace(".", "\\.")}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
}

test("a service card's port link does not take the project's identity color", () => {
	const src = readFileSync("src/lib/panel/CardHead.svelte", "utf8");
	const css = src.slice(src.indexOf("<style>"));
	const rule = ruleOf(css, ".port");
	const color = rule.match(/(?:^|\n)\s*color:\s*([^;]+);/)?.[1] ?? "";
	expect(color).not.toBe("");
	expect(color).not.toContain("--space-accent");
});
