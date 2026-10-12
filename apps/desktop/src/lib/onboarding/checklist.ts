/** Getting-started checklist: items, persisted state shape and pure transitions. */

export type ChecklistId = "project" | "stack" | "terminal" | "agent" | "palette" | "file";

export interface ChecklistItem {
	id: ChecklistId;
	label: string;
	/** Command that does it (shown with its key and runnable from the card). */
	command?: string;
}

export const CHECKLIST: ChecklistItem[] = [
	{ id: "project", label: "Add a project", command: "atlas::AddProject" },
	{ id: "stack", label: "Start the stack", command: "atlas::StartStack" },
	{ id: "terminal", label: "Open a terminal", command: "workspace::NewTerminal" },
	{ id: "agent", label: "Run an agent", command: "atlas::NewAgentSession" },
	{ id: "palette", label: "Try the command palette", command: "command_palette::Toggle" },
	{ id: "file", label: "Search a file", command: "file_finder::Toggle" },
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

/**
 * First run = nothing added yet and the welcome was never finished. Waits for the
 * project list to load so a returning user never flashes the welcome.
 */
export function isFirstRun(state: OnboardingState, loaded: boolean, projectCount: number): boolean {
	return loaded && projectCount === 0 && !state.done;
}

/** Command ids whose run completes a checklist item (palette and key presses). */
export const COMMAND_ITEMS: Record<string, ChecklistId> = {
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
