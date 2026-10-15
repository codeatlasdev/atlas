// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { DEFAULT_PREFS, resolvePrefs } from "./prefs.ts";

test("missing settings give the defaults", () => {
	expect(resolvePrefs({})).toEqual(DEFAULT_PREFS);
});

test("valid values are taken, nested objects merge per key", () => {
	const p = resolvePrefs({ buffer_font_size: 14, terminal: { cursor_shape: "bar", osc52: "deny" } });
	expect(p.buffer_font_size).toBe(14);
	expect(p.terminal.cursor_shape).toBe("bar");
	expect(p.terminal.osc52).toBe("deny");
	expect(p.terminal.font_size).toBe(DEFAULT_PREFS.terminal.font_size);
});

test("out-of-range, wrong-type and unknown enum values fall back", () => {
	const p = resolvePrefs({
		buffer_font_size: 200,
		ui_font_size: "big",
		buffer_font_family: "  ",
		terminal: { cursor_shape: "triangle", osc52: "always", max_scroll_history_lines: -1 },
		active_pane_modifiers: { inactive_opacity: 0 },
	});
	expect(p.buffer_font_size).toBe(DEFAULT_PREFS.buffer_font_size);
	expect(p.ui_font_size).toBe(DEFAULT_PREFS.ui_font_size);
	expect(p.buffer_font_family).toBe(DEFAULT_PREFS.buffer_font_family);
	expect(p.terminal.cursor_shape).toBe("block");
	expect(p.terminal.osc52).toBe("ask");
	expect(p.terminal.max_scroll_history_lines).toBe(10_000);
	expect(p.active_pane_modifiers.inactive_opacity).toBe(1);
});

test("terminal keyboard and mouse settings: defaults follow Ghostty, bad values fall back", () => {
	const d = resolvePrefs({}).terminal;
	expect([d.copy_on_select, d.option_as_alt, d.click_to_move]).toEqual([true, "both", true]);
	const p = resolvePrefs({
		terminal: { option_as_alt: "left", copy_on_select: false, click_to_move: false },
	});
	expect([p.terminal.option_as_alt, p.terminal.copy_on_select, p.terminal.click_to_move]).toEqual([
		"left",
		false,
		false,
	]);
	expect(resolvePrefs({ terminal: { option_as_alt: "meta" } }).terminal.option_as_alt).toBe("both");
});

test("terminal font and padding: defaults, valid values, out-of-range fall back", () => {
	const d = resolvePrefs({}).terminal;
	expect([d.font_thicken, d.padding_x, d.padding_y]).toEqual([false, 10, 6]);
	const p = resolvePrefs({ terminal: { font_thicken: true, padding_x: 12, padding_y: 64 } }).terminal;
	expect([p.font_thicken, p.padding_x, p.padding_y]).toEqual([true, 12, 64]);
	const bad = resolvePrefs({ terminal: { font_thicken: "yes", padding_x: -1, padding_y: 65 } }).terminal;
	expect([bad.font_thicken, bad.padding_x, bad.padding_y]).toEqual([false, 10, 6]);
});

test("lsp map: defaults to empty, keeps valid entries", () => {
	expect(resolvePrefs({}).lsp).toEqual({});
	const p = resolvePrefs({
		lsp: {
			typescript: {
				command: " /opt/tsls ",
				args: ["--stdio", "--log"],
				initialization_options: { a: 1 },
				settings: { b: { c: true } },
				disabled: false,
			},
			rust: { disabled: true },
		},
	});
	expect(p.lsp).toEqual({
		typescript: {
			command: "/opt/tsls",
			args: ["--stdio", "--log"],
			initialization_options: { a: 1 },
			settings: { b: { c: true } },
			disabled: false,
		},
		rust: { disabled: true },
	});
});

test("lsp map: bad shapes are dropped per field, empty entries disappear", () => {
	for (const bad of [null, "x", 3, [], true]) expect(resolvePrefs({ lsp: bad }).lsp).toEqual({});
	const p = resolvePrefs({
		lsp: {
			go: "gopls",
			python: [1, 2],
			c: null,
			zig: { command: 5, args: "--stdio", initialization_options: [], settings: "x", disabled: "yes" },
			lua: { command: "   ", args: ["ok", 1] },
			java: { command: "jdtls", args: ["-data", "/tmp"], extra: 1 },
		},
	});
	expect(p.lsp).toEqual({ java: { command: "jdtls", args: ["-data", "/tmp"] } });
});

test("editor prefs: defaults, valid values, ranges and enums", () => {
	const d = resolvePrefs({}).editor;
	expect(d).toEqual(DEFAULT_PREFS.editor);
	expect([d.tab_size, d.insert_spaces, d.render_whitespace, d.large_file_threshold_bytes]).toEqual([
		4,
		true,
		"none",
		2_000_000,
	]);
	const ok = resolvePrefs({
		editor: { tab_size: 2, word_wrap: true, render_whitespace: "all", large_file_threshold_bytes: 100_000 },
	}).editor;
	expect([ok.tab_size, ok.word_wrap, ok.render_whitespace, ok.large_file_threshold_bytes]).toEqual([
		2,
		true,
		"all",
		100_000,
	]);
	const bad = resolvePrefs({
		editor: {
			tab_size: 0,
			render_whitespace: "boundary",
			large_file_threshold_bytes: 99_999,
			minimap: "yes",
		},
	}).editor;
	expect([bad.tab_size, bad.render_whitespace, bad.large_file_threshold_bytes, bad.minimap]).toEqual([
		4,
		"none",
		2_000_000,
		false,
	]);
	expect(resolvePrefs({ editor: { tab_size: 17 } }).editor.tab_size).toBe(4);
	expect(
		resolvePrefs({ editor: { large_file_threshold_bytes: 100_000_001 } }).editor.large_file_threshold_bytes,
	).toBe(2_000_000);
	expect(resolvePrefs({ editor: "wide" }).editor).toEqual(DEFAULT_PREFS.editor);
});

test("editor.rulers: whole columns, sorted, deduplicated; the rest is dropped", () => {
	expect(
		resolvePrefs({ editor: { rulers: [100, 80, 80, 0, -1, 1.5, "90", 501, 500, null] } }).editor.rulers,
	).toEqual([80, 100, 500]);
	expect(resolvePrefs({ editor: { rulers: "80" } }).editor.rulers).toEqual([]);
	expect(resolvePrefs({ editor: {} }).editor.rulers).toEqual([]);
	expect(resolvePrefs({}).editor.rulers).toEqual([]);
});

test("resolving does not share mutable defaults between calls", () => {
	const a = resolvePrefs({});
	a.editor.rulers.push(80);
	a.lsp.rust = { disabled: true };
	expect(resolvePrefs({}).editor.rulers).toEqual([]);
	expect(resolvePrefs({}).lsp).toEqual({});
	expect(DEFAULT_PREFS.editor.rulers).toEqual([]);
});

// Incident (contrast audit): the default was 1 (off), so the ANSI "black"/"bright black" every TUI uses for dim text
// rendered at 1.2-3.4:1 in all six themes. The GPU terminal lifts a cell to the floor at draw time (the theme's ANSI
// table stays the spec's); the default is WCAG AA text, an explicit value the user set is kept (1 = off).
test("terminal.minimum_contrast defaults to WCAG AA (4.5); an explicit value, including 1 (off), is kept", () => {
	expect(resolvePrefs({}).terminal.minimum_contrast).toBe(4.5);
	expect(
		resolvePrefs({ terminal: { minimum_contrast: 1, minimum_contrast_chosen: true } }).terminal
			.minimum_contrast,
	).toBe(1);
	expect(resolvePrefs({ terminal: { minimum_contrast: 7 } }).terminal.minimum_contrast).toBe(7);
	expect(resolvePrefs({ terminal: { minimum_contrast: 99 } }).terminal.minimum_contrast).toBe(4.5);
	expect(resolvePrefs({ terminal: { minimum_contrast: "high" } }).terminal.minimum_contrast).toBe(4.5);
});

// Incident (contrast audit): changing the default to 4.5 never reached existing installs: the settings file already
// held `"minimum_contrast": 1` (the old default, written back whole by the Terminal tab). A stored 1 with no marker is
// the old default, not a choice; one the user sets afterwards carries `minimum_contrast_chosen: true` and is kept.
test("a persisted minimum_contrast of 1 without the chosen marker is the old default and migrates to 4.5", () => {
	expect(resolvePrefs({ terminal: { minimum_contrast: 1 } }).terminal.minimum_contrast).toBe(4.5);
	expect(
		resolvePrefs({ terminal: { minimum_contrast: 1, minimum_contrast_chosen: true } }).terminal
			.minimum_contrast,
	).toBe(1);
	expect(resolvePrefs({ terminal: { minimum_contrast: 7 } }).terminal.minimum_contrast).toBe(7);
	expect(resolvePrefs({ terminal: { minimum_contrast: 1.5 } }).terminal.minimum_contrast).toBe(1.5);
});

test("diff view pref: defaults to unified, accepts split, bad values fall back (split diff, #26)", () => {
	expect(resolvePrefs({}).diff.view).toBe("unified");
	expect(resolvePrefs({ diff: { view: "split" } }).diff.view).toBe("split");
	expect(resolvePrefs({ diff: { view: "sideways" } }).diff.view).toBe("unified");
	expect(resolvePrefs({ diff: { view: 2 } }).diff.view).toBe("unified");
	expect(resolvePrefs({ diff: "split" }).diff.view).toBe("unified");
});
