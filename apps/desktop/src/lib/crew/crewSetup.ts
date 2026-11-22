/**
 * Setup conversation state machine for a new crewmate.
 *
 * Pure: no side effects, no imports from Svelte or the store.
 * The CrewmateView setup overlay drives this to render questions and map answers
 * back to crewmate_update calls.
 *
 * Steps (persisted on the crewmate record as `setup_step`):
 *   "name" → "role" → "project" → "summary" → (cleared = done)
 *
 * There is no approval step: the crewmate starts in the crew's default permission mode (a crew setting) and the
 * person switches it from the message box. A crewmate saved at the old "approval" step resumes at "summary".
 *
 * The words are catalog messages, so the question and its options are built by functions that read them when the
 * conversation renders (the language can change while a setup is open).
 */
import type { PermissionMode } from "#lib/crew.ts";
import { modeLabel } from "#lib/crew/permissionMode.ts";
import { m } from "#lib/i18n/m.ts";

export type SetupStep = "name" | "role" | "project" | "summary";

/** The step a saved `setup_step` means: the removed "approval" step resumes at the summary; anything unknown is no step. */
export function normalizeStep(step: string | undefined): SetupStep | undefined {
	if (step === "approval") return "summary";
	return step === "name" || step === "role" || step === "project" || step === "summary" ? step : undefined;
}

export interface SetupOption {
	key: string;
	title: string;
	subtitle?: string;
	/** Picking it asks the user to type the answer ("Something else"); it is never the answer itself. */
	write?: true;
}

/** What picking an option does: answer with its title, or wait for the user to type. */
export function optionAnswer(option: SetupOption): { answer: string } | { write: true } {
	return option.write ? { write: true } : { answer: option.title };
}

export interface SetupQuestion {
	step: SetupStep;
	/** What the crewmate says. */
	text: string;
	/** Rich version of `text` (the summary, with bold names); shown instead of `text` when present. */
	parts?: SummaryPart[];
	/** Lettered options for the ask card; absent on free-text-only steps. */
	options?: SetupOption[];
	/** Whether to show the free-text composer input. */
	allowText: boolean;
	/** Suggested name chips shown below the text input (name step only). */
	nameChips?: string[];
}

export interface SetupContext {
	/** Collected name (may be the placeholder "New crewmate" until answered). */
	name?: string;
	/** Available project names from the IDE worktrees. */
	ideProjects?: string[];
}

// ─── crew name suggestions ────────────────────────────────────────────────────

/** Straw Hat crew names used as suggestions when no prior list exists. */
export const CREW_NAMES = ["Nami", "Zoro", "Usopp"] as const;

// ─── role options ─────────────────────────────────────────────────────────────

export const roleOptions = (): SetupOption[] => [
	{
		key: "A",
		title: m.crew_workspace_setup_role_review_title(),
		subtitle: m.crew_workspace_setup_role_review_subtitle(),
	},
	{
		key: "B",
		title: m.crew_workspace_setup_role_ci_title(),
		subtitle: m.crew_workspace_setup_role_ci_subtitle(),
	},
	{
		key: "C",
		title: m.crew_workspace_setup_role_changelog_title(),
		subtitle: m.crew_workspace_setup_role_changelog_subtitle(),
	},
	{
		key: "D",
		title: m.crew_workspace_setup_something_else(),
		subtitle: m.crew_workspace_setup_role_other_subtitle(),
		write: true,
	},
];

// ─── mapping helpers ──────────────────────────────────────────────────────────

/** Map role option key to a role string to store on the crewmate. */
export function roleKeyToText(key: string): string {
	switch (key) {
		case "A":
			return m.crew_workspace_setup_role_review_title();
		case "B":
			return m.crew_workspace_setup_role_ci_title();
		case "C":
			return m.crew_workspace_setup_role_changelog_title();
		default:
			return ""; // D = free text, caller supplies text directly
	}
}

// ─── question builder ─────────────────────────────────────────────────────────

/**
 * Return the question for a given setup step.
 * `ctx` shapes the question text and chip/option list.
 */
export function getQuestion(step: SetupStep, ctx: SetupContext = {}): SetupQuestion {
	switch (step) {
		case "name": {
			const chips = (ctx.ideProjects?.length ? ctx.ideProjects.slice(0, 3) : [...CREW_NAMES]) as string[];
			return {
				step,
				text: m.crew_workspace_setup_ask_name(),
				allowText: true,
				nameChips: chips,
			};
		}
		case "role":
			return {
				step,
				text: m.crew_workspace_setup_ask_role(),
				options: roleOptions(),
				allowText: true,
			};
		case "project": {
			if (ctx.ideProjects?.length) {
				const projectOptions: SetupOption[] = ctx.ideProjects
					.slice(0, 3)
					.map((name, i) => ({ key: String.fromCharCode(65 + i), title: name }));
				projectOptions.push({
					key: String.fromCharCode(65 + projectOptions.length),
					title: m.crew_workspace_setup_something_else(),
					subtitle: m.crew_workspace_setup_project_other_subtitle(),
					write: true,
				});
				return {
					step,
					text: m.crew_workspace_setup_ask_project(),
					options: projectOptions,
					allowText: true,
				};
			}
			return {
				step,
				text: m.crew_workspace_setup_ask_project(),
				options: undefined,
				allowText: true,
			};
		}
		case "summary":
			return {
				step,
				text: "", // caller fills from summaryText()
				options: [
					{ key: "A", title: m.crew_workspace_setup_confirm_title() },
					{
						key: "B",
						title: m.crew_workspace_setup_change_title(),
						subtitle: m.crew_workspace_setup_change_subtitle(),
					},
				],
				allowText: false,
			};
	}
}

// ─── step sequencing ──────────────────────────────────────────────────────────

/** The ordered step list. */
export const STEPS: SetupStep[] = ["name", "role", "project", "summary"];

/** Next step after `current`, or `"done"` when setup is complete. */
export function nextStep(current: SetupStep): SetupStep | "done" {
	const idx = STEPS.indexOf(current);
	if (idx < 0 || idx === STEPS.length - 1) return "done";
	return STEPS[idx + 1];
}

/** Step to restart from when the user picks "Change something" at the summary. */
export const RESTART_STEP: SetupStep = "name";

// ─── summary ──────────────────────────────────────────────────────────────────

export interface SummaryState {
	name: string;
	roleText: string;
	project?: string;
	/** The mode the crewmate starts in (the crew's default); the summary names it. */
	mode: PermissionMode;
}

/** A run of the summary bubble; the chat renders `bold` runs in bold (never markdown, which printed its stars). */
export interface SummaryPart {
	text: string;
	bold?: true;
}

/** Marks the values the summary shows in bold; the message is one sentence, so word order stays the translator's. */
const BOLD = "\u2063";
const bolded = (value: string) => `${BOLD}${value.replaceAll(BOLD, "")}${BOLD}`;

/** The summary bubble shown before confirmation. */
export function summaryText(s: SummaryState): SummaryPart[] {
	const mode = modeLabel(s.mode);
	// Only the first letter drops to lower case: "Review every PR" reads "review every PR", not "every pr".
	const role = s.roleText.charAt(0).toLowerCase() + s.roleText.slice(1);
	// A GitHub URL reads as owner/repo; a name or path stays as given.
	const project = s.project?.replace(/^https?:\/\/github\.com\/|\.git$/g, "");
	const text = project
		? m.crew_workspace_setup_summary_project({
				name: bolded(s.name),
				role,
				project: bolded(project),
				mode: bolded(mode),
			})
		: m.crew_workspace_setup_summary({ name: bolded(s.name), role, mode: bolded(mode) });
	return text
		.split(BOLD)
		.map((part, i): SummaryPart => (i % 2 ? { text: part, bold: true } : { text: part }))
		.filter((part) => part.text !== "");
}

// ─── typing delay ─────────────────────────────────────────────────────────────

/**
 * Simulated typing delay in ms for the crewmate's next message.
 * Varies between 800–1200 ms; pass a seed for reproducibility in tests.
 */
export function typingDelay(seed = Math.random()): number {
	return 800 + Math.round(seed * 400);
}
