/**
 * A modal that is closing must stop being a modal at once, and still fade out (#501).
 *
 * Every kit modal (`Dialog`, `AlertDialog`, `Drawer`, `Sheet`, `CommandPalette`) is a native `<dialog>` opened with
 * `showModal()`. Closing one only flips `data-state="closed"`: the kit keeps the element alive, still modal, until its
 * exit animation ends (150 ms: `presence.svelte.js` waits for every running animation, `layer.js` closes the dialog
 * after that). For that long the rest of the page is inert, so a click on a button behind the scrim after a dismissal
 * ("close it by clicking outside, then click another button") lands on nothing: the press is lost and a second one is
 * needed. Measured on WebKit: a click 30 or 80 ms after the dismissal is lost, one 150 ms after works.
 *
 * Here, the moment any `dialog.ca-overlay` turns `closed` (a microtask after the flip, before any input event can
 * arrive) it is released from modality while the kit goes on with its own exit:
 *
 * - `close()` then `show()` in the same task: the dialog leaves the top layer and stops being modal (the page is not
 *   inert any more, no `::backdrop`) and is shown again non-modal, where it is, so its exit animation keeps playing;
 * - it is made `inert` first: nothing in it can take a press or focus (so the kit's focus trap, which stays until the
 *   layer is torn down, finds nothing to pull focus into) and presses go to the page behind it;
 * - the scrim's fade, which was the dialog's `::backdrop` (modal dialogs only), is replayed by a fixed
 *   `.closing-scrim` element with the kit's own scrim colour and `ca-overlay-out` fade (`app.css`; no blur, Atlas's `--ca-scrim-blur` is `none`);
 * - if the owner opens it again before the exit ends (`data-state` back to `open`), it is made modal again;
 * - the kit restores focus to the opener when it removes the layer; a control the user focused in the meantime
 *   would lose it, so the last focus outside the dialog is put back after that.
 *
 * Done once, for the whole document: no screen opens a dialog "the right way", and a new dialog is covered with no
 * change. An animation that never ends inside it (a spinner) would keep the kit waiting forever: it is cancelled.
 *
 * Non-modal layers (menus, popovers, tooltips: `[popover]`) do not make the page inert, but their exit fade still
 * catches presses over their own box; `app.css` gives a closed one `pointer-events: none`.
 *
 * Works around: @codeatlasdev/ui 4.6.0, `primitives/presence.svelte.js` (waits for the exit animation) and
 * `primitives/layer.js` `openLayer` (keeps the `<dialog>` modal until `close()`). Remove this module, its install in
 * `shell/session.svelte.ts`, the `.closing-scrim` and `[popover][data-state="closed"]` rules in `app.css` and the
 * section in docs/desktop.md when the kit releases the modal as soon as it starts closing (codeatlasdev/ui#1334) and
 * `closingLayers.test.ts` passes without it.
 */
const MODAL = "dialog.ca-overlay";

/** The scrim a released dialog leaves behind to fade; it removes itself when its animation ends. */
function fadeScrim(dialog: Element): HTMLElement {
	const scrim = document.createElement("div");
	scrim.className = "closing-scrim";
	scrim.setAttribute("aria-hidden", "true");
	scrim.addEventListener("animationend", () => scrim.remove(), { once: true });
	dialog.before(scrim);
	return scrim;
}

/** Cancel what can never finish (a spinner in the body): the kit waits for every animation of the subtree. */
function cancelEndless(dialog: Element): void {
	if (typeof dialog.getAnimations !== "function") return;
	for (const a of dialog.getAnimations({ subtree: true }))
		if (a.effect?.getComputedTiming().endTime === Infinity) a.cancel();
}

/** Released dialogs that the kit has not removed yet, and the control the user focused outside them meanwhile. */
const released = new Map<HTMLDialogElement, HTMLElement>();
let userFocus: Element | null = null;

/** End the modality of a closing dialog and let it fade out in place. */
export function release(dialog: HTMLDialogElement): void {
	if (!dialog.matches(":modal")) return;
	const scrim = fadeScrim(dialog);
	// Inert before `show()`: its focusing steps then find nothing to focus inside the dialog.
	dialog.close();
	const back = document.activeElement;
	dialog.inert = true;
	dialog.dataset.released = "";
	dialog.show();
	if (back instanceof HTMLElement && back !== document.body && document.activeElement !== back)
		back.focus({ preventScroll: true });
	released.set(dialog, scrim);
	cancelEndless(dialog);
}

/** The owner opened it again during the exit (`open` flipped back): it is the modal it was, no fade left over. */
export function reclaim(dialog: HTMLDialogElement): void {
	const scrim = released.get(dialog);
	if (!scrim) return;
	released.delete(dialog);
	scrim.remove();
	dialog.inert = false;
	delete dialog.dataset.released;
	dialog.close();
	dialog.showModal();
	// `showModal()` focused nothing if the button that reopened it was the focus; the kit's own open puts it on the first control.
	if (!dialog.contains(document.activeElement))
		dialog
			.querySelector<HTMLElement>("button, [href], input, select, textarea, [tabindex]")
			?.focus({ preventScroll: true });
}

/** Start watching the document; returns the teardown. */
export function releaseClosingModals(): () => void {
	const closing = new MutationObserver((records) => {
		for (const r of records) {
			const el = r.target as Element;
			if (!el.matches(MODAL)) continue;
			if (el.getAttribute("data-state") === "closed") release(el as HTMLDialogElement);
			else reclaim(el as HTMLDialogElement);
		}
	});
	closing.observe(document, { subtree: true, attributes: true, attributeFilter: ["data-state"] });
	// The kit removes the layer (and moves focus back to the opener) in one task; this runs right after it.
	const removed = new MutationObserver((records) => {
		for (const r of records)
			for (const n of r.removedNodes) {
				if (!(n instanceof HTMLDialogElement)) continue;
				released.get(n)?.remove();
				if (released.delete(n)) settleFocus();
			}
	});
	removed.observe(document.body, { childList: true });
	const onFocusin = (e: FocusEvent) => {
		// Only while a released dialog is still in the page: the kit removes it BEFORE it hands focus back to the opener,
		// and that focus is not the user's choice.
		if (
			e.target instanceof Element &&
			!e.target.closest("[data-released]") &&
			[...released.keys()].some((d) => d.isConnected)
		)
			userFocus = e.target;
	};
	document.addEventListener("focusin", onFocusin, true);
	return () => {
		closing.disconnect();
		removed.disconnect();
		document.removeEventListener("focusin", onFocusin, true);
		released.clear();
		userFocus = null;
	};
}

/** The kit handed focus back to the opener; if the user focused something else in the exit window, it keeps it. */
function settleFocus(): void {
	const wanted = userFocus;
	if (released.size > 0) return;
	userFocus = null;
	if (wanted instanceof HTMLElement && wanted.isConnected && document.activeElement !== wanted)
		wanted.focus({ preventScroll: true });
}
