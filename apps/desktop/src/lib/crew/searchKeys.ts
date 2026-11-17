/**
 * The kit's Combobox forwards every key of its search field to the listbox, whose type-to-select swallows a letter
 * that starts an item ("a" for "anthropic/…", "pr" for "prod") and jumps the highlight instead of typing it. Put this
 * on a wrapper's `onkeydowncapture`: printable keys typed in the search field stop there, so they only type; arrows,
 * Enter and Esc still reach the list.
 */
export function typeInSearchField(e: KeyboardEvent) {
	const t = e.target as HTMLElement;
	if (
		t.classList.contains("ca-combobox__input") &&
		e.key.length === 1 &&
		!e.metaKey &&
		!e.ctrlKey &&
		!e.altKey
	)
		e.stopPropagation();
}
