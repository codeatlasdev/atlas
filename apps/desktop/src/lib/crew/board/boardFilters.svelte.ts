/**
 * Board filters: the toolbar state (crewmate chips, source, search, done range), persisted in localStorage, and the
 * values derived from it (which chips exist, which picks are live, the options `boardColumns` takes).
 */
import type { Crewmate } from "#lib/crew.ts";
import { crewmateKey } from "#lib/crew/crewmateKey.ts";
import type { BoardColumn, BoardFilterOptions } from "#lib/crew/board.ts";

const FILTERS_KEY = "crew-board-filters";

export type BoardSource = "all" | "chat" | "auto";

interface PersistedFilters {
	selectedCrewmates: string[];
	source: BoardSource;
	query: string;
	showDone: boolean;
	doneDays: 1 | 7;
}

function loadFilters(): PersistedFilters {
	try {
		const raw = localStorage.getItem(FILTERS_KEY);
		if (raw) {
			const p = JSON.parse(raw) as Partial<PersistedFilters>;
			return {
				selectedCrewmates: Array.isArray(p.selectedCrewmates) ? p.selectedCrewmates : [],
				source: p.source === "chat" || p.source === "auto" ? p.source : "all",
				query: typeof p.query === "string" ? p.query : "",
				showDone: p.showDone !== false,
				doneDays: p.doneDays === 7 ? 7 : 1,
			};
		}
	} catch {}
	return { selectedCrewmates: [], source: "all", query: "", showDone: true, doneDays: 1 };
}

export class BoardFilters {
	#saved = loadFilters();
	selectedCrewmates = $state(new Set<string>(this.#saved.selectedCrewmates));
	source = $state<BoardSource>(this.#saved.source);
	query = $state(this.#saved.query);
	showDone = $state(this.#saved.showDone);
	doneDays = $state<1 | 7>(this.#saved.doneDays);

	/** The unfiltered board columns (to know which crewmates have cards). */
	#columns: () => BoardColumn[];

	constructor(columns: () => BoardColumn[]) {
		this.#columns = columns;
	}

	/** Persist on every change; call once from a component's `$effect`. */
	save() {
		try {
			localStorage.setItem(
				FILTERS_KEY,
				JSON.stringify({
					selectedCrewmates: [...this.selectedCrewmates],
					source: this.source,
					query: this.query,
					showDone: this.showDone,
					doneDays: this.doneDays,
				}),
			);
		} catch {}
	}

	/** Crewmates that actually have at least one card (they get a chip). */
	availableCrewmates = $derived.by(() => {
		const seen = new Map<string, { hostKey: string; crewmate: Crewmate }>();
		for (const col of this.#columns()) {
			for (const card of col.cards) {
				const k = crewmateKey(card.hostKey, card.crewmate.id);
				if (!seen.has(k)) seen.set(k, { hostKey: card.hostKey, crewmate: card.crewmate });
			}
		}
		return [...seen.values()];
	});

	// Only the crewmates that have a chip on screen count. A saved pick whose crewmate lost its cards (or was deleted) had no
	// chip to unselect yet still filtered everything out, which left an empty board with no visible filter.
	activeCrewmates = $derived(
		new Set(
			[...this.selectedCrewmates].filter((k) =>
				this.availableCrewmates.some((b) => crewmateKey(b.hostKey, b.crewmate.id) === k),
			),
		),
	);

	options = $derived<BoardFilterOptions>({
		...(this.activeCrewmates.size > 0 ? { crewmates: this.activeCrewmates } : {}),
		...(this.source !== "all" ? { source: this.source } : {}),
		...(this.query.trim() ? { query: this.query.trim() } : {}),
		doneDays: this.showDone ? this.doneDays : 1,
	});

	hasActive = $derived(
		this.activeCrewmates.size > 0 || this.source !== "all" || this.query.trim().length > 0,
	);

	toggleCrewmate(key: string) {
		const next = new Set(this.selectedCrewmates);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		this.selectedCrewmates = next;
	}

	clear() {
		this.selectedCrewmates = new Set();
		this.source = "all";
		this.query = "";
	}
}
