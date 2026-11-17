/**
 * Svelte action for a full-window modal layer rendered inside the app tree (the crew onboarding). Two things make the
 * app behind it truly behind it: the node moves to `<body>` (out of the stacking contexts of the shell, whose rail and
 * title bar sit above any z-index of a descendant, which let "Board ⇧⌘C" and "Workspaces ⇧⌘B" show over the overlay),
 * and every other child of `<body>` becomes `inert` (no hover, no tooltip, no focus, no clicks, hidden from assistive
 * technology) until the layer goes away. What was inert before stays inert afterwards.
 */
export function modalOverlay(node: HTMLElement) {
	const home = { parent: node.parentNode, next: node.nextSibling };
	document.body.append(node);
	const made: Element[] = [];
	for (const el of Array.from(document.body.children)) {
		if (el === node || el.hasAttribute("inert")) continue;
		el.setAttribute("inert", "");
		made.push(el);
	}
	return {
		destroy() {
			for (const el of made) el.removeAttribute("inert");
			// Svelte removes the node itself; put it back first so that removal finds it where it left it.
			if (node.parentNode === document.body && home.parent) home.parent.insertBefore(node, home.next);
		},
	};
}
