// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const read = (rel) => readFileSync(join(import.meta.dir, rel), "utf8");
const page = read("+page.svelte");
const layout = read("../+layout.svelte");
const invoked = (src) => [...src.matchAll(/invoke(?:<[^>]*>)?\(\s*"([a-z_]+)"/g)].map((m) => m[1]);

// The overlay's commands (`local://shots-contract.md`, "Overlay page"). A page that calls a name outside this list
// calls a command Rust does not implement and fails only in a window nobody has open. `shots_overlay_previous` is the
// HUD's "Previous area": the Rust command only feeds `Hotkey::Previous`.
const OVERLAY_COMMANDS = [
	"shots_overlay_init",
	"shots_overlay_commit",
	"shots_overlay_cancel",
	"shots_overlay_loupe",
	"shots_overlay_previous",
	"shots_timer_cycle",
];

test("the overlay page calls every overlay command and nothing but them", () => {
	const names = new Set(invoked(page));
	for (const n of OVERLAY_COMMANDS) expect(names.has(n), `${n} is not called`).toBe(true);
	for (const n of names) expect(OVERLAY_COMMANDS, `${n} is not in the contract`).toContain(n);
});

test("the page is registered under no other IPC than the contract (every shots_ name is a contract name)", () => {
	for (const m of page.matchAll(/["'`](shots_[a-z_]+)["'`]/g)) expect(OVERLAY_COMMANDS).toContain(m[1]);
});

// `shots_overlay_commit(commit: serde_json::Value)` names its argument: a bare payload would arrive as a missing `commit`.
test("the commit is sent under the `commit` argument Rust names", () => {
	expect(page).toMatch(/invoke\("shots_overlay_commit",\s*\{\s*commit:\s*e\.payload\s*\}\)/);
});

test("the HUD effect `previous` calls shots_overlay_previous, and plain cancel calls shots_overlay_cancel", () => {
	expect(page).toMatch(/e\.type === "previous"[\s\S]{0,60}shots_overlay_previous/);
	expect(page).toContain('invoke("shots_overlay_cancel")');
});

// Incident class (tray page): the kit declares its whole token set on a bare `dark` class.
test("no element carries the kit's `dark` class", () => {
	expect(page).not.toMatch(/class:dark|class="[^"]*\bdark\b[^"]*"/);
	expect(layout).not.toMatch(/class:dark|class="[^"]*\bdark\b[^"]*"/);
});

test("no workspace store is imported", () => {
	expect(page).not.toMatch(/stores\/(workspace|projects|agents)\.svelte/);
	expect(layout).not.toMatch(/stores\/(workspace|projects|agents)\.svelte/);
});

test("the window is transparent and the cursor is a crosshair on the whole page", () => {
	expect(page).toMatch(/:global\(html\),\s*:global\(body\)\s*{[^}]*background:\s*transparent\s*!important/);
	expect(page).toMatch(/cursor:\s*crosshair/);
});

test("the loupe is pixelated, 8x, and hides when the command fails", () => {
	expect(page).toContain("image-rendering: pixelated");
	expect(page).toMatch(/LOUPE_ZOOM\s*=\s*8/);
	expect(page).toMatch(/shots_overlay_loupe[\s\S]*catch/);
});

// `shots_overlay_loupe` answers `{ url, hex } | null` (`shots::Loupe`): the colour comes from Rust, so the page decodes
// no image and reads no canvas back on every pointer move.
test("the loupe reads url and hex from Rust and decodes nothing itself", () => {
	expect(page).toMatch(/invoke<\{ url: string; hex: string \} \| null>\("shots_overlay_loupe"/);
	expect(page).not.toMatch(/getImageData|createElement\("canvas"\)|new Image\(\)|img\.decode|colourOf/);
	expect(page).not.toContain("hexOf");
});

test("the page is driven by the pure reducer in lib/shots/overlay.ts", () => {
	expect(page).toMatch(/from "#lib\/shots\/overlay\.ts"/);
	expect(page).toContain("reduce(");
});

test("the shared layout starts the error handler for `shots` and imports the kit css", () => {
	expect(layout).toContain('startErrorHandler("shots")');
	expect(layout).toContain("@codeatlasdev/ui/styles/global.css");
	expect(layout).toContain("../../app.css");
});

test("the page paints before init resolves: the dim mask does not depend on the display", () => {
	// the mask is rendered unconditionally; only the extras are guarded by the init-filled state
	expect(page).toMatch(/class="dim"/);
});

test("the toolbar has a named button per HUD mode, a previous-area button and a timer readout", () => {
	expect(page).toContain('role="toolbar"');
	expect(page).toMatch(/aria-label=\{MODE_LABELS\[m\]\}/);
	for (const name of ["Region", "Window", "Screen", "Record", "GIF", "Text (OCR)"]) {
		expect(page, name).toContain(`"${name}"`);
	}
	expect(page).toContain('aria-label="Previous area"');
	expect(page).toMatch(/disabled=\{!st\.hud\.hasPrevious\}/);
	expect(page).toMatch(/Timer:/);
	expect(page).toMatch(/st\.hud\.timerSecs/);
});

// The timer is a saved setting that must show on every display's overlay: the click asks Rust, Rust announces the
// new value, and the page applies it from the event (not from the click), so two screens never disagree.
test("the timer button asks Rust and the page applies the announced value to the reducer", () => {
	expect(page).toMatch(/class="timer"[\s\S]{0,200}onclick=\{cycleTimer\}/);
	expect(page).toMatch(/invoke<number>\("shots_timer_cycle"\)/);
	expect(page).toMatch(
		/listen<number>\("shots:\/\/hud-timer"[\s\S]{0,80}dispatch\(\{ type: "timer", secs: ev\.payload \}\)/,
	);
});

test("toolbar buttons forward to the reducer instead of deciding anything", () => {
	expect(page).toMatch(/dispatch\(\{ type: "mode", mode: m \}\)/);
	expect(page).toMatch(/dispatch\(\{ type: "previous" \}\)/);
});

test("the toolbar only renders in the HUD (mode all), and its pointer events do not reach the selection", () => {
	expect(page).toMatch(/\{#if st\.hud\}/);
	expect(page).toMatch(/closest\?\.\("\.hud"\)/);
});

test("the keys the reducer owns are forwarded with their modifiers: Return, Tab, 1..6, arrows with shift/alt", () => {
	for (const k of [
		'"Enter"',
		'"Tab"',
		'"ArrowLeft"',
		'"ArrowRight"',
		'"ArrowUp"',
		'"ArrowDown"',
		'"Escape"',
	]) {
		expect(page, k).toContain(k);
	}
	expect(page).toMatch(/type: "key", key: e\.key, up, shift: e\.shiftKey, alt: e\.altKey/);
	expect(page).toMatch(/HUD_MODES\.length/);
});

test("pointer presses carry the click count (double click commits) and arrows may repeat", () => {
	expect(page).toMatch(/clicks:\s*e\.detail/);
	expect(page).toMatch(/startsWith\("Arrow"\)/);
});

test("the held selection shows the eight handles and names itself with its pixel size", () => {
	expect(page).toContain("HANDLES");
	expect(page).toMatch(/data-handle=\{h\}/);
	expect(page).toMatch(/aria-label=\{st\.held \? selectionAria\(st\.held, scale\) : undefined\}/);
	// the readout is the scale-aware pixel size plus the position
	expect(page).toMatch(/sizeLabel\(rect, scale\)/);
	expect(page).toMatch(/positionLabel\(rect, scale\)/);
});

test("the loupe draws a 1px pixel grid, outlines the centre pixel and shows its hex colour", () => {
	expect(page).toMatch(/class="grid"/);
	expect(page).toMatch(/linear-gradient\(to right, rgb\([^)]*\) 1px, transparent 1px\)/);
	expect(page).toMatch(/linear-gradient\(to bottom, rgb\([^)]*\) 1px, transparent 1px\)/);
	expect(page).toMatch(/background-size:\s*var\(--cell\)\s*var\(--cell\)/);
	expect(page).toMatch(/class="reticle"/);
	// the colour comes with the block from Rust (`shots::Loupe.hex`)
	expect(page).toMatch(/centre = l\?\.hex/);
	expect(page).toMatch(/class="hex"/);
});

test("Space still flips region/window in a plain overlay but never inside the HUD", () => {
	expect(page).toMatch(/!st\.drag && !st\.hud/);
});
