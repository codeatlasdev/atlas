/**
 * What keeps a resting neighbour layer out of reach now that it is not `inert`.
 *
 * `inert` (and `pointer-events: none`) on a layer root is an inherited style change: every role flip restyled the
 * whole subtree, 57 ms per layer of 23k nodes (#121). A parked layer is still translated out of the pager's clip and
 * `aria-hidden`, so it takes no pointer input and is not in the accessibility tree; what is left is focus (Tab, a
 * script's `.focus()`) and the swipe zones. Both read the role live from `data-role` here, which costs nothing to
 * change: no style depends on it below the root.
 */

/** Is `el` inside a layer that is not the current one? */
export function inParkedLayer(el: Element | null): boolean {
	return !!el?.closest(".space-layer[data-role='near']");
}

/**
 * Bounce focus out of a parked layer: back to the element that had it, else blurred. Capture phase on `root`, so it
 * runs before the focused element's own handlers. Returns the detach function.
 *
 * Focusing an element makes the browser scroll every `overflow: hidden` ancestor to reveal it, before `focusin` fires.
 * The pager and its layers never scroll, so a bounce also puts them back at 0; left scrolled by one page, the parked
 * layer would sit under the pointer and take the clicks.
 */
export function guardFocus(root: HTMLElement): () => void {
	let last: HTMLElement | null = null;
	const onFocusIn = (e: FocusEvent) => {
		const el = e.target as HTMLElement;
		if (!inParkedLayer(el)) {
			last = el;
			return;
		}
		el.blur();
		if (last?.isConnected && !inParkedLayer(last)) last.focus({ preventScroll: true });
		for (let a: HTMLElement | null = el; a && a !== root.parentElement; a = a.parentElement) {
			if (a.scrollLeft || a.scrollTop) {
				if (a === root || a.classList.contains("space-layer")) a.scrollTo(0, 0);
			}
		}
	};
	root.addEventListener("focusin", onFocusIn, true);
	return () => root.removeEventListener("focusin", onFocusIn, true);
}
