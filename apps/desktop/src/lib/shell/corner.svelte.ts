/**
 * The corner coordinator's state. Cards (`CornerCard.svelte`) claim a place while they are mounted; `plan` is
 * `planCorner` over the live claims and the user's choices, and every card draws only what the plan says.
 * Nothing here knows about the DOM.
 */
import { MAX_TOASTS, planCorner, type CardKind, type Claim } from "./cornerPlan.ts";

/** localStorage key of the user's choice for the Getting started card (kept from before the coordinator). */
const CHECKLIST_KEY = "atlas.checklistOpen";

function savedOpen(): boolean {
	try {
		return localStorage.getItem(CHECKLIST_KEY) !== "0";
	} catch {
		return true;
	}
}

class Corner {
	#seq = 0;
	/** How many live cards hold each id: a card that re-mounts claims before the old one releases. */
	#holds = new Map<string, number>();
	claims = $state<Claim[]>([]);
	checklistOpen = $state(savedOpen());
	checklistPinned = $state(false);
	toastsExpanded = $state(false);
	plan = $derived(planCorner(this.claims, this));

	/** Take a place in the corner; returns the release. A claim made later is newer. */
	claim(id: string, kind: CardKind): () => void {
		this.#holds.set(id, (this.#holds.get(id) ?? 0) + 1);
		if (!this.claims.some((c) => c.id === id)) this.claims.push({ id, kind, seq: ++this.#seq });
		let released = false;
		return () => {
			if (released) return;
			released = true;
			const left = (this.#holds.get(id) ?? 1) - 1;
			if (left > 0) return this.#holds.set(id, left);
			this.#holds.delete(id);
			this.claims = this.claims.filter((c) => c.id !== id);
			this.#settle();
		};
	}

	/** The pin and the opened row only last while there is something to pin against or to fold. */
	#settle() {
		const toasts = this.claims.filter((c) => c.kind === "alert" || c.kind === "toast").length;
		if (toasts === 0) this.checklistPinned = false;
		if (toasts <= MAX_TOASTS) this.toastsExpanded = false;
	}

	/** Distance from the corner in the plan (0 = nearest); -1 when the card is not on screen. */
	slot(id: string): number {
		return this.plan.order.indexOf(id);
	}

	shown(id: string): boolean {
		return this.slot(id) >= 0;
	}

	/**
	 * The checklist header. Folded by a toast: opens it over the toast (pinned). Closed by the user: opens it.
	 * Open and showing: closes it, which is the user's choice and survives toasts.
	 */
	toggleChecklist() {
		const collapsed = this.plan.checklist?.collapsed ?? false;
		if (!collapsed) {
			this.checklistOpen = false;
			this.checklistPinned = false;
		} else {
			this.checklistOpen = true;
			this.checklistPinned = this.plan.toasts.length > 0;
		}
		try {
			localStorage.setItem(CHECKLIST_KEY, this.checklistOpen ? "1" : "0");
		} catch {
			// private mode: the choice lasts for the session
		}
	}

	toggleMore() {
		this.toastsExpanded = !this.toastsExpanded;
	}

	/** Tests: forget every claim and choice. */
	reset() {
		this.#holds.clear();
		this.claims = [];
		this.checklistOpen = true;
		this.checklistPinned = false;
		this.toastsExpanded = false;
		try {
			localStorage.removeItem(CHECKLIST_KEY);
		} catch {
			// nothing stored
		}
	}
}

export const corner = new Corner();
