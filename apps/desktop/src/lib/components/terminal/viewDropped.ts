/**
 * The backend answers `term_bounds` with this when a tab's native view is gone (its render
 * thread ended without a `term_close`): the tab must run `term_open` again. Same string as
 * `VIEW_DROPPED` in crates/atlas-desktop/src/term/lifecycle.rs.
 */
export const VIEW_DROPPED = "terminal view dropped";

/** Is `err` (whatever `invoke` rejected with) the backend saying the view was dropped? */
export function isViewDropped(err: unknown): boolean {
	const msg = typeof err === "string" ? err : err instanceof Error ? err.message : String(err ?? "");
	return msg.includes(VIEW_DROPPED);
}
