/**
 * Re-measure a native child view (terminal, browser) whenever its DOM slot may have moved:
 * the slot resizes, the window resizes, or a dialog/palette opens or closes (the native view must
 * be parked under those). Returns the teardown.
 *
 * Every trigger only schedules one `requestAnimationFrame`; `sync` (which reads layout and crosses
 * IPC) runs once per frame, after the frame's DOM writes. Observers used to call it directly, so one
 * layout change fired several syncs in the same frame, each forcing layout in the middle of Svelte's
 * writes, and the native view lagged the DOM by a frame or two (visible as a flash).
 */
export function watchSlot(slot: Element, sync: () => void): () => void {
	let raf = 0;
	const schedule = () => {
		if (!raf)
			raf = requestAnimationFrame(() => {
				raf = 0;
				sync();
			});
	};
	const ro = new ResizeObserver(schedule);
	ro.observe(slot);
	const mo = new MutationObserver(schedule);
	mo.observe(document.body, {
		subtree: true,
		attributes: true,
		attributeFilter: ["data-state"],
		childList: true,
	});
	window.addEventListener("resize", schedule);
	return () => {
		cancelAnimationFrame(raf);
		ro.disconnect();
		mo.disconnect();
		window.removeEventListener("resize", schedule);
	};
}

/**
 * Dialogs and the palette must sit above the native view, which is a sibling NSView no web layer can cover: park it
 * while one is open over it. Only a dialog that overlaps the slot can cover it; the others (a popover over the
 * sidebar, a chip far from the terminal) used to park it too, so it vanished and reappeared on every open/close.
 */
export function overlayOpen(slot: Pick<DOMRect, "left" | "top" | "right" | "bottom">): boolean {
	for (const d of document.querySelectorAll("[role=dialog][data-state=open]")) {
		const r = d.getBoundingClientRect();
		if (r.left < slot.right && r.right > slot.left && r.top < slot.bottom && r.bottom > slot.top) return true;
	}
	return false;
}
