/** Window-level key handling of the main window: keymap dispatch, atlas.yaml bare keys and the ⌃Tab switcher. */
import { configKey } from "#lib/actions.svelte.ts";
import { editorOwnsKey } from "#lib/editor/ownedKeys.ts";
import { dispatch } from "#lib/keymap.svelte.ts";
import { cancelSwitcher, commitSwitcher, switcher } from "#lib/shell/switcher.svelte.ts";
import { zoomKey } from "#lib/stores/zoom.svelte.ts";
import { terminalKeeps } from "#lib/termKeys.ts";

/** Bare keys declared in atlas.yaml (`open_key`, action `key`). */
export function onKeydown(e: KeyboardEvent) {
	if (!e.metaKey && configKey(e)) e.preventDefault();
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
	// A focused text editor keeps its editing chords (⌘/ comment, ⌘[ ⌘] indent, …); see editor/ownedKeys.ts.
	const owned = sink ? terminalKeeps : editorOwnsKey;
	if (e.isComposing) return;
	// Layout zoom: ⌘ on macOS, Ctrl elsewhere (so it wins over ⌃- "go back" there); a focused GPU terminal keeps ⌘= ⌘- ⌘0 for its font.
	if (!(sink && terminalKeeps(e)) && zoomKey(e)) {
		e.preventDefault();
		e.stopPropagation();
		return;
	}
	if (!dispatch(e, owned)) return;
	e.preventDefault();
	e.stopPropagation();
}

/** ⌃ released ends the ⌃Tab switcher: activate the highlighted tab. */
export function onKeyup(e: KeyboardEvent) {
	if (e.key === "Control" && switcher.s) commitSwitcher();
}
