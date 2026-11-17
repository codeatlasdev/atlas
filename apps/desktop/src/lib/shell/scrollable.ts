/**
 * Would something under the pointer scroll horizontally for this wheel event? Gestures
 * (back/forward, tab swipe) only act when nothing can: logs, editors, terminals and
 * horizontal lists keep their scrolling.
 */

/** The bits of an element the check reads; the DOM adapter below produces it. */
export interface ScrollProbe {
	scrollLeft: number;
	scrollWidth: number;
	clientWidth: number;
	/** `overflow-x` computed value. */
	overflowX: string;
	/** Surface that handles horizontal wheel itself and shows no scroll position (terminal, `data-gesture="own"`). */
	own: boolean;
}

/** Can the element scroll further in the direction of `deltaX` (DOM sign: positive = content moves left)? */
export function canScrollX(el: ScrollProbe, deltaX: number): boolean {
	if (el.own) return true;
	if (!/auto|scroll|overlay/.test(el.overflowX)) return false;
	const max = el.scrollWidth - el.clientWidth;
	if (max <= 1) return false;
	return deltaX > 0 ? el.scrollLeft < max - 1 : el.scrollLeft > 1;
}

/** Walk `chain` (target first, outward) and report whether any element can consume `deltaX`. */
export function consumedBy(chain: Iterable<ScrollProbe>, deltaX: number): boolean {
	for (const el of chain) if (canScrollX(el, deltaX)) return true;
	return false;
}

/**
 * Which way an element can still scroll, as the native swipe engine wants it (see `swipe_engine::Scroll`):
 * `left` = it can reveal more of its left side (`scrollLeft` above 0), `right` = of its right side. An editor with a
 * long line is a scroller like any other: at its edge it can no longer go that way and the gesture is a swipe's.
 */
export function edgesOf(el: ScrollProbe): { left: boolean; right: boolean } {
	return { left: canScrollX(el, -1), right: canScrollX(el, 1) };
}

export function probeOf(el: HTMLElement): ScrollProbe {
	const overflows = el.scrollWidth > el.clientWidth + 1;
	return {
		scrollLeft: el.scrollLeft,
		scrollWidth: el.scrollWidth,
		clientWidth: el.clientWidth,
		// Cheap pre-check: an element that does not overflow never consumes, skip the style read.
		overflowX: overflows ? getComputedStyle(el).overflowX : "visible",
		own: el.dataset.gesture === "own" || el.classList.contains("xterm"),
	};
}

/** Ancestors of `from` up to and including `stop`, as probes, target first. */
function* ancestors(from: Element | null, stop: Element): Generator<ScrollProbe> {
	for (let el = from; el instanceof HTMLElement; el = el.parentElement) {
		yield probeOf(el);
		if (el === stop) return;
	}
}

/** Does anything between the wheel target and `stop` (inclusive) scroll horizontally for `deltaX`? */
export function scrollsHorizontally(target: EventTarget | null, stop: Element, deltaX: number): boolean {
	return consumedBy(ancestors(target instanceof Element ? target : null, stop), deltaX);
}
