/** Window-level key handling of the main window: keymap dispatch, atlas.yaml bare keys and the ⌃Tab switcher. */
import { configKey } from "#lib/actions.svelte.ts";
import { editorOwnsKey } from "#lib/editor/ownedKeys.ts";
import { primaryKey, shellKeepsCtrl } from "#lib/keys.ts";
import { dispatch } from "#lib/keymap.svelte.ts";
import { cancelSwitcher, commitSwitcher, switcher } from "#lib/shell/switcher.svelte.ts";
import { terminalKeeps } from "#lib/termKeys.ts";

/** Bare keys declared in atlas.yaml (`open_key`, action `key`). */
export function onKeydown(e: KeyboardEvent) {
	if (!primaryKey(e) && configKey(e)) e.preventDefault();
}

/** Keymap commands win over focused terminals/editors, as in Zed. */
export function onKeydownCapture(e: KeyboardEvent) {
	if (e.key === "Escape" && switcher.s) {
		cancelSwitcher();
		e.preventDefault();
		e.stopPropagation();
		return;
	}
	const sink = document.activeElement instanceof HTMLElement && document.activeElement.dataset.termTab;
	// xterm.js (every terminal off macOS) focuses a textarea inside `.xterm`.
	const xterm = document.activeElement instanceof Element && document.activeElement.closest(".xterm");
	if ((sink || xterm) && shellKeepsCtrl(e)) return;
	// A focused text editor keeps its editing chords (⌘/ comment, ⌘[ ⌘] indent, …); see editor/ownedKeys.ts.
	const owned = sink ? terminalKeeps : editorOwnsKey;
	if (e.isComposing || !dispatch(e, owned)) return;
	e.preventDefault();
	e.stopPropagation();
}

/** ⌃ released ends the ⌃Tab switcher: activate the highlighted tab. */
export function onKeyup(e: KeyboardEvent) {
	if (e.key === "Control" && switcher.s) commitSwitcher();
}
