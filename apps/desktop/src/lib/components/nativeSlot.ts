/**
 * Re-measure a native view whenever its DOM slot may have moved: the slot resizes or the window resizes.
 * Returns the teardown.
 *
 * A GPU terminal sits BELOW the transparent webview, so a dialog, menu or peek that opens over it needs no
 * native work and `watchSlot` ignores them. An embedded browser is a child webview ABOVE the page and cannot be
 * covered by CSS: it passes `overlays: true` and is re-synced (and parked, see `overlayOpen`) when one opens.
 *
 * Every trigger only schedules one `requestAnimationFrame`; `sync` (which reads layout and crosses
 * IPC) runs once per frame, after the frame's DOM writes. Observers used to call it directly, so one
 * layout change fired several syncs in the same frame, each forcing layout in the middle of Svelte's
 * writes, and the native view lagged the DOM by a frame or two (visible as a flash).
 *
 * `onSwipe` runs (in that frame) when a tab pane starts or stops moving (`data-swiping`), so a terminal's slot can
 * paint over its view while the pane is transformed. Only that attribute is observed for it: no `childList` over the
 * whole body per terminal.
 */
export function watchSlot(
	slot: Element,
	sync: () => void,
	onSwipe?: () => void,
	opts: { overlays?: boolean } = {},
): () => void {
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
	const mo = new MutationObserver((records) => {
		if (onSwipe && records.some((r) => r.attributeName === "data-swiping")) onSwipe();
		schedule();
	});
	mo.observe(document.body, {
		subtree: true,
		attributes: true,
		attributeFilter: opts.overlays ? ["data-state", "data-swiping"] : ["data-swiping"],
		childList: !!opts.overlays,
	});
	window.addEventListener("resize", schedule);
	return () => {
		cancelAnimationFrame(raf);
		ro.disconnect();
		mo.disconnect();
		window.removeEventListener("resize", schedule);
	};
}

/** Is `el` inside a pane the tab pager is moving by a CSS transform? */
export function isSwiping(el: Element | undefined | null): boolean {
	return !!el?.closest("[data-swiping]");
}

const hits = (a: Pick<DOMRect, "left" | "top" | "right" | "bottom">, b: typeof a) =>
	a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;

/**
 * EMBEDDED BROWSERS ONLY. A browser tab is a child webview above the page, a sibling NSView no web layer can cover.
 * GPU terminals are under the page and never ask this (`termLayout.ts`): nothing may park a terminal for an overlay.
 *
 * Dialogs, menus, listboxes, the palette and an open edge peek must sit above the native view (tooltips stay out: a
 * hover hint would park it on every pass): park it while one is open over it. Only one that overlaps the slot can cover it; the others (a popover
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
	// Menus and listboxes (DropdownMenu, Select, Combobox) are kit layers too: one over the slot must park it.
	for (const d of document.querySelectorAll(
		":is([role=dialog], [role=menu], [role=listbox])[data-state=open]",
	))
		// A modal's scrim covers the whole window, whatever its box overlaps, and the native view paints above it.
		if (d.matches(":modal") || hits(d.getBoundingClientRect(), slot)) return true;
	for (const p of document.querySelectorAll<HTMLElement>(".peek-panel[data-state=open]")) {
		const { offsetLeft: left, offsetTop: top } = p;
		if (hits({ left, top, right: left + p.offsetWidth, bottom: top + p.offsetHeight }, slot)) return true;
	}
	return false;
}
