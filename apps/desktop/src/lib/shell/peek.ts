/**
 * Edge-peek intent for a hidden sidebar (Arc). Pure: the caller owns timers and the clock.
 *
 * - Open: the pointer must dwell in the hot zone for `OPEN_DELAY`, except a fast flick into the
 *   edge (speed ≥ `FLICK_SPEED` px/ms) which opens at once.
 * - Close: once out of the panel, a grace of `CLOSE_DELAY`. The "inside" region is wider than the
 *   one that opens it (panel width + `HYSTERESIS`), so the pointer skimming the panel border does
 *   not flicker it.
 */

export const HOT_ZONE = 6;
export const OPEN_DELAY = 60;
export const CLOSE_DELAY = 250;
export const HYSTERESIS = 24;
export const FLICK_SPEED = 1.2;

export type Edge = "left" | "right";

/** Distance of `x` from the `edge` of a window `width` wide (0 = on the edge, grows inward). */
export function depthFrom(edge: Edge, x: number, width: number): number {
	return edge === "left" ? x : width - x;
}

/** Is a pointer at `x` inside the hot zone of `edge` in a window `width` wide? */
export function inHotZone(edge: Edge, x: number, width: number, zone = HOT_ZONE): boolean {
	return depthFrom(edge, x, width) <= zone;
}

/** Is `x` over the open panel (`size` px wide, plus `slack` so its border does not flicker)? */
export function inPanel(edge: Edge, x: number, width: number, size: number, slack = HYSTERESIS): boolean {
	return depthFrom(edge, x, width) <= size + slack;
}

export type PeekEvent =
	| {
			t: "move";
			hot: boolean;
			over: boolean;
			/** px/ms toward the edge (positive = approaching). */ speed?: number;
	  }
	| { t: "leave-window" }
	| { t: "force-close" };

export interface PeekState {
	open: boolean;
}

/** What the caller must do after an event: schedule/cancel a timer, or change state now. */
export type PeekAction =
	| { do: "schedule-open" }
	| { do: "open-now" }
	| { do: "schedule-close" }
	| { do: "cancel" }
	| { do: "close-now" }
	| { do: "none" };

/** Pure transition: given the current state and a pointer event, the action to take. */
export function peekAction(s: PeekState, e: PeekEvent): PeekAction {
	switch (e.t) {
		case "force-close":
			return s.open ? { do: "close-now" } : { do: "cancel" };
		case "leave-window":
			// Leaving through the sidebar's own edge must not close it: the pointer is on the border.
			return s.open ? { do: "schedule-close" } : { do: "cancel" };
		case "move":
			if (s.open) return e.over || e.hot ? { do: "cancel" } : { do: "schedule-close" };
			if (!e.hot) return { do: "cancel" };
			return (e.speed ?? 0) >= FLICK_SPEED ? { do: "open-now" } : { do: "schedule-open" };
	}
}

/**
 * Hot-path guard: what a `pointermove` can possibly change. Returns `true` only when the
 * pointer crossed a region boundary (hot / over), so the handler can bail out in O(1) on the
 * thousands of moves that stay inside one region without touching reactive state or timers.
 */
export interface PeekRegion {
	hot: boolean;
	over: boolean;
}

export function regionOf(edge: Edge, x: number, width: number, size: number, open: boolean): PeekRegion {
	return {
		hot: inHotZone(edge, x, width),
		// Hysteresis: an open panel keeps the pointer for `size + HYSTERESIS`; a closed one only reports "over" in the zone.
		over: open ? inPanel(edge, x, width, size) : false,
	};
}

export const sameRegion = (a: PeekRegion, b: PeekRegion) => a.hot === b.hot && a.over === b.over;
