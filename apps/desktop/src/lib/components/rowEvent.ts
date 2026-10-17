/**
 * True when a click/key that reached a clickable row started inside a menu opened from it. The kit renders a
 * DropdownMenu as a child of its trigger's parent (only its paint is hoisted to the top layer), so picking an item
 * bubbles to the row that owns the menu: without this a menu action also selected the row (codeatlasdev/atlas#67).
 */
export function fromMenu(e: Event): boolean {
	return e.target instanceof Element && !!e.target.closest("[role=menu]");
}
