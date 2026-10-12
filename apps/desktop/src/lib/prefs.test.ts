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
