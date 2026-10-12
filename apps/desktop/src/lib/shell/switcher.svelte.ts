import { ws } from "#lib/stores/workspace.svelte.ts";
import { openSwitcher, pickSwitcher, stepSwitcher, type Switcher } from "#lib/shell/switcher.ts";

/** The open ⌃Tab overlay (null = closed). Activation happens on ⌃ release, never per press. */
export const switcher = $state<{ s: Switcher | null }>({ s: null });

/** ⌃Tab / ⌃⇧Tab: open the overlay, or step it when already open. */
export function switchTab(step: 1 | -1) {
	const w = ws.selected;
	if (!w) return;
	switcher.s = switcher.s ? stepSwitcher(switcher.s, step) : openSwitcher(ws.mru(w.id), step);
}

/** ⌃ released: activate the highlighted tab and close the overlay. */
export function commitSwitcher() {
	const id = switcher.s && pickSwitcher(switcher.s);
	switcher.s = null;
	if (id) ws.activate(id);
}

export function cancelSwitcher() {
	switcher.s = null;
}
