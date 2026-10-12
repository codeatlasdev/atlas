/** Guided tour: step list and pure sequencing (anchors are `[data-tour="<anchor>"]`). */

export interface TourStep {
	id: string;
	/** `data-tour` value of the element to spotlight. */
	anchor: string;
	title: string;
	body: string;
	/** Preferred card side relative to the anchor. */
	side: "right" | "left" | "bottom" | "top";
}

export const TOUR_STEPS: TourStep[] = [
	{
		id: "sidebar",
		anchor: "sidebar",
		title: "Your projects live here",
		body: "Every project is a space with its own color. Pick one to switch, and its worktrees sit right under it.",
		side: "right",
	},
	{
		id: "spaces",
		anchor: "spaces",
		title: "Switch spaces",
		body: "Each space remembers its tabs, panes and stack, so switching never loses your place.",
		side: "right",
	},
	{
		id: "breadcrumb",
		anchor: "breadcrumb",
		title: "Know where you are",
		body: "Project, worktree and branch at a glance. Click a part to jump somewhere else.",
		side: "bottom",
	},
	{
		id: "panel-kpis",
		anchor: "panel-kpis",
		title: "The stack, at a glance",
		body: "Health, services and resources for this project. Start the stack from the header when you are ready.",
		side: "bottom",
	},
	{
		id: "service-card",
		anchor: "service-card",
		title: "Every service, live",
		body: "State, CPU and memory update as things run. Click a card to open its logs or restart it.",
		side: "top",
	},
	{
		id: "right-toggle",
		anchor: "right-toggle",
		title: "Details when you want them",
		body: "The right sidebar holds agent history and ports. Hide it for a calmer view; it comes back with one key.",
		side: "bottom",
	},
	{
		id: "palette",
		anchor: "palette",
		title: "Go anywhere with one key",
		body: "Jump to a worktree, terminal, file or command by typing a few letters.",
		side: "bottom",
	},
];

/** Steps whose anchor is currently on screen, in tour order. */
export function visibleSteps(steps: TourStep[], has: (anchor: string) => boolean): TourStep[] {
	return steps.filter((s) => has(s.anchor));
}

/** Index after `i`, or `null` past the last step (= finish). */
export function nextIndex(i: number, length: number): number | null {
	return i + 1 < length ? i + 1 : null;
}

/** Index before `i`; stays on the first step. */
export function prevIndex(i: number): number {
	return Math.max(0, i - 1);
}

/**
 * Keep the user on the same step when the visible list changes (resize, a dock
 * opening): index of `currentId` in `steps`, else the closest valid index.
 */
export function reanchor(steps: TourStep[], currentId: string | undefined, fallback: number): number {
	const i = currentId ? steps.findIndex((s) => s.id === currentId) : -1;
	return i >= 0 ? i : Math.min(Math.max(0, fallback), Math.max(0, steps.length - 1));
}

export interface Box {
	x: number;
	y: number;
	w: number;
	h: number;
}

/**
 * Card position next to `target`, flipped to the opposite side when the preferred
 * one does not fit, then clamped inside the viewport.
 */
export function placeCard(
	target: Box,
	card: { w: number; h: number },
	view: { w: number; h: number },
	side: TourStep["side"],
	gap = 14,
	margin = 12,
): { x: number; y: number; side: TourStep["side"] } {
	const room = {
		right: view.w - (target.x + target.w) - gap - margin,
		left: target.x - gap - margin,
		bottom: view.h - (target.y + target.h) - gap - margin,
		top: target.y - gap - margin,
	};
	const need = side === "left" || side === "right" ? card.w : card.h;
	const opposite = { right: "left", left: "right", bottom: "top", top: "bottom" } as const;
	let s = side;
	if (room[s] < need && room[opposite[s]] >= need) s = opposite[s];
	let x: number;
	let y: number;
	if (s === "right" || s === "left") {
		x = s === "right" ? target.x + target.w + gap : target.x - gap - card.w;
		y = target.y + target.h / 2 - card.h / 2;
	} else {
		y = s === "bottom" ? target.y + target.h + gap : target.y - gap - card.h;
		x = target.x + target.w / 2 - card.w / 2;
	}
	x = Math.min(Math.max(margin, x), Math.max(margin, view.w - card.w - margin));
	y = Math.min(Math.max(margin, y), Math.max(margin, view.h - card.h - margin));
	return { x, y, side: s };
}
