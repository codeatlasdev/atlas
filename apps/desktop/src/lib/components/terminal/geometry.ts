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

/** Scroll lines for a wheel delta (one line per cell height; 16 px before the cell size is known). */
export function wheelLines(deltaY: number, cell: Cell | null): number {
	return Math.round(-deltaY / (cell?.h ?? 16));
}

/** `'path'` with embedded quotes escaped, plus a trailing space, as the xterm tab's drop does. */
export const shellQuote = (p: string) => `'${p.replace(/'/g, "'\\''")}' `;
