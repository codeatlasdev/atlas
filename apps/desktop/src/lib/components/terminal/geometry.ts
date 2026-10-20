/** Pure pointer/shell math for the GPU terminal: cell under the pointer, wheel lines, drop quoting. */

/** Exact cell size in logical px, as reported by the renderer. */
export interface Cell {
	w: number;
	h: number;
}

/** Viewport cell under the pointer, plus the x fraction inside it (selection side). */
export interface CellHit {
	col: number;
	row: number;
	frac: number;
}

export function cellAt(
	clientX: number,
	clientY: number,
	origin: { left: number; top: number },
	cell: Cell,
): CellHit {
	const fx = (clientX - origin.left) / cell.w;
	const fy = (clientY - origin.top) / cell.h;
	return {
		col: Math.max(0, Math.floor(fx)),
		row: Math.max(0, Math.floor(fy)),
		frac: fx - Math.floor(fx),
	};
}

/**
 * The renderer's cell size for a tab from `term_stats`, or `null` before the first frame. The renderer
 * measures in window points; the pointer and the layout are in CSS px, which are `zoom` times smaller.
 */
export function cellFromStats(t: { cell_w: number; cell_h: number } | undefined, zoom = 1): Cell | null {
	return t && t.cell_w > 0 && t.cell_h > 0 ? { w: t.cell_w / zoom, h: t.cell_h / zoom } : null;
}

/**
 * Scroll lines (fractional) for a wheel delta: one line per cell height in pixel mode (16 px
 * before the cell size is known), lines as given in line mode, a screen per page.
 */
export function wheelLines(deltaY: number, cell: Cell | null, deltaMode = 0, rows = 0): number {
	if (deltaMode === 1) return -deltaY;
	if (deltaMode === 2) return -deltaY * rows;
	return -deltaY / (cell?.h ?? 16);
}

/** Whole lines out of a stream of fractional wheel lines; the remainder carries until the direction flips. */
export class WheelAccum {
	#rest = 0;
	take(lines: number): number {
		if (lines * this.#rest < 0) this.#rest = 0;
		this.#rest += lines;
		const n = Math.trunc(this.#rest);
		this.#rest -= n;
		return n || 0;
	}
}

/** `'path'` with embedded quotes escaped, plus a trailing space, as the xterm tab's drop does. */
export const shellQuote = (p: string) => `'${p.replace(/'/g, "'\\''")}' `;
