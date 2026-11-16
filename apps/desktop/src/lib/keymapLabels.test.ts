// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { expect, test } from "bun:test";

// Incident (#270): the command palette is where a Linux/Windows user finds shortcuts once the native menu bar is gone
// (menu accelerators are macOS only). Every row carries `shortcutFor(id)`: the platform's label of the command's key
// from the one keymap: `Ctrl+Shift+P` off macOS, `⇧⌘P` on macOS, and the keys that moved off macOS (reopen tab,
// sidebar) as moved. A user's own binding is labelled the same way.
const { keymap, shortcutFor, bindingLabels } = await import("./keymap.svelte.ts");

const ids = [
	"command_palette::Toggle",
	"pane::ReopenClosedItem",
	"shell::ToggleSidebar",
	"workspace::ToggleLeftDock",
	"pane::ActivateNextItem",
	"atlas::Help",
];
const shortcuts = (mac: boolean) => Object.fromEntries(ids.map((id) => [id, shortcutFor(id, mac)]));

test("shortcutFor reads Ctrl labels off macOS, from the platform's own keymap", () => {
	expect(shortcuts(false)).toEqual({
		"command_palette::Toggle": "Ctrl+Shift+P",
		"pane::ReopenClosedItem": "Alt+Shift+T",
		"shell::ToggleSidebar": "Ctrl+B",
		"workspace::ToggleLeftDock": "Alt+Shift+B",
		"pane::ActivateNextItem": "Ctrl+Shift+]",
		"atlas::Help": "Ctrl+/",
	});
});

test("shortcutFor reads glyphs on macOS", () => {
	expect(shortcuts(true)).toEqual({
		"command_palette::Toggle": "⇧⌘P",
		"pane::ReopenClosedItem": "⇧⌘T",
		"shell::ToggleSidebar": "⌃⌘S",
		"workspace::ToggleLeftDock": "⌘B",
		"pane::ActivateNextItem": "⇧⌘]",
		"atlas::Help": "⌘/",
	});
});

test("a user's own binding is labelled for the platform too, and an unbound command has no label", () => {
	keymap.user = { "cmd-shift-j": "atlas::Help", "cmd-/": null, "cmd-shift-p": null };
	try {
		expect(shortcutFor("atlas::Help", false)).toBe("Ctrl+Shift+J");
		expect(shortcutFor("atlas::Help", true)).toBe("⇧⌘J");
		expect(bindingLabels("atlas::Help", false)).toEqual(["Ctrl+Shift+J"]);
		expect(shortcutFor("command_palette::Toggle", false)).toBeUndefined();
	} finally {
		keymap.user = {};
	}
});
