/**
 * Input methods (fcitx5, ibus) on the native terminal. WebKitGTK already runs the GTK input-method context for the
 * focused sink textarea and sends us its composition events; the two things the page must still do:
 *
 *  - tell the renderer the preedit text, so it draws it at the cursor (`term_preedit`), and
 *  - put the sink where the cursor is while composing, because the IM's candidate window opens at the focused text
 *    field's caret rectangle and the sink otherwise sits at the slot's corner.
 *
 * Pure: `GpuTerminal.svelte` does the I/O.
 */

/** The part of a `term_stats` row the sink placement reads. */
export interface CursorStats {
	cell_w: number;
	cell_h: number;
	/** The program's cursor in cells of the viewport (`term/settings.rs::cursor_row`). */
	cursor?: { col: number; row: number };
}

/** The sink's box in the slot, CSS px from the slot's top left. */
export interface SinkBox {
	left: number;
	top: number;
	width: number;
	height: number;
}

/**
 * Where the sink goes while composing: on the cursor's cell, one cell tall so the candidate window opens just below
 * the line being typed. `null` when the cell size or the cursor is unknown (before the first frame): the sink stays
 * where it is.
 */
export function sinkBox(
	s: CursorStats | undefined,
	pad: { padding_x: number; padding_y: number },
): SinkBox | null {
	if (!s?.cursor || !(s.cell_w > 0) || !(s.cell_h > 0)) return null;
	return {
		left: pad.padding_x + s.cursor.col * s.cell_w,
		top: pad.padding_y + s.cursor.row * s.cell_h,
		width: 1,
		height: s.cell_h,
	};
}

/** The sink at rest: no area, so it has no caret to blink (an idle terminal draws nothing). */
export const SINK_REST: SinkBox = { left: 0, top: 0, width: 0, height: 0 };
