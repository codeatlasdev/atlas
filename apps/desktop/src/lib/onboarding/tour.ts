import { m } from "#lib/i18n/m.ts";

/** Guided tour: step list and pure sequencing (anchors are `[data-tour="<anchor>"]`). */

export interface TourStep {
	id: string;
	/** `data-tour` value of the element to spotlight. */
	anchor: string;
	/** The copy is read when the card is drawn (a getter), so a language switch shows at once. */
	readonly title: string;
	readonly body: string;
	/** Preferred card side relative to the anchor. */
	side: "right" | "left" | "bottom" | "top";
}

export const TOUR_STEPS: TourStep[] = [
	{
		id: "sidebar",
		anchor: "sidebar",
		get title() {
			return m.onboarding_tour_sidebar_title();
		},
		get body() {
			return m.onboarding_tour_sidebar_body();
		},
		side: "right",
	},
	{
		id: "spaces",
		anchor: "spaces",
		get title() {
			return m.onboarding_tour_spaces_title();
		},
		get body() {
			return m.onboarding_tour_spaces_body();
		},
		side: "right",
	},
	{
		id: "breadcrumb",
		anchor: "breadcrumb",
		get title() {
			return m.onboarding_tour_breadcrumb_title();
		},
		get body() {
			return m.onboarding_tour_breadcrumb_body();
		},
		side: "bottom",
	},
	{
		id: "panel-kpis",
		anchor: "panel-kpis",
		get title() {
			return m.onboarding_tour_panel_kpis_title();
		},
		get body() {
			return m.onboarding_tour_panel_kpis_body();
		},
		side: "bottom",
	},
	{
		id: "service-card",
		anchor: "service-card",
		get title() {
			return m.onboarding_tour_service_card_title();
		},
		get body() {
			return m.onboarding_tour_service_card_body();
		},
		side: "top",
	},
	{
		id: "right-toggle",
		anchor: "right-toggle",
		get title() {
			return m.onboarding_tour_right_toggle_title();
		},
		get body() {
			return m.onboarding_tour_right_toggle_body();
		},
		side: "bottom",
	},
	{
		id: "palette",
		anchor: "palette",
		get title() {
			return m.onboarding_tour_palette_title();
		},
		get body() {
			return m.onboarding_tour_palette_body();
		},
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
