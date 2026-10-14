/**
 * Native menu bridge. The menu (crates/atlas-desktop/src/menu.rs) owns the key equivalents of the
 * commands it lists, so macOS delivers them even when focus is in the GPU terminal, an
 * embedded browser page or a native text field. The webview never sees those keys, so nothing
 * fires twice. We push the effective keymap (user remaps included) and each command's
 * availability; a click or key sends `menu://command` back.
 */
import { IS_MAC, bindingsOf } from "#lib/keys.ts";

/**
 * Edit items: the standard macOS keys, fixed (not part of the remappable keymap, so a
 * user binding can never take ⌘Z away from the editor).
 */
export const EDIT_ACCELERATORS: Record<string, string> = {
	"edit::Undo": "Cmd+Z",
	"edit::Redo": "Shift+Cmd+Z",
	"edit::SelectAll": "Cmd+A",
};

/** Commands that have a native menu item (ids match `menu.rs`). */
export const MENU_COMMANDS = [
	"atlas::NewWorktree",
	"workspace::NewTerminal",
	"atlas::NewAgentSession",
	"atlas::NewBrowserTab",
	"workspace::Save",
	"workspace::SaveAll",
	"pane::CloseActiveItem",
	"pane::CloseOtherItems",
	"pane::CloseAllItems",
	"pane::ReopenClosedItem",
	"pane::ActivateNextItem",
	"pane::ActivatePreviousItem",
	"pane::SplitRight",
	"pane::SplitDown",
	"file_finder::Toggle",
	"search::Toggle",
	"command_palette::Toggle",
	"atlas::Jump",
	"shell::ToggleSidebar",
	"workspace::ToggleLeftDock",
	"workspace::ToggleRightDock",
	"workspace::ToggleBottomDock",
	"workspace::ToggleZoom",
	"git::Diff",
	"atlas::Logs",
	"atlas::Config",
	"atlas::RestartService",
	"zed::OpenSettings",
	"atlas::Help",
] as const;

/**
 * Muda accelerator for a canonical stroke (`shift-cmd-p` → `Shift+Cmd+P`), or null when it has none. Off macOS
 * there is none: muda reads `Cmd` as the Super/Windows key there, and the page's keymap owns the keys.
 */
export function accelerator(stroke: string, mac = IS_MAC): string | null {
	if (!mac) return null;
	const parts = stroke.split("-");
	if (stroke.endsWith("--")) parts.splice(-2, 2, "-");
	const key = parts.pop()!;
	const mods = parts.map((m) => ({ ctrl: "Ctrl", alt: "Alt", shift: "Shift", cmd: "Cmd" })[m]);
	const named: Record<string, string> = {
		escape: "Escape",
		enter: "Enter",
		tab: "Tab",
		backspace: "Backspace",
		delete: "Delete",
		space: "Space",
		up: "Up",
		down: "Down",
		left: "Left",
		right: "Right",
		home: "Home",
		end: "End",
		pageup: "PageUp",
		pagedown: "PageDown",
	};
	if (mods.some((m) => !m)) return null;
	// Shift-Escape and bare-ish combos stay with the webview.
	if (key === "escape" || key === "tab") return null;
	return [...mods, named[key] ?? key.toUpperCase()].join("+");
}

/**
 * Commands whose key the terminal takes while it has focus (Ghostty's conventions): ⌘K clears the
 * screen and ⇧⌘D splits down there; elsewhere they stay Jump and Changes (the menu items,
 * ⇧⌘P and the Go menu still reach both).
 */
const TERMINAL_KEYS: Record<string, string> = { "atlas::Jump": "Cmd+K", "git::Diff": "Shift+Cmd+D" };

/**
 * `{ commandId: accelerator|null }` for every menu command, from the effective keymap (Edit keys
 * are fixed). With `terminalFocused`, a key in `TERMINAL_KEYS` is released to the page.
 */
export function menuAccelerators(
	effective: Map<string, string>,
	terminalFocused = false,
	mac = IS_MAC,
): Record<string, string | null> {
	const out: Record<string, string | null> = {
		...Object.fromEntries(
			MENU_COMMANDS.map((id) => [
				id,
				bindingsOf(id, effective)
					.map((k) => accelerator(k, mac))
					.find(Boolean) ?? null,
			]),
		),
		...(mac ? EDIT_ACCELERATORS : Object.fromEntries(Object.keys(EDIT_ACCELERATORS).map((id) => [id, null]))),
	};
	if (terminalFocused) {
		for (const [id, key] of Object.entries(TERMINAL_KEYS)) if (out[id] === key) out[id] = null;
	}
	return out;
}
