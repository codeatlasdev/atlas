// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const read = (rel) => readFileSync(join(import.meta.dir, rel), "utf8");
const page = read("+page.svelte");
const model = read("../../../lib/shots/pin.ts");
const rustPin = readFileSync(
	join(import.meta.dir, "../../../../../../crates/atlas-desktop/src/shots/pin.rs"),
	"utf8",
);

// The commands the pin page may use: every one is a pin-only command in `shots/pin.rs`.
const COMMANDS = ["shots_pin_load", "shots_pin_resize", "shots_pin_keys", "shots_pin_close"];
const invoked = (src) => [...src.matchAll(/invoke(?:<[^>]*>)?\(\s*"([a-z_]+)"/g)].map((m) => m[1]);

test("every command the pin invokes is a pin command, and every pin command is used", () => {
	const names = new Set(invoked(page));
	for (const n of names) expect(COMMANDS, `${n} is not a pin command`).toContain(n);
	for (const n of COMMANDS) expect(names.has(n), `${n} is never invoked`).toBe(true);
});

test("each of those commands exists in pin.rs with the same name", () => {
	for (const n of COMMANDS) expect(rustPin, n).toContain(`pub fn ${n}(`);
});

test("the pin calls no shots command outside its own set (also via helper strings)", () => {
	for (const m of page.matchAll(/"(shots_[a-z_]+)"/g)) expect(COMMANDS, m[1]).toContain(m[1]);
});

// The window is built with `?path=<capture>&n=<n>` (`pin::query`): the page reads `path` and asks Rust for the image.
test("it loads the picture named by ?path= through shots_pin_load", () => {
	expect(page).toMatch(/URLSearchParams\(location\.search\)/);
	expect(page).toMatch(/q\.get\(\s*"path"\s*\)/);
	expect(page).toMatch(/shots_pin_load",\s*\{\s*path/);
	expect(rustPin).toContain('format!("path={enc}&n={n}")');
});

test("the pin's window size comes from Rust: the page only asks and reads the applied size", () => {
	expect(page).toMatch(/shots_pin_resize",\s*\{\s*width:\s*e\.size\.w,\s*height:\s*e\.size\.h/);
	expect(page).toContain('type: "applied"');
});

test("wheel and pinch are handled with a non-passive listener that prevents the page from scrolling or zooming", () => {
	expect(page).toMatch(/addEventListener\("wheel",\s*onWheel,\s*\{\s*passive:\s*false\s*\}\)/);
	expect(page).toMatch(/preventDefault\(\)/);
	expect(page).toContain("alt: ev.altKey");
	expect(page).toContain("pinch: ev.ctrlKey");
});

test("keys go through the pure keyEvent (+ - [ ] Esc), never a hand-rolled switch in the page", () => {
	expect(page).toMatch(/keyEvent\(ev\)/);
	expect(page).not.toMatch(/case "Escape"|ev\.key ===/);
	for (const k of ['"+"', '"-"', '"["', '"]"', '"Escape"']) expect(model, k).toContain(k);
});

test("dragging the body moves the window; the close button does not start a drag", () => {
	expect(page).toContain("startDragging()");
	expect(page).toMatch(/closest\("button"\)/);
});

test("a double click resets the zoom and does not start a drag", () => {
	expect(page).toMatch(/ondblclick=\{\(\) => dispatch\(\{ type: "reset" \}\)\}/);
	expect(page).toMatch(/ev\.detail >= 2/);
});

// A pin is a non-activating panel: without key status it never sees a key. The page asks on click, gives it up on blur.
test("it takes the keyboard on press and hands it back on blur", () => {
	expect(page).toMatch(/shots_pin_keys",\s*\{\s*on:\s*true/);
	expect(page).toMatch(/shots_pin_keys",\s*\{\s*on:\s*false/);
	expect(page).toMatch(/addEventListener\("blur"/);
});

test("the opacity is applied to the whole pin and shown with the zoom for a moment after a change", () => {
	expect(page).toMatch(/style:opacity=\{st\.opacity\}/);
	expect(page).toContain("percent(st.zoom)");
	expect(page).toContain("percent(st.opacity)");
});

test("the close button is hidden until hover or keyboard focus and has an accessible name", () => {
	expect(page).toContain('aria-label="Close pin"');
	expect(page).toMatch(/\.pin:hover \.x,\s*\.x:focus-visible\s*\{\s*opacity:\s*1/);
	expect(page).toContain(":focus-visible");
});

test("backend errors are shown through errText and announced", () => {
	expect(page).toContain("errText");
	expect(page).toContain('role="alert"');
});

test("transparent window background like the other capture pages", () => {
	expect(page).toMatch(/:global\(html\),\s*:global\(body\)\s*\{\s*background: transparent !important;/);
});

test("the pin imports no workspace store and no element carries the kit's `dark` class", () => {
	expect(page).not.toMatch(/stores\/(workspace|projects|agents)\.svelte/);
	expect(page).not.toMatch(/class:dark|class="[^"]*\bdark\b[^"]*"/);
});

test("the pure model has no DOM, Tauri or store imports", () => {
	const code = model.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
	expect(code).not.toMatch(/from "@tauri-apps|from "#lib\/stores|\bdocument\.|\bwindow\./);
});

test("the page takes its logic from lib/shots/pin.ts", () => {
	expect(page).toMatch(/from "#lib\/shots\/pin\.ts"/);
	expect(page).toContain("reduce(");
});
