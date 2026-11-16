/**
 * The bottom-right corner of the window holds transient cards (toasts, error toasts, the action-result chip, the
 * Getting started checklist, suggestions). They are children of ONE fixed flex column (`#atlas-corner`,
 * `CornerStack.svelte`); what is in it, in which order and which card folds is decided by `planCorner`
 * (`cornerPlan.ts`, state in `corner.svelte.ts`), and every card is a `CornerCard`.
 */

/** Distance of the column from the window's right and bottom edges (narrow windows use `CORNER_MARGIN_NARROW`). */
export const CORNER_MARGIN = 16;
/** The same on a narrow window, where the column is a full-width bottom sheet. */
export const CORNER_MARGIN_NARROW = 8;
/** Space between two stacked cards. */
export const CORNER_GAP = 8;
/** Width of every card; narrower windows get the full width minus the margins. */
export const CORNER_WIDTH = 360;
/** Padding the column carries so its scrolling does not clip the cards' shadows (offset by the same amount). */
export const CORNER_BLEED = 20;

/** Id of the container every corner card mounts into. */
export const CORNER_ID = "atlas-corner";

/**
 * Svelte action: moves the node into the corner column (the node keeps its component, reactivity and transitions;
 * only its DOM parent changes) and out again on destroy. Without a corner (tests, the tray page) it stays in place.
 */
export function corner(node: HTMLElement) {
	const host = document.getElementById(CORNER_ID);
	if (!host) return;
	host.append(node);
	return { destroy: () => node.remove() };
}
