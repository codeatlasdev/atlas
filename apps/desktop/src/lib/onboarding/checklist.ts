import { m } from "#lib/i18n/m.ts";

/** Getting-started checklist: items, persisted state shape and pure transitions. */

export type ChecklistId = "project" | "stack" | "terminal" | "agent" | "palette" | "file";

export interface ChecklistItem {
	id: ChecklistId;
	/** In the display language; read when it is drawn, so a language switch shows at once. */
	readonly label: string;
	/** Command that does it (shown with its key and runnable from the card). */
	command?: string;
}

export const CHECKLIST: ChecklistItem[] = [
	{
		id: "project",
		get label() {
			return m.onboarding_checklist_item_project();
		},
		command: "atlas::AddProject",
	},
	{
		id: "stack",
		get label() {
			return m.onboarding_checklist_item_stack();
		},
		command: "atlas::StartStack",
	},
	{
		id: "terminal",
		get label() {
			return m.onboarding_checklist_item_terminal();
		},
		command: "workspace::NewTerminal",
	},
	{
		id: "agent",
		get label() {
			return m.onboarding_checklist_item_agent();
		},
		command: "atlas::NewAgentSession",
	},
	{
		id: "palette",
		get label() {
			return m.onboarding_checklist_item_palette();
		},
		command: "command_palette::Toggle",
	},
	{
		id: "file",
		get label() {
			return m.onboarding_checklist_item_file();
		},
		command: "file_finder::Toggle",
	},
];

/** What is persisted under `onboarding` in settings.json. */
export interface OnboardingState {
	/** First-run welcome and tour were completed or skipped. */
	done: boolean;
	/** The tour ran (finished or skipped) at least once. */
	toured: boolean;
	checklist: { done: ChecklistId[]; dismissed: boolean };
}

export const EMPTY_STATE: OnboardingState = {
	done: false,
	toured: false,
	checklist: { done: [], dismissed: false },
};

const IDS: Record<string, true> = Object.fromEntries(CHECKLIST.map((c) => [c.id, true]));

/** Parse whatever settings.json holds into a valid state (unknown ids dropped). */
export function parseState(raw: unknown): OnboardingState {
	if (!raw || typeof raw !== "object") return structuredClone(EMPTY_STATE);
	const r = raw as Record<string, unknown>;
	const c = (r.checklist && typeof r.checklist === "object" ? r.checklist : {}) as Record<string, unknown>;
	const done = Array.isArray(c.done)
		? [...new Set(c.done.filter((x): x is ChecklistId => typeof x === "string" && x in IDS))]
		: [];
	return {
		done: r.done === true,
		toured: r.toured === true,
		checklist: { done, dismissed: c.dismissed === true },
	};
}

/**
 * The state to start from, given the outcome of reading settings. A read that failed is NOT a first run: showing the
 * deck to a user whose settings could not be read is the path that wipes them (and with no state at all `loaded`
 * never became true, so the app sat without overlay, hero or checklist). Welcome counts as done in memory only; the
 * next successful save is a patch of that one key, never a rewrite from this fallback.
 */
export function stateFromSettings(
	read: { settings: Record<string, unknown> } | { error: string },
): OnboardingState {
	if ("error" in read) return { ...structuredClone(EMPTY_STATE), done: true };
	return parseState(read.settings.onboarding);
}

/** `state` with `id` completed; same object when it already was. */
export function complete(state: OnboardingState, id: ChecklistId): OnboardingState {
	if (state.checklist.done.includes(id)) return state;
	return { ...state, checklist: { ...state.checklist, done: [...state.checklist.done, id] } };
}

export function progress(state: OnboardingState): { done: number; total: number; all: boolean } {
	const done = CHECKLIST.filter((c) => state.checklist.done.includes(c.id)).length;
	return { done, total: CHECKLIST.length, all: done === CHECKLIST.length };
}

/** The first item still to do, to highlight as "next". */
export function nextItem(state: OnboardingState): ChecklistItem | undefined {
	return CHECKLIST.find((c) => !state.checklist.done.includes(c.id));
}

/** Command ids whose run completes a checklist item (palette and key presses). */
const COMMAND_ITEMS: Record<string, ChecklistId> = {
	"command_palette::Toggle": "palette",
	"atlas::Jump": "palette",
	"file_finder::Toggle": "file",
};

/**
 * Items completed by a `recent` command list change: ids that appear at the front
 * of `next` and were not at the front of `prev`.
 */
export function itemsFromRecent(prev: string[], next: string[]): ChecklistId[] {
	if (!next.length || next[0] === prev[0]) return [];
	const id = COMMAND_ITEMS[next[0]];
	return id ? [id] : [];
}
