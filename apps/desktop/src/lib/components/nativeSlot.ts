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
		attributeFilter: ["data-state", "data-swiping"],
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

const hits = (a: Pick<DOMRect, "left" | "top" | "right" | "bottom">, b: typeof a) =>
	a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;

/**
 * Dialogs, the palette and an open edge peek must sit above the native view, which is a sibling NSView no web layer
 * can cover: park it while one is open over it. Only one that overlaps the slot can cover it; the others (a popover
 * over the sidebar, a chip far from the terminal) used to park it too, so it vanished and reappeared on every
 * open/close. A modal is the exception: its scrim dims the whole window and everything behind it is inert, so a
 * native view left bright and clickable beside its box would break the modal (codeatlasdev/atlas#66).
 *
 * A slot inside a pane the tab pager is moving (`data-swiping`) is covered too: the native view cannot follow the pane's
 * CSS transform (no per-frame IPC), so it is parked for the generation and `watchSlot` re-syncs it, at the
 * untransformed rect, when the mark is lifted on `rest`. A sync that runs mid-swipe therefore never measures the rect.
 *
 * A peek panel slides in by `transform`, so its bounding rect is off-window until the animation ends: use its layout
 * box (offset*, transform-free), which is where it will rest. The `data-state` flip on open/close is what makes
 * `watchSlot` re-sync, once, and never on pointermove.
 */
export function overlayOpen(slot: Pick<DOMRect, "left" | "top" | "right" | "bottom">, el?: Element): boolean {
	if (el?.closest("[data-swiping]")) return true;
	for (const d of document.querySelectorAll("[role=dialog][data-state=open]"))
		// A modal's scrim covers the whole window, whatever its box overlaps, and the native view paints above it.
		if (d.matches(":modal") || hits(d.getBoundingClientRect(), slot)) return true;
	for (const p of document.querySelectorAll<HTMLElement>(".peek-panel[data-state=open]")) {
		const { offsetLeft: left, offsetTop: top } = p;
		if (hits({ left, top, right: left + p.offsetWidth, bottom: top + p.offsetHeight }, slot)) return true;
	}
	return false;
}
