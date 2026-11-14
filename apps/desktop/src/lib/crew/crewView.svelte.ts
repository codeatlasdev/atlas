/**
 * Crew view switcher state: 'chat' (default) or 'board'.
 * Persisted in localStorage so the chosen view survives reloads.
 */

const LS_KEY = "atlas.crewView";

type CrewView = "chat" | "board";

class CrewViewStore {
	// Only known views: a stale saved value (the removed "office") would render an empty panel.
	#value = $state<CrewView>(
		typeof localStorage !== "undefined" && localStorage.getItem(LS_KEY) === "board" ? "board" : "chat",
	);

	get value(): CrewView {
		return this.#value;
	}

	set value(v: CrewView) {
		this.#value = v;
		try {
			localStorage.setItem(LS_KEY, v);
		} catch {
			/* storage full */
		}
	}
}

export const crewView = new CrewViewStore();
