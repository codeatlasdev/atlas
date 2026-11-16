/**
 * The bottom-right corner as one decision. Every card in it (kit toasts, error toasts, action results, the Getting
 * started checklist, suggestions such as "Enable Claude Code integration") is a claim; `planCorner` says which
 * claims are on screen, in which order, which one folds to its header and which one waits. Pure, no DOM: the shell
 * (`CornerStack.svelte`, `CornerCard.svelte`) draws exactly this, and the tests pin it.
 *
 * Priority, nearest the corner first:
 *   1. toasts: errors and anything that needs action now outrank plain ones; on screen newest first, at most
 *      `MAX_TOASTS`, the rest fold into one "+N more" row;
 *   2. the Getting started checklist, folded to its header while any toast is up;
 *   3. one suggestion (arrival order), only while no toast is up: it waits its turn.
 */

export type CardKind = "alert" | "toast" | "checklist" | "suggestion";

export interface Claim {
	id: string;
	kind: CardKind;
	/** Arrival order: a larger number arrived later. */
	seq: number;
}

/** Most toasts on screen before the rest fold into the "+N more" row. */
export const MAX_TOASTS = 3;

/** Id of the "+N more" / "Show less" row in `CornerPlan.order`. */
export const MORE_ID = "corner:more";

export interface CornerUi {
	/** The user's own choice for the checklist (open or closed); a toast never rewrites it. */
	checklistOpen: boolean;
	/** The user opened the checklist while a toast was up: it stays open until the corner is free. */
	checklistPinned: boolean;
	/** The "+N more" row was opened. */
	toastsExpanded: boolean;
}

export interface CornerPlan {
	/** What is on screen, nearest the corner first. Includes `MORE_ID` when the row shows. */
	order: string[];
	/** The toasts on screen, nearest the corner first. */
	toasts: string[];
	/** Toasts folded into the row. */
	hiddenToasts: number;
	/** `more`: "+N more"; `less`: "Show less" (everything is open); null: no row. */
	moreRow: "more" | "less" | null;
	checklist: { id: string; collapsed: boolean } | null;
	/** The one suggestion on screen. */
	suggestion: string | null;
	/** Suggestions waiting for their turn (not on screen), in arrival order. */
	queued: string[];
}

/** Errors first: they are the ones never folded away. */
const rank = (c: Claim) => (c.kind === "alert" ? 0 : 1);

export function planCorner(claims: Claim[], ui: CornerUi): CornerPlan {
	const toasts = claims.filter((c) => c.kind === "alert" || c.kind === "toast");
	let shown = toasts;
	if (!ui.toastsExpanded && toasts.length > MAX_TOASTS)
		shown = [...toasts].sort((a, b) => rank(a) - rank(b) || b.seq - a.seq).slice(0, MAX_TOASTS);
	const visible = [...shown].sort((a, b) => b.seq - a.seq).map((c) => c.id);
	const hiddenToasts = toasts.length - visible.length;
	const moreRow = hiddenToasts > 0 ? "more" : ui.toastsExpanded && toasts.length > MAX_TOASTS ? "less" : null;

	const list = claims.filter((c) => c.kind === "checklist").sort((a, b) => a.seq - b.seq)[0];
	const checklist = list
		? { id: list.id, collapsed: !ui.checklistOpen || (toasts.length > 0 && !ui.checklistPinned) }
		: null;

	const waiting = claims.filter((c) => c.kind === "suggestion").sort((a, b) => a.seq - b.seq);
	const suggestion = toasts.length === 0 ? (waiting[0]?.id ?? null) : null;
	const queued = waiting.filter((c) => c.id !== suggestion).map((c) => c.id);

	const order = [...visible];
	if (moreRow) order.push(MORE_ID);
	if (checklist) order.push(checklist.id);
	if (suggestion) order.push(suggestion);
	return { order, toasts: visible, hiddenToasts, moreRow, checklist, suggestion, queued };
}

/**
 * FLIP: a card whose resting place moved from `prev` to `cur` (both measured as the distance of its bottom edge
 * from the corner) starts its glide at the old spot. `visual` is the offset it still shows from a glide in
 * progress, so a second move does not jump. A move under half a pixel is not worth animating.
 */
export function flipStart(prev: number, cur: number, visual: number): number {
	return Math.abs(cur - prev) < 0.5 ? 0 : cur - prev + visual;
}
