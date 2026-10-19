import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { EDIT_COMMANDS, runEdit, targetOf, type EditEnv } from "#lib/edit.ts";
import { allCommands, dispatchStroke, keymap, runCommand } from "#lib/keymap.svelte.ts";
import { ops } from "#lib/symbols.ts";
import { defaultKeymap, merge } from "#lib/keys.ts";
import { forwardedStrokes, strokeOfBrowserKey, type BrowserKey } from "#lib/browserKeys.ts";
import { EDIT_ACCELERATORS, MENU_COMMANDS, menuAccelerators } from "#lib/menu.ts";
import { termFocus } from "#lib/stores/termFocus.svelte.ts";

/** Input sink of a GPU terminal: `data-term-tab` is set by GpuTerminal.svelte. */
const env: EditEnv = {
	viewOf: (el) => {
		const host = el.closest<HTMLElement>(".cm-editor");
		return host ? ops.viewOf(host) : undefined;
	},
	terminalOf: (el) => (el as HTMLElement).dataset?.termTab,
	isField: (el) => !!el.closest("input, textarea, [contenteditable=true], [contenteditable='']"),
};

/** Undo / Redo / Select All from the Edit menu: act on whatever has the focus. */
function edit(id: string): boolean {
	const action = EDIT_COMMANDS[id];
	if (!action) return false;
	runEdit(action, targetOf(document.activeElement, env), {
		exec: (name) => void document.execCommand(name),
		terminalSelectAll: (tab) => void invoke("term_select_all", { tab }).catch(() => {}),
	});
	return true;
}

/** Start listening for menu clicks/keys and keep the menu's keys and enabled state in sync. Returns cleanup. */
export async function startMenu(): Promise<() => void> {
	const off = await listen<string>("menu://command", (e) => edit(e.payload) || runCommand(e.payload));
	// Keys the native side took from a focused browser page (issue #42): the same keymap resolves them.
	const offKeys = await listen<BrowserKey>(
		"browser://key",
		(e) => void dispatchStroke(strokeOfBrowserKey(e.payload)),
	);
	const stop = $effect.root(() => {
		$effect(() => {
			const enabled = new Set(allCommands().map((c) => c.id));
			const accels = menuAccelerators(merge(defaultKeymap(), keymap.user), termFocus.on);
			const items = [...MENU_COMMANDS, ...Object.keys(EDIT_ACCELERATORS)].map((id) => ({
				id,
				accelerator: accels[id],
				// Edit items are always live: they route by focus, not by the command registry.
				enabled: id in EDIT_ACCELERATORS || enabled.has(id),
			}));
			void invoke("menu_sync", { list: items }).catch(() => {});
			void invoke("browser_set_keys", {
				strokes: forwardedStrokes(merge(defaultKeymap(), keymap.user)),
			}).catch(() => {});
		});
	});
	return () => {
		off();
		offKeys();
		stop();
	};
}
