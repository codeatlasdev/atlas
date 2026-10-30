/**
 * The globe as a progress meter and as the resting stage. These build the `Frame`s of the first-run flow (the renderer
 * paints them): a scan lights one sheet per project, finishing a stack lights them all with a hit, and the empty state
 * is the dim globe with a single slow glint.
 */
import { BEATS, TILES, type Frame, type TileState } from "./timeline.ts";

/** The 12 sheets of the mark, ids in a fixed lighting order that spreads over the sphere (far-apart sheets first). */
export const SHEETS: number[] = (() => {
	const sheets = TILES.filter((t) => t.kind === "sheet");
	const left = [...sheets];
	const order: number[] = [];
	// Farthest-point order: start with the centre-most sheet, then always take the sheet farthest from those chosen.
	let cur = left.sort((a, b) => Math.hypot(a.cx - 214, a.cy - 214) - Math.hypot(b.cx - 214, b.cy - 214))[0]!;
	while (cur) {
		order.push(cur.id);
		left.splice(left.indexOf(cur), 1);
		if (!left.length) break;
		cur = left.reduce((best, t) => {
			const d = (x: typeof t) =>
				Math.min(
					...order.map((id) => {
						const o = sheets.find((s) => s.id === id)!;
						return Math.hypot(x.cx - o.cx, x.cy - o.cy);
					}),
				);
			return d(t) > d(best) ? t : best;
		});
	}
	return order;
})();

/** The sheets lit by `n` projects found: the first n of the fixed order (monotone, so nothing re-lights as the scan streams). */
export const litSheets = (n: number): number[] =>
	SHEETS.slice(0, Math.max(0, Math.min(SHEETS.length, Math.floor(n))));

/** Brightness of an unlit tile on the resting globe, and of a lit sheet once its flare has passed. */
const DIM = 0.08;
const LIT = 0.5;
const FLARE = 0.5;
const FLARE_SECONDS = 0.9;
/** The completion hit: how long its flash and ring last. */
const HIT_FLASH = 0.5;
const HIT_RING = 0.9;

function whole(glow: (id: number, kind: "sheet" | "rim") => number): TileState[] {
	return TILES.map((t) => ({ id: t.id, alpha: 1, glow: glow(t.id, t.kind) }));
}

const base = (tiles: TileState[]): Frame => ({
	// A beat after the hit, where the camera's punch has decayed to rest.
	t: BEATS.hit + 1,
	tiles,
	flash: 0,
	ring: 0,
	markScale: 1,
});

/** The globe while projects arrive. `litAt` is when each sheet was lit (seconds, same clock as `now`). */
export function progressFrame(s: {
	lit: number;
	doneAt: number | null;
	now: number;
	litAt: Record<number, number>;
}): Frame {
	const lit = new Set(litSheets(s.lit));
	const since = s.doneAt === null ? -1 : s.now - s.doneAt;
	const done = since >= 0;
	const tiles = whole((id, kind) => {
		if (done && kind === "sheet") return LIT + 0.5 * Math.max(0, 1 - since / 1.2);
		if (!lit.has(id)) return DIM;
		const age = s.now - (s.litAt[id] ?? -Infinity);
		return LIT + FLARE * Math.max(0, 1 - age / FLARE_SECONDS);
	});
	return {
		...base(tiles),
		flash: done ? Math.max(0, 1 - since / HIT_FLASH) ** 2 : 0,
		ring: done && since < HIT_RING ? since / HIT_RING : 0,
	};
}

/** The restrained empty state: the dim globe, a slow breath, and one tile glinting at a time. */
export function calm(time: number): Frame {
	const period = 3.2;
	const k = Math.floor(time / period);
	const phase = (time - k * period) / period;
	// A deterministic walk over the tiles: each period glints a different one.
	const pick = TILES[(k * 7 + 3) % TILES.length]!.id;
	const tiles = whole((id) => (id === pick ? DIM + 0.24 * Math.sin(Math.PI * phase) ** 2 : DIM));
	return { ...base(tiles), markScale: 1 + 0.006 * Math.sin((time / 6) * Math.PI * 2) };
}
