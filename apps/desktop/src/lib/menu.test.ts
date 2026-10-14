// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { DEFAULT_KEYMAP, merge } from "./keys.ts";
import { MENU_COMMANDS, accelerator, menuAccelerators } from "./menu.ts";

test("strokes become muda accelerators", () => {
	expect(accelerator("cmd-w")).toBe("Cmd+W");
	expect(accelerator("shift-cmd-t")).toBe("Shift+Cmd+T");
	expect(accelerator("alt-cmd-left")).toBe("Alt+Cmd+Left");
	expect(accelerator("cmd-\\")).toBe("Cmd+\\");
	expect(accelerator("shift-escape")).toBeNull();
	expect(accelerator("ctrl-tab")).toBeNull();
});

test("every menu command has a key in the defaults and a user remap follows", () => {
	const acc = menuAccelerators(merge(DEFAULT_KEYMAP, {}));
	expect(acc["pane::CloseActiveItem"]).toBe("Cmd+W");
	// ⇧Esc stays with the webview; New Browser Tab has no default key.
	const unbound = MENU_COMMANDS.filter((id) => !acc[id]);
	expect(unbound).toEqual(["atlas::NewBrowserTab", "workspace::ToggleZoom"]);
	const remapped = menuAccelerators(
		merge(DEFAULT_KEYMAP, { "cmd-w": null, "cmd-shift-w": "pane::CloseActiveItem" }),
	);
	expect(remapped["pane::CloseActiveItem"]).toBe("Shift+Cmd+W");
});

test("⌘S saves, ⌥⌘S saves all, and the sidebar moved to ⌃⌘S", () => {
	const acc = menuAccelerators(merge(DEFAULT_KEYMAP, {}));
	expect(acc["workspace::Save"]).toBe("Cmd+S");
	expect(acc["workspace::SaveAll"]).toBe("Alt+Cmd+S");
	expect(acc["shell::ToggleSidebar"]).toBe("Ctrl+Cmd+S");
});

test("Edit keys are fixed: a user keymap cannot take ⌘Z / ⇧⌘Z / ⌘A from the editor", () => {
	const acc = menuAccelerators(merge(DEFAULT_KEYMAP, { "cmd-z": "x", "cmd-a": "y" }));
	expect(acc["edit::Undo"]).toBe("Cmd+Z");
	expect(acc["edit::Redo"]).toBe("Shift+Cmd+Z");
	expect(acc["edit::SelectAll"]).toBe("Cmd+A");
});

test("a focused terminal takes ⌘K (clear) and ⇧⌘D (split down); the menu keeps them elsewhere", () => {
	const keys = merge(DEFAULT_KEYMAP, {});
	expect(menuAccelerators(keys)["atlas::Jump"]).toBe("Cmd+K");
	expect(menuAccelerators(keys)["git::Diff"]).toBe("Shift+Cmd+D");
	const inTerminal = menuAccelerators(keys, true);
	expect(inTerminal["atlas::Jump"]).toBeNull();
	expect(inTerminal["git::Diff"]).toBeNull();
	// Everything else is untouched, and a remapped Jump key is not stolen.
	expect(inTerminal["pane::CloseActiveItem"]).toBe("Cmd+W");
	const remapped = menuAccelerators(
		merge(DEFAULT_KEYMAP, { "cmd-k": null, "cmd-shift-j": "atlas::Jump" }),
		true,
	);
	expect(remapped["atlas::Jump"]).toBe("Shift+Cmd+J");
});

// Incident (Windows/Linux): muda reads `Cmd` as the Super/Windows key outside macOS, so the menu advertised and
// grabbed Win+T, Win+S … (keys the system owns) while the page ran the same commands on Ctrl. Off macOS the page's
// keymap handles every key (a focused terminal keeps its Ctrl letters, see shellKeepsCtrl), and the menu sets none.
test("off macOS the menu sets no accelerators: the page's keymap owns the keys", () => {
	const acc = menuAccelerators(merge(DEFAULT_KEYMAP, {}), false, false);
	expect(Object.entries(acc).filter(([, a]) => a !== null)).toEqual([]);
	expect(accelerator("cmd-w", false)).toBeNull();
	expect(accelerator("cmd-w", true)).toBe("Cmd+W");
});
