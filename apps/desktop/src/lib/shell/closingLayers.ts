/**
 * A modal that is closing must stop being a modal at once (#501).
 *
 * Every kit modal (`Dialog`, `AlertDialog`, `Drawer`, `Sheet`, `CommandPalette`) is a native `<dialog>` opened with
 * `showModal()`. Closing one only flips `data-state="closed"`: the kit keeps the element alive, still modal, until its
 * exit animation ends (150 ms: `presence.svelte.js` waits for every running animation, `layer.js` closes the dialog
 * after that). For that long the rest of the page is inert, so a click on a button behind the scrim after a dismissal
 * ("close it by clicking outside, then click another button") lands on nothing: the press is lost and a second one is
 * needed. Measured on WebKit: a click 80 ms after the dismissal is lost, one 150 ms after works.
 *
 * Here, the moment any `dialog.ca-overlay` becomes `closed`, its exit animations are finished. The kit's next frame
 * finds nothing running, drops the layer, releases the modal and restores focus: the page answers again one frame
 * (about 16 ms, shorter than a human click to click) after the dismissal. Done once, for the whole document, so no
 * screen opens a dialog "the right way": there is no per-dialog code and a new dialog is covered with no change.
 *
 * Non-modal layers (menus, popovers, tooltips: `[popover]`) do not make the page inert, but their exit fade still
 * catches presses over their own box; `app.css` gives a closed one `pointer-events: none` (a CSS rule is enough there,
 * and it keeps the fade).
 *
 * Works around: @codeatlasdev/ui 4.6.0, `primitives/presence.svelte.js` (waits for the exit animation) and
 * `primitives/layer.js` `openLayer` (keeps the `<dialog>` modal until `close()`). Remove this module, its install in
 * `shell/session.svelte.ts`, the `[popover][data-state="closed"]` rule in `app.css` and the section in docs/desktop.md when the kit
 * releases the modal as soon as it starts closing (codeatlasdev/ui#1334) and `closingLayers.test.ts` passes without it.
 */
const MODAL = "dialog.ca-overlay";

/**
 * Finish every animation running on a closing modal (and inside it) so the kit tears the layer down next frame. One
 * that never ends (a spinner in the body) cannot be finished, and the kit would wait for it forever: it is cancelled.
 */

export function finishExit(dialog: Element): void {
	if (typeof dialog.getAnimations !== "function") return;
	for (const a of dialog.getAnimations({ subtree: true })) {
		if (a.effect?.getComputedTiming().endTime === Infinity) a.cancel();
		else a.finish();
	}
}

/** Start watching the document; returns the teardown. */
export function releaseClosingModals(): () => void {
	const observer = new MutationObserver((records) => {
		for (const r of records) {
			const el = r.target as Element;
			if (el.matches(MODAL) && el.getAttribute("data-state") === "closed") finishExit(el);
		}
	});
	observer.observe(document, { subtree: true, attributes: true, attributeFilter: ["data-state"] });
	return () => observer.disconnect();
}
