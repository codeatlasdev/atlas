/**
 * The tiles of the Atlas mark and the `Frame` the renderer paints. `globe.ts` builds the frames of the first-run flow
 * (scan progress, the calm empty state) from these; `render.ts` draws them.
 */
import { MARK } from "./mark.ts";

/** A tile of the globe: the sheets of the mark and its rim pieces. Sub-pixel specks of the hand trace stay still. */
export interface Tile {
	id: number;
	kind: "sheet" | "rim";
	cx: number;
	cy: number;
	w: number;
	h: number;
}

const SHEET_MIN = 8000;
const RIM_MIN = 100;

/** The completion hit, in seconds on a frame's clock: the camera punches in here and settles. Progress frames sit a second after it. */
export const BEATS = { hit: 1.68 } as const;

export const TILES: Tile[] = MARK.filter((m) => m.w * m.h >= RIM_MIN).map((m) => ({
	id: m.id,
	kind: m.w * m.h >= SHEET_MIN ? ("sheet" as const) : ("rim" as const),
	cx: m.cx,
	cy: m.cy,
	w: m.w,
	h: m.h,
}));

export interface TileState {
	id: number;
	alpha: number;
	/** 0..1: how lit the tile is. */
	glow: number;
}

export interface Frame {
	t: number;
	tiles: TileState[];
	/** White flash of the completion hit (0..1) and the ring it radiates (0..1 = expanding). */
	flash: number;
	ring: number;
	/** Scale of the mark (1 = rest). */
	markScale: number;
}
