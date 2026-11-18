// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { COLORS, WIDTHS, TEXT_SIZES } from "../../../lib/shots/annotate.ts";
import { BEAUTIFY_GRADIENTS, BEAUTIFY_PADDINGS, BEAUTIFY_RADII } from "../../../lib/shots/render.ts";
import { m } from "#lib/i18n/m.ts";

const read = (rel) => readFileSync(join(import.meta.dir, rel), "utf8");
const page = read("+page.svelte");
const annotate = read("../../../lib/shots/annotate.ts");
const render = read("../../../lib/shots/render.ts");

/** The page shows `m.<key>()` and the catalog's English (the tests run in `en`) is what the contract names. */
const says = (key, english) => {
	expect(page, key).toContain(`m.${key}(`);
	expect(m[key](), key).toBe(english);
};
// The editor commands of `local://shots-contract.md` (authoritative).
const CONTRACT_COMMANDS = ["shots_editor_load", "shots_editor_save", "shots_editor_close"];

const invoked = (src) => [...src.matchAll(/invoke(?:<[^>]*>)?\(\s*"([a-z_]+)"/g)].map((m) => m[1]);

test("every command the editor invokes is in the contract, and load/save/close are all used", () => {
	const names = new Set(invoked(page));
	for (const n of names) expect(CONTRACT_COMMANDS, `${n} is not in the contract`).toContain(n);
	for (const n of CONTRACT_COMMANDS) expect(names.has(n), `${n} is never invoked`).toBe(true);
});

test("the editor calls no shots command outside the contract (also via helper strings)", () => {
	const all = [...page.matchAll(/"(shots_[a-z_]+)"/g)].map((m) => m[1]);
	for (const n of all) expect(CONTRACT_COMMANDS, n).toContain(n);
});

test("the page loads the file named by the ?path= query", () => {
	expect(page).toMatch(/searchParams\.get\(\s*"path"\s*\)|URLSearchParams\([^)]*\)\.get\(\s*"path"\s*\)/);
	expect(page).toMatch(/invoke(?:<[^>]*>)?\(\s*"shots_editor_load",\s*\{\s*path/);
});

// Incident class (tray page): the kit declares its whole token set on a bare `dark` class.
test("no element of the editor carries the kit's `dark` class", () => {
	expect(page).not.toMatch(/class:dark|class="[^"]*\bdark\b[^"]*"/);
});

test("the editor imports no workspace store", () => {
	expect(page).not.toMatch(/stores\/(workspace|projects|agents)\.svelte/);
});

test("transparent window background like the dock", () => {
	expect(page).toMatch(/:global\(html\),\s*:global\(body\)\s*\{\s*background: transparent !important;/);
});

test("backend errors are shown through errText, not swallowed", () => {
	expect(page).toContain("errText");
	expect(page).toMatch(/role="alert"/);
});

test("the pure modules have no DOM, Tauri or store imports", () => {
	for (const src of [annotate, render]) {
		expect(src).not.toMatch(/@tauri-apps|\$app\/|stores\/|svelte/);
		expect(src).not.toMatch(/\bdocument\.|\bwindow\./);
	}
});

test("the toolbar offers every tool, with names and the single-letter shortcuts V A R T H M N S B C", () => {
	for (const [key, label] of [
		["capture_editor_page_tool_select", "Select"],
		["capture_editor_page_tool_arrow", "Arrow"],
		["capture_editor_page_rectangle", "Rectangle"],
		["capture_editor_page_tool_text", "Text"],
		["capture_editor_page_tool_highlight", "Highlight"],
		["capture_editor_page_tool_highlighter", "Highlighter"],
		["capture_editor_page_tool_counter", "Counter"],
		["capture_editor_page_tool_spotlight", "Spotlight"],
		["capture_editor_page_blur", "Blur"],
		["capture_editor_page_tool_crop", "Crop"],
	]) {
		says(key, label);
	}
	const letters = {
		v: "select",
		a: "arrow",
		r: "rect",
		t: "text",
		h: "highlight",
		m: "marker",
		n: "counter",
		s: "spotlight",
		b: "blur",
		c: "crop",
	};
	for (const [k, tool] of Object.entries(letters)) {
		expect(page, `${k} -> ${tool}`).toMatch(new RegExp(`\\b${k}:\\s*"${tool}"`));
		// the same letter is shown in the tooltip of that tool's button
		expect(page, `${tool} entry`).toMatch(
			new RegExp(`tool:\\s*"${tool}",\\s*label:\\s*m\\.[a-z_]+\\(\\),\\s*key:\\s*"${k.toUpperCase()}"`),
		);
	}
	expect(page).toContain("m.capture_editor_page_tool_tip({ tool: t.label, key: t.key })");
	says("capture_editor_page_pixelate", "Pixelate");
});

test("every tool letter is unique, and no letter shadows a modifier shortcut", () => {
	const keys = [...page.matchAll(/key:\s*"([A-Z])"/g)].map((m) => m[1]);
	expect(new Set(keys).size).toBe(keys.length);
	// tool letters only fire without Cmd/Ctrl/Alt, so Cmd+S / Cmd+C / Cmd+Z never switch tool
	expect(page).toMatch(/!mod && !e\.altKey && KEYS\[e\.key\.toLowerCase\(\)\]/);
});

test("the palette has at least 8 colours (the first is red), a custom hex input and 4 stroke widths 2/4/6/10", () => {
	expect(COLORS.length).toBeGreaterThanOrEqual(8);
	expect(COLORS[0]).toBe("#ef4444");
	expect(page).toContain("COLORS");
	says("capture_editor_page_custom_color_hex", "Custom color, hex");
	expect(page).toContain("normalizeHex");
	expect(WIDTHS).toEqual([2, 4, 6, 10]);
	expect(page).toContain("WIDTHS");
	// colour / width apply to the selected item and to new ones
	expect(page).toMatch(/function pickColor[\s\S]{0,160}restyle\(doc, doc\.selected, \{ color/);
	expect(page).toMatch(/function pickWidth[\s\S]{0,160}restyle\(doc, doc\.selected, \{ width/);
	expect(page).toContain("styleOf(");
});

test("text controls: size S/M/L/XL, bold, background none/solid/pill, applied to the selected text too", () => {
	expect(TEXT_SIZES.map((t) => t.label)).toEqual(["S", "M", "L", "XL"]);
	expect(page).toContain("TEXT_SIZES");
	says("capture_editor_page_bold", "Bold");
	for (const [key, label] of [
		["capture_editor_page_text_bg_none", "None"],
		["capture_editor_page_text_bg_solid", "Solid"],
		["capture_editor_page_text_bg_pill", "Pill"],
	])
		says(key, label);
	expect(page).toMatch(/function pickTextSize[\s\S]{0,160}restyle\(doc, selectedItem\.id, \{ size/);
	expect(page).toMatch(/function pickWeight[\s\S]{0,160}restyle\(doc, selectedItem\.id, \{ weight/);
	expect(page).toMatch(/function pickBg[\s\S]{0,160}restyle\(doc, selectedItem\.id, \{ bg/);
});

test("Beautify: a toggle with 4 gradients, paddings 0/32/64/96, radii 0/12/24 and a shadow switch", () => {
	says("capture_editor_page_beautify", "Beautify");
	expect(page).toContain("BEAUTIFY_GRADIENTS");
	expect(BEAUTIFY_GRADIENTS).toHaveLength(4);
	expect(BEAUTIFY_PADDINGS).toEqual([0, 32, 64, 96]);
	expect(BEAUTIFY_RADII).toEqual([0, 12, 24]);
	expect(page).toContain("BEAUTIFY_PADDINGS");
	expect(page).toContain("BEAUTIFY_RADII");
	says("capture_editor_page_shadow", "Shadow");
	expect(page).toMatch(/aria-pressed=\{beautify\.on\}/);
});

test("the live preview and the export both go through the shared beautify painters", () => {
	expect(page).toContain("paintBackdrop");
	expect(page).toContain("paintBeautified");
	expect(page).toContain("beautifyLayout");
	// the shown size is the beautified output size
	expect(page).toContain("m.capture_editor_page_size_px(");
	expect(m.capture_editor_page_size_px({ w: 10, h: 20 })).toBe("10 × 20 px");
});

test("export order: items on the full image, then crop, then beautify; chrome and the backdrop preview never leak in", () => {
	const at = page.indexOf("function exportPng");
	const exp = page.slice(at, page.indexOf("async function done", at));
	const items = exp.indexOf("paintItems");
	const crop = exp.indexOf("drawImage(full, crop.x");
	const beaut = exp.indexOf("paintBeautified");
	expect(items).toBeGreaterThan(-1);
	expect(crop).toBeGreaterThan(items);
	expect(beaut).toBeGreaterThan(crop);
	expect(exp).toMatch(/beautifyLayout\(size, beautify\)/);
	expect(exp).not.toMatch(/paintBackdrop|drawChrome/);
	// counters need their numbers and spotlights the image size, in the export as in the preview
	expect(exp).toMatch(/numbers: counterNumbers\(doc\.items\)/);
	expect(exp).toMatch(/size: \{ w: doc\.width, h: doc\.height \}/);
});

test("beautify is an export setting: not in the undo history", () => {
	expect(annotate).not.toMatch(/beautify/i);
});

test("each Done button saves with its contract mode: Copy=copy, Save=file, Save & Copy=both; Cancel only closes", () => {
	expect(page).toMatch(
		/aria-label=\{m\.capture_editor_page_copy_to_clipboard\(\)\}[^>]*onclick=\{\(\) => done\("copy"\)\}/,
	);
	expect(page).toMatch(
		/aria-label=\{m\.capture_editor_page_save_a_copy\(\)\}[^>]*onclick=\{\(\) => done\("file"\)\}/,
	);
	expect(page).toMatch(
		/aria-label=\{m\.capture_editor_page_save_and_copy\(\)\}[^>]*onclick=\{\(\) => done\("both"\)\}/,
	);
	expect(page).toMatch(/aria-label=\{m\.common_action_cancel\(\)\}[^>]*onclick=\{requestClose\}/);
	// the mode reaches Rust unchanged, with the base64 PNG, and the window closes only after a successful save
	expect(page).toMatch(
		/invoke<Entry>\("shots_editor_save",\s*\{\s*path,\s*png:\s*exportPng\(\),\s*mode\s*\}\)/,
	);
	const save = page.indexOf('"shots_editor_save"');
	expect(page.indexOf("closeWindow()", save)).toBeGreaterThan(save);
	expect(page).toMatch(/Save &(amp;)? Copy/);
});

test("the export is at image resolution through an offscreen canvas, crop applied, blur applied to pixels", () => {
	expect(page).toMatch(/document\.createElement\(\s*"canvas"\s*\)|new OffscreenCanvas/);
	expect(page).toContain("exportSize");
	expect(page).toContain("paintItems");
	expect(page).toMatch(/getImageData/);
	expect(page).toMatch(/toDataURL\(\s*"image\/png"|toBlob/);
});

test("the export never draws the editor chrome (selection, crop dim) into the saved image", () => {
	const exp = page.slice(page.indexOf("function exportPng"), page.indexOf("function exportPng") + 1800);
	expect(exp).toContain("paintItems");
	expect(exp).not.toMatch(/drawChrome|drawSelection|dim/i);
});

test("keyboard: Cmd/Ctrl+Z undo, Shift redo, Delete/Backspace, Esc, Cmd+Enter = Save & Copy, tool letters", () => {
	expect(page).toMatch(/e\.metaKey\s*\|\|\s*e\.ctrlKey/);
	expect(page).toMatch(/mod && e\.key\.toLowerCase\(\) === "z"/);
	expect(page).toMatch(/e\.shiftKey \? redo\(doc\) : undo\(doc\)/);
	expect(page).toMatch(/mod && e\.key === "Enter"[\s\S]{0,80}done\("both"\)/);
	expect(page).toMatch(/e\.key === "Delete" \|\| e\.key === "Backspace"[\s\S]{0,80}deleteSelected/);
	// Cmd+S saves a copy, Cmd+C copies the image: both only when no text field has the focus
	expect(page).toMatch(/mod && e\.key\.toLowerCase\(\) === "s"[\s\S]{0,80}done\("file"\)/);
	expect(page).toMatch(/mod && e\.key\.toLowerCase\(\) === "c"[\s\S]{0,260}done\("copy"\)/);
	expect(page).toMatch(/if \(isTyping\(e\.target\)\) return;/);
	expect(page).toMatch(/instanceof HTMLInputElement/);
	// Cmd+C does not steal a text selection
	expect(page).toMatch(/hasTextSelection\(\)/);
	expect(page).toMatch(/e\.key === "Escape"[\s\S]{0,200}escape\(doc\)/);
	expect(page).toMatch(/\/\/ Esc: cancels the running gesture/); // documented in the header comment
});

test("closing with unsaved edits asks first, and only then (isDirty gates the confirmation)", () => {
	expect(page).toContain("isDirty");
	says("capture_discard", "Discard");
	says("capture_editor_page_keep_editing", "Keep editing");
});

test("text is edited in an inline <input>, never contenteditable", () => {
	expect(page).toMatch(/<input[^>]*class="text-edit"|<input[^>]*bind:this=\{textInput\}/s);
	expect(page).not.toMatch(/contenteditable/i);
});

test("pointer positions go through viewToImage: the doc never holds CSS pixels", () => {
	expect(page).toContain("viewToImage");
	expect(page).toContain("imageToView");
});

test("buttons carry accessible names and keyboard focus stays visible", () => {
	expect(page).toMatch(/aria-label=/);
	expect(page).toMatch(/:focus-visible/);
	expect(page).not.toMatch(/outline:\s*none(?!\s*;?\s*\/\*)/);
});

test("it uses the shared pure modules", () => {
	expect(page).toContain("#lib/shots/annotate.ts");
	expect(page).toContain("#lib/shots/render.ts");
});
