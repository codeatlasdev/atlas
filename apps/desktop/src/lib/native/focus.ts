/**
 * Focus for native surfaces (terminal, browser, any future kind). A native view's focus is not DOM focus, and the page
 * moves DOM focus on its own after a press: a kit menu or popover closed by that press restores focus to its trigger
 * (`layer.js` close → `anchor.focus()`), so keys went to the "+" button instead of the terminal the user clicked.
 *
 * Rule: a press on a surface's slot focuses that surface; for `GRACE_MS` after it, any DOM focus that lands outside the
 * slot, or dropped to nothing (a restore or a removed element, not a user act: the user's own next press resets the
 * claim), is handed back to the surface.
 */
export interface FocusTarget {
	slot: () => HTMLElement | undefined;
	focus(): void;
}

const GRACE_MS = 400;

export function startNativeFocus(o: {
	surfaces: () => Iterable<FocusTarget>;
	now?: () => number;
}): () => void {
	const now = o.now ?? (() => performance.now());
	let claim: { target: FocusTarget; at: number } | null = null;
	const owner = (el: EventTarget | null) => {
		if (!(el instanceof Node)) return null;
		for (const s of o.surfaces()) if (s.slot()?.contains(el)) return s;
		return null;
	};
	/** The surface under a press: by DOM (the press hit the slot) or by position (it hit an ancestor over the slot). */
	const under = (e: PointerEvent) => {
		const s = owner(e.target);
		if (s) return s;
		for (const c of o.surfaces()) {
			const el = c.slot();
			if (!el || !(e.target instanceof Node) || !e.target.contains(el)) continue;
			const r = el.getBoundingClientRect();
			if (e.clientX >= r.left && e.clientX < r.right && e.clientY >= r.top && e.clientY < r.bottom) return c;
		}
		return null;
	};
	const onDown = (e: PointerEvent) => {
		const s = under(e);
		claim = s ? { target: s, at: now() } : null;
		s?.focus();
	};
	const onFocusIn = (e: FocusEvent) => {
		if (!claim || now() - claim.at > GRACE_MS) return;
		if (claim.target.slot()?.contains(e.target as Node)) return;
		const t = claim.target;
		queueMicrotask(() => t.focus());
	};
	// Focus dropped to nothing (an element removed or blurred, `relatedTarget` null) fires no focusin: check after the
	// event settles whether anything outside the surface took it; if nobody did, give it back.
	const onFocusOut = (e: FocusEvent) => {
		if (!claim || now() - claim.at > GRACE_MS || e.relatedTarget) return;
		const t = claim.target;
		setTimeout(() => {
			const a = document.activeElement;
			if (!a || a === document.body) t.focus();
		}, 0);
	};
	document.addEventListener("pointerdown", onDown, true);
	document.addEventListener("focusin", onFocusIn, true);
	document.addEventListener("focusout", onFocusOut, true);
	return () => {
		document.removeEventListener("pointerdown", onDown, true);
		document.removeEventListener("focusin", onFocusIn, true);
		document.removeEventListener("focusout", onFocusOut, true);
	};
}
