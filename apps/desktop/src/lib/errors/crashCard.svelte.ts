import { invoke } from "@tauri-apps/api/core";
import { parseCard, type CrashCard } from "./crashCard.ts";

/** The card for a crash of an earlier session, until it is dismissed. */
class CrashCards {
	card = $state<CrashCard | null>(null);

	/** Ask once at startup; a failing call is no card (the screen that would show it must never be the broken part). */
	async load() {
		this.card = parseCard(await invoke("crash_card_pending").catch(() => null));
	}

	/** The stamp moves to this card's time in `~/.atlas/desktop`, so it does not come back at the next launch. */
	async dismiss() {
		const c = this.card;
		this.card = null;
		if (c) await invoke("crash_card_dismiss", { atMs: c.atMs }).catch(() => {});
	}
}

export const crashCards = new CrashCards();
