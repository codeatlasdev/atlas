/**
 * Layout zoom: applies `prefs.zoom` to the main webview (Rust `zoom_set`: `Webview::set_zoom`, which scales
 * the page like a browser tab; native views are placed in window points by `lib/zoom.ts`) and persists a
 * change as `zoom` in settings.json. The zoom lives in the webview, so it is re-applied after a reload.
 */
import { invoke } from "@tauri-apps/api/core";
import { toast } from "@codeatlasdev/ui";
import { isMac } from "#lib/lsp/navigation.ts";
import { prefs, setPref } from "#lib/stores/prefs.svelte.ts";
import { applyZoom, zoomAction, zoomStep, type ZoomAction } from "#lib/zoom.ts";

/** Step the zoom, apply it and save it. Resolves once the webview has scaled. */
export async function stepZoom(action: ZoomAction) {
	const next = zoomStep(prefs.value.zoom, action);
	if (next === prefs.value.zoom) {
		if (action !== "reset") toast.info(`Zoom ${Math.round(next * 100)}% is the limit`);
		return;
	}
	await setPref("zoom", next);
}

/** ⌘+ ⌘− ⌘0 (Ctrl elsewhere) from a key event the keymap does not bind (⌘+ is ⇧⌘=). True when handled. */
export function zoomKey(e: KeyboardEvent): boolean {
	const action = zoomAction(e, isMac());
	if (!action) return false;
	void stepZoom(action);
	return true;
}

$effect.root(() => {
	$effect(() => {
		const z = prefs.value.zoom;
		if (!("__TAURI_INTERNALS__" in window)) return;
		void invoke("zoom_set", { factor: z })
			.then(() => {
				applyZoom(z);
				// Layout changed under every native view: `watchSlot` (terminal, browser) and the pager re-measure on resize.
				window.dispatchEvent(new Event("resize"));
			})
			.catch(() => {});
	});
});
