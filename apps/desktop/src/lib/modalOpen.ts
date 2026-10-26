/**
 * Is a modal layer (a kit Dialog, Drawer, Sheet) open? The kit announces every layer change as a `layerchange`
 * event on `document` with `detail.modal` (documented on `onLayerChange`), which also covers dialogs opened by code
 * that is not ours. Listening to the event, not importing the kit, keeps the keymap free of the component library.
 *
 * A modal makes the page behind it inert for pointer and focus, but window-level key handling (the keymap, native
 * menu items, strokes forwarded from a native page) runs ahead of any element and has to ask (codeatlasdev/atlas#66).
 */
let open = false;

/** True while at least one modal layer is open. */
export const modalOpen = () => open;

/** Start following the kit's layers; returns the teardown, which also reopens the gate (hot reload). */
export function startModalWatch(): () => void {
	const on = (e: Event) => (open = !!(e as CustomEvent<{ modal?: boolean }>).detail?.modal);
	document.addEventListener("layerchange", on);
	return () => {
		document.removeEventListener("layerchange", on);
		open = false;
	};
}

/**
 * Follow modal state from the same `layerchange` event: `fn(true)` when a modal opens, `fn(false)` when the last one
 * closes (called on every layer change, so keep it idempotent). Returns the teardown. For a surface that must not
 * appear behind a dialog (the edge peek).
 */
export function onModalChange(fn: (modal: boolean) => void): () => void {
	const on = (e: Event) => fn(!!(e as CustomEvent<{ modal?: boolean }>).detail?.modal);
	document.addEventListener("layerchange", on);
	return () => document.removeEventListener("layerchange", on);
}
