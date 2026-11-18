// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { renderMarkdown } from "./markdown.ts";

/** Every CSS rule text in the document's <style> elements. */
const css = () => [...document.querySelectorAll("style")].map((s) => s.textContent).join("\n");

// Regression: the highlight classes of a fenced block (`ͼ…`) only have CSS once an editor mounts the HighlightStyle's
// StyleModule; the crew chat renders code without any editor, so its fences showed in one colour until a file opened.
test("a highlighted fence brings the CSS of the classes it uses", () => {
	const html = renderMarkdown("```rust\nfn main() { let x = 1; }\n```", "");
	const classes = [...html.matchAll(/<span class="([^"]+)">/g)].map((m) => m[1]);
	expect(classes.length).toBeGreaterThan(0);
	for (const c of new Set(classes.flatMap((c) => c.split(" ")))) expect(css()).toContain(`.${c}`);
});

test("the highlight CSS is added once however many blocks render", () => {
	renderMarkdown("```ts\nconst a = 1\n```", "");
	const before = document.querySelectorAll("style").length;
	renderMarkdown("```ts\nconst b = 2\n```\n\n```rust\nfn f() {}\n```", "");
	expect(document.querySelectorAll("style").length).toBe(before);
});

test("a fence with no known language is escaped text and needs no style", () => {
	expect(renderMarkdown("```nope\n<b>x</b>\n```", "")).toContain("&lt;b&gt;x&lt;/b&gt;");
});
