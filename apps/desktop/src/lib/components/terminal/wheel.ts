/**
 * What one `wheel` event becomes on its way to `term_wheel` (`crates/atlas-desktop/src/term/wheel.rs`): the page's
 * only job is to say how far, with what kind of device, and where. Accumulating pixels into rows, the ×3 of a notch
 * wheel, routing (arrows, mouse reports, scrollback) and the sub-row offset are the host's (`atlas_term::wheel`,
 * Ghostty's algorithm), in one place and one IPC call per event.
 */
import type { Cell } from "./geometry.ts";

/** Pixels a row is worth before the renderer has measured a cell. */
const FALLBACK_ROW_PX = 16;

/** The wire shape; keys are the Rust `WheelArgs` fields. */
export interface WheelArgs {
	/** Positive = up, in rows of the page's cell height (a notch wheel: in lines). */
	rows: number;
	/** A trackpad-like device (`deltaMode` pixels or pages); a notch wheel is not. */
	precise: boolean;
	col: number;
	row: number;
	shift: boolean;
	alt: boolean;
	ctrl: boolean;
	readonly: boolean;
}

/**
 * `screenRows` is the terminal's height in rows (a page-mode delta scrolls a screen). `at` is the cell under the
 * pointer. Natural scrolling is the OS's business: `deltaY` already has the sign the user's fingers meant.
 */
export function wheelArgs(
	e: Pick<WheelEvent, "deltaY" | "deltaMode" | "shiftKey" | "altKey" | "ctrlKey">,
	cell: Cell | null,
	at: { col: number; row: number },
	screenRows: number,
	readonly: boolean,
): WheelArgs {
	const lines = e.deltaMode === 1;
	const rows = lines
		? -e.deltaY
		: e.deltaMode === 2
			? -e.deltaY * screenRows
			: -e.deltaY / (cell?.h ?? FALLBACK_ROW_PX);
	return {
		rows,
		precise: !lines,
		col: at.col,
		row: at.row,
		shift: e.shiftKey,
		alt: e.altKey,
		ctrl: e.ctrlKey,
		readonly,
	};
}
