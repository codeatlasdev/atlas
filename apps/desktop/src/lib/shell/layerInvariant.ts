/**
 * "Current state only": outside a gesture, no space layer but the current one may be visible on screen. A
 * parked layer drawn in view is exactly a ghost frame (docs/postmortems/ghost-frames.md). `pager`: the `.pager`
 * element, whose `overflow: hidden` is what keeps a parked layer (one pager-width away) out of view. `kept`: the
 * layers a running gesture moves (pager.ts `kept`), which are allowed on screen.
 */
export function offendingLayers(pager: Element, kept: readonly string[] = []): HTMLElement[] {
	const clip = pager.getBoundingClientRect();
	const x1 = Math.min(clip.right, window.innerWidth);
	const y1 = Math.min(clip.bottom, window.innerHeight);
	const x0 = Math.max(clip.left, 0);
	const y0 = Math.max(clip.top, 0);
	return [...pager.querySelectorAll<HTMLElement>(".space-layer")].filter((el) => {
		if (el.dataset.role === "current" || kept.includes(el.dataset.space ?? "")) return false;
		if (getComputedStyle(el).visibility !== "visible") return false;
		const r = el.getBoundingClientRect();
		return Math.min(r.right, x1) > Math.max(r.left, x0) && Math.min(r.bottom, y1) > Math.max(r.top, y0);
	});
}
