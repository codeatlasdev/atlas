// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const read = (rel) => readFileSync(join(import.meta.dir, rel), "utf8");
const page = read("+page.svelte");
const record = read("../../../lib/shots/record.ts");
const rustRecord = readFileSync(
	join(import.meta.dir, "../../../../../../crates/atlas-desktop/src/shots/record.rs"),
	"utf8",
);

// The commands and events of `local://shots-contract.md` for the recording HUD (authoritative).
const CONTRACT_COMMANDS = [
	"shots_record_pause",
	"shots_record_resume",
	"shots_record_stop",
	"shots_record_cancel",
	"shots_record_state",
	// Done affordance: the History entry actions of the settings page.
	"shots_history_open",
	// Trim step: save the clip (whole or a range) or throw it away.
	"shots_record_save",
	"shots_record_discard",
];
const CONTRACT_EVENTS = ["shots://record"];

const invoked = (src) => [...src.matchAll(/invoke(?:<[^>]*>)?\(\s*"([a-z_]+)"/g)].map((m) => m[1]);

test("every command the HUD invokes is in the contract, and every recording command is used", () => {
	const names = new Set(invoked(page));
	for (const n of names) expect(CONTRACT_COMMANDS, `${n} is not in the contract`).toContain(n);
	for (const n of [
		"shots_record_pause",
		"shots_record_resume",
		"shots_record_stop",
		"shots_record_cancel",
		"shots_record_state",
		"shots_record_save",
		"shots_record_discard",
	])
		expect(names.has(n), `${n} is never invoked`).toBe(true);
});

test("the HUD listens to shots://record and to nothing else", () => {
	const listened = [...page.matchAll(/listen(?:<[^>]*>)?\(\s*"([^"]+)"/g)].map((m) => m[1]);
	expect(listened).toEqual(CONTRACT_EVENTS);
});

test("the HUD calls no shots command outside the contract (also via helper strings)", () => {
	const all = [...page.matchAll(/"(shots_[a-z_]+)"/g)].map((m) => m[1]);
	for (const n of all) expect(CONTRACT_COMMANDS, n).toContain(n);
});

test("no element of the HUD carries the kit's `dark` class", () => {
	expect(page).not.toMatch(/class:dark|class="[^"]*\bdark\b[^"]*"/);
});

test("the HUD imports no workspace store", () => {
	expect(page).not.toMatch(/stores\/(workspace|projects|agents)\.svelte/);
});

test("transparent window background like the dock", () => {
	expect(page).toMatch(/:global\(html\),\s*:global\(body\)\s*\{\s*background: transparent !important;/);
});

test("the page takes its logic from lib/shots/record.ts and shows backend errors through errText", () => {
	expect(page).toMatch(/from "#lib\/shots\/record\.ts"/);
	expect(page).toContain("errText");
	expect(page).toContain("keyAction");
	expect(page).toContain("view(");
});

test("the red dot pulses via the `pulse` flag only (no unconditional animation on the dot)", () => {
	expect(page).toMatch(/data-pulse=\{/);
	expect(page).toMatch(/\.dot\[data-pulse="true"\]\s*\{[^}]*animation:/);
	expect(page).not.toMatch(/\.dot\s*\{[^}]*animation:/);
	expect(page).toContain("prefers-reduced-motion");
});

test("buttons have accessible names and the countdown is announced politely", () => {
	for (const name of ["Pause recording", "Resume recording", "Stop recording", "Cancel recording"])
		expect(page).toContain(`aria-label="${name}"`);
	expect(page).toContain('aria-live="polite"');
});

test("the pure module has no DOM, Tauri or store imports", () => {
	expect(record).not.toMatch(/from "@tauri-apps|from "#lib\/stores|document\.|window\./);
});

// Incident class: a field the page reads that Rust spells differently shows nothing and throws nothing.
// The extras `HudState` reads must be the keys Rust's `RecordState` serializes (camelCase of its fields).
test("the extra HUD state keys exist in Rust's RecordState (estimate, notes, progress, trim)", () => {
	const snake = (k) => k.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
	for (const k of ["estimateBytes", "notes", "progress", "trim"]) {
		expect(record, `${k} missing from HudState`).toContain(k);
		expect(rustRecord, `${k} missing from RecordState in record.rs`).toMatch(new RegExp(`pub ${snake(k)}: `));
	}
	for (const k of ["secs", "posters"]) expect(rustRecord).toMatch(new RegExp(`pub ${k}: `));
	expect(rustRecord).toMatch(
		/pub struct RecordState[\s\S]*?#\[serde\(rename_all = "camelCase"\)\]|rename_all = "camelCase"\)\]\npub struct RecordState/,
	);
});

test("Esc cancels the GIF encode through shots_record_cancel (the page passes whether an encode runs)", () => {
	expect(page).toMatch(/encoding:\s*v\.encode !== null/);
	expect(page).toContain('role="progressbar"');
});

test("the trim handles are keyboard operable sliders with names and values", () => {
	expect(page).toContain('aria-label="Trim start"');
	expect(page).toContain('aria-label="Trim end"');
	expect(page.match(/role="slider"/g)?.length).toBe(2);
	expect(page).toMatch(/aria-valuenow=/);
	expect(page).toContain("ArrowLeft");
});

test("Save sends the range through saveArgs, so keeping everything sends nothing", () => {
	expect(page).toMatch(/invoke\("shots_record_save",\s*saveArgs\(/);
});
