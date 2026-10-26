/**
 * Keep a native view and its hole in step with its DOM slot. Returns the teardown.
 *
 * `sync` (the surface's) measures and carves the page's grounds in the caller's task and queues the IPC behind it
 * (`surface.ts`). It runs straight from the observer callbacks: a `ResizeObserver` fires after layout and before paint
 * of the frame whose layout changed, so the hole lands in that frame, not the next one.
 *
 * What triggers it:
 *  - the slot resizing, and the window resizing (page zoom and the sidebar's glide move a slot without resizing it);
 *  - a tab swipe's `data-swiping` mark flipping (the view is parked for the swipe and put back when it lifts).
 *
 * What does not: overlays. A menu, a dialog or a tooltip is ordinary DOM above the view and never moves it; the
 * input over it is the hit map's (`hit.ts`). Nothing loops and nothing runs per frame at rest.
 */
export function watchSlot(slot: Element, sync: () => void): () => void {
	const ro = new ResizeObserver(sync);
	ro.observe(slot);
	// Only the swipe mark matters, anywhere above the slot: a narrow filter keeps unrelated DOM churn out.
	const mo = new MutationObserver(sync);
	mo.observe(document.body, { subtree: true, attributes: true, attributeFilter: ["data-swiping"] });
	window.addEventListener("resize", sync);
	return () => {
		ro.disconnect();
		mo.disconnect();
		window.removeEventListener("resize", sync);
	};
}
