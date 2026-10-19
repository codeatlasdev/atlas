/**
 * GPU terminal key chords on macOS, following Ghostty's defaults (`ghostty +list-keybinds
 * --default`). Pure: a `KeyboardEvent`-like in, an action out; GpuTerminal.svelte runs it.
 *
 * Two kinds of action:
 *  - `term`: handled by the terminal itself (scroll, clear, font size, select all, …);
 *  - `key`: a line-editing chord sent to the program as the named key with modifiers, so the
 *    Rust encoder picks legacy bytes or Kitty `CSI u` (see `atlas-term/src/input.rs`).
 */

interface KeyLike {
	key: string;
	metaKey: boolean;
	altKey: boolean;
	ctrlKey: boolean;
	shiftKey: boolean;
}

export type TermAction =
	| { kind: "search" }
	| { kind: "searchNext"; forward: boolean }
	| { kind: "selectOutput" }
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
	g: { kind: "searchNext", forward: true },
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
	g: { kind: "searchNext", forward: false },
	y: { kind: "selectOutput" },
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
 * otherwise win in the window's capture handler. ⌘K (Jump) is Ghostty's clear_screen, ⇧⌘D
 * (Changes) its split-down and ⌘= ⌘+ ⌘- ⌘0 its font size (the layout zoom, `zoom.ts`, takes them
 * everywhere else). Everything else (⌘T, ⌘W, ⇧⌘P …) stays with the app.
 */
const KEPT: Record<string, true> = {
	k: true,
	"shift+d": true,
	"=": true,
	"+": true,
	"shift++": true,
	"-": true,
	"0": true,
};

export function terminalKeeps(e: KeyLike): boolean {
	return (
		e.metaKey &&
		!e.altKey &&
		!e.ctrlKey &&
		KEPT[`${e.shiftKey ? "shift+" : ""}${e.key.toLowerCase()}`] === true
	);
}

type OptionAsAlt = "both" | "left" | "right" | "none";

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

/** Keys that never produce terminal input on their own (modifiers, IME and dead-key states). */
const NO_INPUT = new Set(["Shift", "Control", "Alt", "Meta", "CapsLock", "Dead", "Process", "Unidentified"]);

/** ⌘ chords, IME composition and bare modifiers never reach the program as keys. */
export const producesInput = (e: { key: string; metaKey: boolean; isComposing: boolean }) =>
	!(e.metaKey || e.isComposing || NO_INPUT.has(e.key));

/** Payload of the `term_key` command (the renderer's encoder picks legacy bytes or Kitty `CSI u`). */
interface TermKey {
	key: string;
	shift: boolean;
	alt: boolean;
	ctrl: boolean;
}

/**
 * The key to send to the program for a keydown the page must handle itself, or `null` when the
 * event belongs elsewhere: ⌘ chords (menu / keymap / paste), IME composition, bare modifiers, and
 * plain printable keys (those arrive through the textarea's `input`, where IME and dead keys are
 * composed; so does ⌥-typed text when ⌥ is not Alt).
 */
export function keyToSend(
	e: KeyLike & { code: string; isComposing: boolean },
	option: OptionAsAlt,
	side: "left" | "right" | null,
): TermKey | null {
	if (!producesInput(e)) return null;
	let key = e.key;
	const alt = e.altKey && optionActsAsAlt(option, side, key);
	// Option as Meta: the physical key, not the macOS option-layer character.
	if (alt && /^(Key[A-Z]|Digit\d)$/.test(e.code)) {
		const c = e.code.slice(-1);
		key = e.shiftKey ? c : c.toLowerCase();
	}
	if (key.length === 1 && !e.ctrlKey && !alt) return null;
	return { key, shift: e.shiftKey, alt, ctrl: e.ctrlKey };
}

/** What the xterm.js terminal does with a keydown before it encodes it for the program. */
export type XtermKey = "search" | "copy" | "paste" | "app" | "shiftEnter" | null;

/**
 * xterm.js keys (every terminal off macOS, and remote ones on macOS). macOS: ⌘F searches, other ⌘ chords are the
 * app's. Elsewhere, like Windows Terminal: Ctrl+C copies when text is selected (else it is ^C for the program),
 * Ctrl+Shift+C copies, Ctrl+V and Ctrl+Shift+V paste, Ctrl+Shift+F searches and other Ctrl+Shift chords are the
 * app's. Shift+Enter is sent as `CSI 13;2u` so agents can tell it from Enter.
 */
export function xtermKey(e: KeyLike, hasSelection: boolean, mac: boolean): XtermKey {
	const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
	if (mac) {
		if (e.metaKey && k === "f") return "search";
		if (e.metaKey) return "app";
	} else if (e.ctrlKey && !e.altKey && !e.metaKey) {
		if (k === "c" && (e.shiftKey || hasSelection)) return "copy";
		if (k === "v") return "paste";
		if (e.shiftKey && k === "f") return "search";
		if (e.shiftKey) return "app";
	}
	if (e.shiftKey && k === "Enter" && !e.ctrlKey && !e.metaKey && !e.altKey) return "shiftEnter";
	return null;
}
