/**
 * The corner's one motion: a card fades and settles in (`cornerFlow`), fades out of the flow, and every card that
 * had to move glides to its new place (FLIP: the layout jumps first, then each card animates from where it was).
 * Reduce motion: nothing animates, the layout simply changes. Nothing here touches the page underneath: the corner
 * is its own fixed column.
 */
import { cubicIn, cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";
import { flipStart } from "./cornerPlan.ts";
import { CORNER_ID } from "./cornerStack.ts";

export const ENTER_MS = 220;
export const LEAVE_MS = 150;
export const GLIDE_MS = 220;
/** Offset a card travels while it fades in or out. */
const TRAVEL_PX = 8;

/** The user's OS setting, read when an animation starts so changing it takes effect without a restart. */
export function reducedMotion(): boolean {
	return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** The wrapper `CornerCard` puts around a card: the unit that is placed, measured and glided. */
function slotOf(node: Element): HTMLElement | null {
	return node.closest<HTMLElement>("[data-corner-card]");
}

/**
 * A leaving card stops taking room at once (so the others glide into the gap while it fades) but stays where it is
 * until the fade ends.
 */
function leaveFlow(slot: HTMLElement) {
	slot.dataset.leaving = "";
	slot.style.position = "absolute";
	slot.style.top = `${slot.offsetTop}px`;
	slot.style.left = `${slot.offsetLeft}px`;
	slot.style.width = `${slot.offsetWidth}px`;
	slot.style.pointerEvents = "none";
}

/** Svelte transition for the card inside a slot; use as `in:cornerFlow|global` and `out:cornerFlow={{ out: true }}|global`. */
export function cornerFlow(node: Element, { out = false }: { out?: boolean } = {}): TransitionConfig {
	const slot = slotOf(node);
	if (out && slot) leaveFlow(slot);
	requestFlip();
	if (reducedMotion()) return { duration: 0 };
	return {
		duration: out ? LEAVE_MS : ENTER_MS,
		easing: out ? cubicIn : cubicOut,
		css: (t, u) => `opacity: ${t}; transform: translateY(${u * TRAVEL_PX}px) scale(${1 - u * 0.03});`,
	};
}

/** Where each card rested the last time the corner was laid out: distance of its bottom edge from the corner. */
const rested = new Map<string, number>();
let queued = false;

/** Lay out is done (or about to be): glide whatever moved. Coalesced to once per microtask, before paint. */
export function requestFlip() {
	if (queued) return;
	queued = true;
	queueMicrotask(flip);
}

/** The vertical offset a card still shows from a glide in progress. */
function shownOffset(el: HTMLElement): number {
	const t = getComputedStyle(el).transform;
	return t && t !== "none" && typeof DOMMatrix === "function" ? new DOMMatrix(t).m42 : 0;
}

function flip() {
	queued = false;
	const host = document.getElementById(CORNER_ID);
	if (!host) return rested.clear();
	const animate = !reducedMotion();
	const seen = new Set<string>();
	for (const el of host.querySelectorAll<HTMLElement>("[data-corner-card]:not([data-leaving])")) {
		const id = el.dataset.cornerCard ?? "";
		seen.add(id);
		const bottom = host.offsetHeight - (el.offsetTop + el.offsetHeight);
		const was = rested.get(id);
		rested.set(id, bottom);
		if (!animate || was === undefined || typeof el.animate !== "function") continue;
		const from = flipStart(was, bottom, shownOffset(el));
		if (from === 0) continue;
		for (const a of el.getAnimations?.() ?? []) a.cancel();
		el.animate([{ transform: `translateY(${from}px)` }, { transform: "none" }], {
			duration: GLIDE_MS,
			easing: "cubic-bezier(0.2, 0.8, 0.2, 1)",
		});
	}
	for (const id of rested.keys()) if (!seen.has(id)) rested.delete(id);
}
