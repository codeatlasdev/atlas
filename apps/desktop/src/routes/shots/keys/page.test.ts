// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const page = readFileSync(join(import.meta.dir, "+page.svelte"), "utf8");
const keysRs = readFileSync(
	join(import.meta.dir, "../../../../../../crates/atlas-desktop/src/shots/keys_mac.rs"),
	"utf8",
);

/** Rust emits `shots://keys` with a plain `Vec<String>`; the page must listen to exactly that event. */
test("the pill listens to the event Rust emits, and to nothing else", () => {
	const listened = [...page.matchAll(/listen(?:<[^>]*>)?\(\s*"([^"]+)"/g)].map((m) => m[1]);
	expect(listened).toEqual(["shots://keys"]);
	expect(keysRs).toContain('"shots://keys"');
});

/** Privacy: the page draws what it is told; it must not read the keyboard, store keys or call a command. */
test("the pill reads no key, stores nothing and invokes no command", () => {
	expect(page).not.toMatch(/keydown|keyup|keypress|addEventListener|localStorage|sessionStorage|invoke\(/);
});

test("the window never takes the mouse: the page is pointer-events none over a transparent body", () => {
	expect(page).toMatch(/pointer-events:\s*none/);
	expect(page).toMatch(/:global\(html\),\s*:global\(body\)\s*\{\s*background: transparent !important;/);
});

test("a pill shows only while there are keys, and is hidden from assistive tech (it is part of the video)", () => {
	expect(page).toMatch(/\{#if keys\.length > 0\}/);
	expect(page).toContain('aria-hidden="true"');
});

test("a malformed payload clears the pill instead of throwing", () => {
	expect(page).toMatch(/Array\.isArray\(e\.payload\)/);
});
