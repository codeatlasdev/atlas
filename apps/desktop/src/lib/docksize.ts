/** Dock sizing rules, pure for tests. */

export type DockSide = "left" | "right" | "bottom";

export const DOCK_LIMITS = {
	left: { min: 180, max: 520 },
	right: { min: 220, max: 560 },
	bottom: { min: 80, max: Infinity },
} as const;

/** Size restored by double-clicking a resize handle. */
export const DOCK_DEFAULTS: Record<DockSide, number> = { left: 260, right: 300, bottom: 260 };

/** Clamp a persisted or dragged size; anything non-numeric falls back to the minimum. */
export function clampSize(side: DockSide, px: unknown, ceiling = Infinity): number {
	const { min, max } = DOCK_LIMITS[side];
	const n = typeof px === "number" && Number.isFinite(px) ? px : min;
	return Math.round(Math.min(Math.max(n, min), Math.min(max, Math.max(ceiling, min))));
}
