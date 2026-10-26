/**
 * The one loading indicator of every native surface: a 2 px strip the core reserves ABOVE the native view (the surface
 * insets the rect it sends by `STRIP`), so it can never be covered by, or bleed into, the native view or the DOM chrome
 * around it. It is a child of the slot, absolutely placed on the strip; the look lives in `app.css` (`.native-load`):
 * brand colour, an indeterminate shimmer (a still bar under `prefers-reduced-motion`), faded in and out by opacity.
 *
 * One element per surface, kept for its whole life and only toggled by `data-on`: inserting or removing it would feed
 * the slot's own resize observer (`watchSlot`) and loop.
 */

/** Height in CSS px of the strip the core takes from the top of the native view for the indicator. */
export const STRIP = 2;

export interface Indicator {
	/** The element the strip lives in (a slot that was replaced needs a new indicator). */
	host: HTMLElement;
	/** Show (fade in) or hide (fade out) the bar. */
	set(on: boolean): void;
	/** Put the strip at `top` px below the slot's top edge (the native view starts `STRIP` lower). */
	place(top: number): void;
	remove(): void;
}

export function createIndicator(slot: HTMLElement): Indicator {
	const el = document.createElement("div");
	el.className = "native-load";
	el.setAttribute("aria-hidden", "true");
	el.dataset.on = "false";
	el.style.height = `${STRIP}px`;
	slot.append(el);
	return {
		host: slot,
		set(on) {
			el.dataset.on = String(on);
		},
		place(top) {
			el.style.top = `${top}px`;
		},
		remove() {
			el.remove();
		},
	};
}
