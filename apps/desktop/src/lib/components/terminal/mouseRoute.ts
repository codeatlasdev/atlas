/** Who a pointer event belongs to when the program in the terminal may want the mouse. Pure. */

/**
 * `program`: the app running in the terminal asked for mouse reports (vim, tmux, an agent TUI) and gets this
 * event. `terminal`: Atlas handles it (selection, links). ⌘ is the app's gesture (links, paths) and ⇧ gives
 * the selection back to the terminal, as Ghostty's `mouse-shift-capture = false`.
 */
export function mouseRoute(
	programOwnsMouse: boolean,
	e: { shiftKey: boolean; metaKey: boolean },
): "program" | "terminal" {
	return programOwnsMouse && !e.shiftKey && !e.metaKey ? "program" : "terminal";
}
