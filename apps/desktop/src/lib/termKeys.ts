/**
 * GPU terminal key chords on macOS, following Ghostty's defaults (`ghostty +list-keybinds
 * --default`). Pure: a `KeyboardEvent`-like in, an action out; GpuTerminal.svelte runs it.
 *
 * Two kinds of action:
 *  - `term`: handled by the terminal itself (scroll, clear, font size, select all, …);
 *  - `key`: a line-editing chord sent to the program as the named key with modifiers, so the
 *    Rust encoder picks legacy bytes or Kitty `CSI u` (see `atlas-term/src/input.rs`).
 */

export interface KeyLike {
	key: string;
	metaKey: boolean;
	altKey: boolean;
	ctrlKey: boolean;
	shiftKey: boolean;
}

export type TermAction =
	| { kind: "search" }
	| { kind: "copy" }
	| { kind: "selectAll" }
	| { kind: "clear" }
	| { kind: "jumpPrompt"; up: boolean }
	| { kind: "scroll"; to: "top" | "bottom" | "pageUp" | "pageDown" }
	| { kind: "font"; change: "increase" | "decrease" | "reset" }
	| { kind: "split"; dir: "right" | "down" }
	/** Sent to the program: `name` is the DOM key, `meta` is ⌘ (mapped to a line-editing chord by the encoder). */
	| { kind: "key"; name: string; meta: boolean };

/** ⌘ chords with no shift/alt/ctrl: key → action. */
const CMD: Record<string, TermAction> = {
	f: { kind: "search" },
	c: { kind: "copy" },
	a: { kind: "selectAll" },
	k: { kind: "clear" },
	"=": { kind: "font", change: "increase" },
	"+": { kind: "font", change: "increase" },
	"-": { kind: "font", change: "decrease" },
	"0": { kind: "font", change: "reset" },
	d: { kind: "split", dir: "right" },
	ArrowUp: { kind: "jumpPrompt", up: true },
	ArrowDown: { kind: "jumpPrompt", up: false },
	Home: { kind: "scroll", to: "top" },
	End: { kind: "scroll", to: "bottom" },
	PageUp: { kind: "scroll", to: "pageUp" },
	PageDown: { kind: "scroll", to: "pageDown" },
	// Terminal.app / Ghostty line editing: the shell sees ^U, ^A, ^E.
	Backspace: { kind: "key", name: "Backspace", meta: true },
	ArrowLeft: { kind: "key", name: "ArrowLeft", meta: true },
	ArrowRight: { kind: "key", name: "ArrowRight", meta: true },
};

/** ⇧⌘ chords. */
const CMD_SHIFT: Record<string, TermAction> = {
	d: { kind: "split", dir: "down" },
	ArrowUp: { kind: "jumpPrompt", up: true },
	ArrowDown: { kind: "jumpPrompt", up: false },
	// Ghostty `+`: ⇧⌘= arrives as "+".
	"+": { kind: "font", change: "increase" },
};

/** The terminal action for a ⌘ chord, or `null` (the key keeps its normal path). */
export function termAction(e: KeyLike): TermAction | null {
	if (!e.metaKey || e.ctrlKey) return null;
	if (e.altKey) return null;
	const table = e.shiftKey ? CMD_SHIFT : CMD;
	return table[e.key.length === 1 ? e.key.toLowerCase() : e.key] ?? null;
}

/**
 * Chords a focused GPU terminal keeps instead of the app keymap: the ones whose app binding would
 * otherwise win in the window's capture handler. ⇧⌘D (Changes) is Ghostty's split-down; ⌘D, ⌘A
 * and ⌘K never collide here except through the native menu (see `menu.ts`). Everything else
 * (⌘T, ⌘W, ⇧⌘P …) stays with the app.
 */
const KEPT: Record<string, true> = { "shift+d": true };

export function terminalKeeps(e: KeyLike): boolean {
	return (
		e.metaKey &&
		!e.altKey &&
		!e.ctrlKey &&
		KEPT[`${e.shiftKey ? "shift+" : ""}${e.key.toLowerCase()}`] === true
	);
}

export type OptionAsAlt = "both" | "left" | "right" | "none";

/**
 * Ghostty `macos-option-as-alt`: does this ⌥ press act as Alt (ESC-prefixed) rather than
 * compose a macOS character? `side` is which ⌥ key is held (`AltLeft` / `AltRight`, tracked
 * from keydown/keyup because a chord's own event does not say). Keys that make no printable
 * character with ⌥ (arrows, ⌫, Enter, F-keys) are always Alt: they have no macOS meaning.
 */
export function optionActsAsAlt(setting: OptionAsAlt, side: "left" | "right" | null, key: string): boolean {
	if (key.length !== 1) return true;
	if (setting === "both") return true;
	return setting !== "none" && setting === side;
}

/** Font size after `change`, clamped to the renderer's 6–48 pt range (Ghostty steps by 1 pt). */
export function fontStep(current: number, base: number, change: "increase" | "decrease" | "reset"): number {
	if (change === "reset") return base;
	return Math.min(48, Math.max(6, current + (change === "increase" ? 1 : -1)));
}
