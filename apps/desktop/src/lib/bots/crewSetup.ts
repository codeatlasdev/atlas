/**
 * Setup conversation state machine for a new crewmate.
 *
 * Pure: no side effects, no imports from Svelte or the store.
 * The BotView setup overlay drives this to render questions and map answers
 * back to bot_update calls.
 *
 * Steps (persisted on the bot record as `setup_step`):
 *   "name" → "role" → "project" → "approval" → "summary" → (cleared = done)
 */

export type SetupStep = "name" | "role" | "project" | "approval" | "summary";

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
	/** What the bot says. */
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

export const ROLE_OPTIONS: SetupOption[] = [
	{ key: "A", title: "Review every PR", subtitle: "Reads diffs, leaves comments" },
	{ key: "B", title: "Fix failing CI", subtitle: "Debugs and patches broken builds" },
	{ key: "C", title: "Write the changelog", subtitle: "Summarises commits into release notes" },
	{ key: "D", title: "Something else", subtitle: "You describe it", write: true },
];

// ─── approval options ─────────────────────────────────────────────────────────

export const APPROVAL_OPTIONS: SetupOption[] = [
	{ key: "A", title: "Ask before anything risky", subtitle: "Recommended" },
	{ key: "B", title: "Ask before everything" },
	{ key: "C", title: "Never ask", subtitle: "Full auto" },
];

// ─── mapping helpers ──────────────────────────────────────────────────────────

/** Map role option key to a role string to store on the bot. */
export function roleKeyToText(key: string): string {
	switch (key) {
		case "A":
			return "Review every PR";
		case "B":
			return "Fix failing CI";
		case "C":
			return "Write the changelog";
		default:
			return ""; // D = free text, caller supplies text directly
	}
}

/** Map approval option key to the bot's approval mode. */
export function approvalKeyToMode(key: string): "ask" | "auto" {
	// A = ask before risky → "ask"; B = ask before everything → "ask"; C = never → "auto"
	return key === "C" ? "auto" : "ask";
}

/** Human-readable description of the chosen approval mode for the summary bubble. */
export function approvalDescription(key: string): string {
	switch (key) {
		case "A":
			return "ask before anything risky";
		case "B":
			return "ask before every step";
		default:
			return "work autonomously";
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
				text: "What should I call myself?",
				allowText: true,
				nameChips: chips,
			};
		}
		case "role":
			return {
				step,
				text: "What's my job?",
				options: ROLE_OPTIONS,
				allowText: true,
			};
		case "project": {
			if (ctx.ideProjects?.length) {
				const projectOptions: SetupOption[] = ctx.ideProjects
					.slice(0, 3)
					.map((name, i) => ({ key: String.fromCharCode(65 + i), title: name }));
				projectOptions.push({
					key: String.fromCharCode(65 + projectOptions.length),
					title: "Something else",
					subtitle: "Paste a git URL or path",
					write: true,
				});
				return {
					step,
					text: "Which project should I work on?",
					options: projectOptions,
					allowText: true,
				};
			}
			return {
				step,
				text: "Which project should I work on?",
				options: undefined,
				allowText: true,
			};
		}
		case "approval":
			return {
				step,
				text: "How careful should I be?",
				options: APPROVAL_OPTIONS,
				allowText: false,
			};
		case "summary":
			return {
				step,
				text: "", // caller fills from summaryText()
				options: [
					{ key: "A", title: "Looks good, let's go" },
					{ key: "B", title: "Change something", subtitle: "Start over from the name" },
				],
				allowText: false,
			};
	}
}

// ─── step sequencing ──────────────────────────────────────────────────────────

/** The ordered step list. */
export const STEPS: SetupStep[] = ["name", "role", "project", "approval", "summary"];

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
	approvalKey: string;
}

/** A run of the summary bubble; the chat renders `bold` runs in bold (never markdown, which printed its stars). */
export interface SummaryPart {
	text: string;
	bold?: true;
}

/** The summary bubble shown before confirmation. */
export function summaryText(s: SummaryState): SummaryPart[] {
	const approval = approvalDescription(s.approvalKey);
	// Only the first letter drops to lower case: "Review every PR" reads "review every PR", not "every pr".
	const role = s.roleText.charAt(0).toLowerCase() + s.roleText.slice(1);
	// A GitHub URL reads as owner/repo; a name or path stays as given.
	const project = s.project?.replace(/^https?:\/\/github\.com\/|\.git$/g, "");
	return [
		{ text: "I'm " },
		{ text: s.name, bold: true },
		{ text: `. I'll ${role}` },
		...(project ? [{ text: " on " }, { text: project, bold: true as const }] : []),
		{ text: `, and I'll ${approval}.` },
	];
}

// ─── typing delay ─────────────────────────────────────────────────────────────

/**
 * Simulated typing delay in ms for the bot's next message.
 * Varies between 800–1200 ms; pass a seed for reproducibility in tests.
 */
export function typingDelay(seed = Math.random()): number {
	return 800 + Math.round(seed * 400);
}
