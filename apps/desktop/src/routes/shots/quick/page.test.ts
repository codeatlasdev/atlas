// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const read = (rel) => readFileSync(join(import.meta.dir, rel), "utf8");
const page = read("+page.svelte");
const quick = read("../../../lib/shots/quick.ts");
const rustQuick = readFileSync(
	join(import.meta.dir, "../../../../../../crates/atlas-desktop/src/shots/quick.rs"),
	"utf8",
);
const rustGif = readFileSync(
	join(import.meta.dir, "../../../../../../crates/atlas-desktop/src/shots/gif.rs"),
	"utf8",
);

// The commands and events the quick-access stack may use (authoritative): the stack and the actions of `quick.rs`
// and the GIF sheet. The Pin button is a quick action (`pin`, handled by `pin::open`): no pin command is called here.
const CONTRACT_COMMANDS = [
	"shots_quick_stack",
	"shots_quick_action",
	"shots_quick_text",
	"shots_quick_hover",
	"shots_quick_drag",
	"shots_gif_presets",
	"shots_gif_estimate",
	"shots_gif_export",
	"shots_gif_cancel",
];
const CONTRACT_EVENTS = ["shots://quick-stack", "shots://gif-progress"];

const invoked = (src) => [...src.matchAll(/invoke(?:<[^>]*>)?\(\s*"([a-z_]+)"/g)].map((m) => m[1]);

test("every command the stack invokes is in the contract, and every one in the contract is used", () => {
	const names = new Set(invoked(page));
	for (const n of names) expect(CONTRACT_COMMANDS, `${n} is not in the contract`).toContain(n);
	for (const n of CONTRACT_COMMANDS) expect(names.has(n), `${n} is never invoked`).toBe(true);
});

// Esc during a GIF export stops the encode before the sheet goes away; the cancelled export rejects with Rust's
// exact sentence, which closes the sheet quietly instead of showing an error.
test("a GIF export is cancelled through shots_gif_cancel and its cancel rejection is not shown as an error", () => {
	expect(page).toContain('invoke("shots_gif_cancel")');
	expect(page).toContain("GIF_CANCELLED");
	expect(page).toMatch(/errText\(err\) === GIF_CANCELLED/);
	expect(page).toContain('type: "exportCancelled"');
	expect(page).toContain('type: "exportStart"');
	expect(rustGif).toContain("pub const CANCELLED:");
});

test("the stack calls no shots command outside the contract (also via helper strings)", () => {
	const all = [...page.matchAll(/"(shots_[a-z_]+)"/g)].map((m) => m[1]);
	for (const n of all) expect(CONTRACT_COMMANDS, n).toContain(n);
});

test("the stack listens to shots://quick-stack and shots://gif-progress and to nothing else", () => {
	const listened = [...page.matchAll(/listen(?:<[^>]*>)?\(\s*"([^"]+)"/g)].map((m) => m[1]).sort();
	expect(listened).toEqual([...CONTRACT_EVENTS].sort());
});

test("Rust emits the event the page listens to, and has the commands the page calls", () => {
	expect(rustQuick).toContain('"shots://quick-stack"');
	for (const n of ["shots_quick_stack", "shots_quick_action", "shots_quick_text", "shots_quick_drag"])
		expect(rustQuick, n).toMatch(new RegExp(`pub (async )?fn ${n}\\(`));
});

test("it loads the stack on mount (the first card is pushed before the page has loaded)", () => {
	expect(page).toMatch(/onMount\(/);
	expect(page).toContain('invoke<StackIn>("shots_quick_stack")');
});

// Incident class (tray page): the kit declares its whole token set on a bare `dark` class.
test("no element of the stack carries the kit's `dark` class", () => {
	expect(page).not.toMatch(/class:dark|class="[^"]*\bdark\b[^"]*"/);
});

test("the stack imports no workspace store", () => {
	expect(page).not.toMatch(/stores\/(workspace|projects|agents)\.svelte/);
});

test("transparent window background like the dock", () => {
	expect(page).toMatch(/:global\(html\),\s*:global\(body\)\s*\{\s*background: transparent !important;/);
});

test("the page takes its logic from lib/shots/quick.ts and shows backend errors through errText", () => {
	expect(page).toMatch(/from "#lib\/shots\/quick\.ts"/);
	expect(page).toContain("errText");
	expect(page).toContain("reduce(");
});

test("the pure module has no DOM, Tauri or store imports", () => {
	const code = quick.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
	expect(code).not.toMatch(/from "@tauri-apps|from "#lib\/stores|\bdocument\.|\bwindow\./);
});

test("cards are keyed by id and drawn in the order the pure layout gives (newest nearest the corner)", () => {
	expect(page).toMatch(/\{#each cards as c \(c\.id\)\}/);
	expect(page).toMatch(/const cards = \$derived\(layout\(st\)\)/);
});

test("every event the page sends for a card carries that card's id (timers, hover and confirmation are per card)", () => {
	expect(page).toMatch(/type: "hover", id: c\.id, on: true/);
	expect(page).toMatch(/type: "hover", id: c\.id, on: false/);
	expect(page).toMatch(/dispatch\(\{ type: "action", id, name \}\)/);
	expect(page).toMatch(/dispatch\(\{ type: "textAction", id, name \}\)/);
	// no per-card event is dispatched without an id (it would act on no card, or on the wrong one)
	const perCard = [
		...page.matchAll(/dispatch\(\{ type: "(action|hover|textAction|openGifSheet|closeSheet)"[^}]*\}\)/g),
	];
	expect(perCard.length).toBeGreaterThan(5);
	for (const m of perCard) expect(m[0], m[0]).toMatch(/\bid\b/);
});

test("dragging a thumbnail starts the native drag of THAT card's file and cancels the web one", () => {
	expect(page).toMatch(/ondragstart=/);
	expect(page).toContain('invoke("shots_quick_drag"');
	expect(page).toMatch(/preventDefault\(\)/);
	// The native drag needs the path of the dragged card's entry, not the thumbnail data URL.
	expect(page).toMatch(/shots_quick_drag",\s*\{\s*path:\s*c\.entry\.path/);
});

test("hover is reported to Rust on pointerenter and pointerleave", () => {
	expect(page).toMatch(/onpointerenter=/);
	expect(page).toMatch(/onpointerleave=/);
	expect(page).toContain('invoke("shots_quick_hover"');
});

test("the capture card's actions: Copy, Save…, Annotate, Pin, Text (OCR), Send to agent, Reveal, Discard, Make GIF", () => {
	for (const label of [
		"Copy",
		"Save…",
		"Annotate",
		"Pin",
		"Text",
		"Send to agent",
		"Reveal in Finder",
		"Discard",
		"Make GIF",
	])
		expect(page, label).toContain(label);
	for (const a of ['"annotate"', '"pin"', '"ocr"', '"send"', '"reveal"', '"discard"', '"copy"', '"save"'])
		expect(page, a).toContain(a);
	// annotate / pin / OCR only for screenshots; Make GIF only for recordings
	expect(page).toMatch(/c\.entry\.kind === "screenshot"/);
	expect(page).toMatch(/c\.entry\.kind === "recording"/);
	expect(page).toContain('aria-label="More actions"');
});

test("the action names the page sends are the ones Rust deserializes", () => {
	for (const a of ["copy", "save", "annotate", "pin", "ocr", "send", "reveal", "close", "discard"])
		expect(rustQuick, a).toMatch(new RegExp(`Action::${a[0].toUpperCase()}${a.slice(1)}`));
	expect(rustQuick).toContain('#[serde(rename_all = "lowercase")]');
	// the Pin button is the `pin` action: Rust opens the pin window from the card's (history-checked) path
	expect(rustQuick).toContain("Action::Pin => super::pin::open(");
});

test("the text card: preview, Copy again, Send to agent, 'No text found' for an empty read (nothing to copy)", () => {
	for (const s of ["Copy again", "Send to agent", "No text found", "c.preview"]) expect(page, s).toContain(s);
	// an empty read has no buttons
	expect(page).toMatch(/\{#if !c\.empty\}/);
	expect(page).toContain('"shots_quick_text"');
});

test("the discard button says it needs a second click while armed", () => {
	expect(page).toMatch(/confirmDiscard/);
	expect(page).toContain("Click again to delete");
});

test("the GIF sheet: presets radio group, estimate refetched on preset change, export with progress", () => {
	expect(page).toContain('type="radio"');
	expect(page).toMatch(/shots_gif_estimate",\s*\{\s*path/);
	expect(page).toMatch(/shots_gif_export",\s*\{\s*path/);
	expect(page).toContain("≈");
	expect(page).toContain("<progress");
});

test("Esc is handled (document in the header comment) and goes through the reducer", () => {
	expect(page).toMatch(/Escape/);
	expect(page).toContain('type: "escape"');
	expect(page).toMatch(/Esc/);
});

test("the thumbnail is contained over a checkerboard", () => {
	expect(page).toMatch(/object-fit:\s*contain/);
	expect(page).toMatch(/\.thumb[^{]*\{[^}]*(conic-gradient|repeating-conic-gradient|linear-gradient)/s);
});

// The window is `QUICK_CARD` × the number of cards (`surface.rs`): the CSS slot must be the same size or the stack
// leaves a gap / clips a card.
test("a card is 220px wide in a 252pt window and exactly one slot (270pt) high, as Rust sizes the window", () => {
	expect(page).toMatch(/width:\s*220px/);
	expect(page).toMatch(/flex:\s*0 0 270px;\s*height:\s*270px/);
	const surface = readFileSync(
		join(import.meta.dir, "../../../../../../crates/atlas-desktop/src/shots/surface.rs"),
		"utf8",
	);
	expect(surface).toContain("QUICK_CARD: (f64, f64) = (252.0, 270.0)");
	expect(surface).toContain("QUICK_GAP: f64 = 8.0");
	expect(page).toMatch(/gap:\s*8px/);
});

test("a stack taller than the display scrolls inside the window without a scrollbar", () => {
	expect(page).toMatch(/overflow-y:\s*auto/);
	expect(page).toMatch(/scrollbar-width:\s*none/);
});

test("buttons carry accessible names, errors are announced, keyboard focus stays visible", () => {
	expect(page).toContain('role="alert"');
	expect(page).toContain(":focus-visible");
	expect(page).not.toMatch(/outline:\s*none(?![^;]*focus-visible)/);
});

test("the dismiss timers are driven by one tick, not by a setTimeout per card", () => {
	expect(page).toMatch(/setInterval\(/);
	expect(page).toContain('type: "tick"');
	expect(page).not.toMatch(/setTimeout\(/);
});
