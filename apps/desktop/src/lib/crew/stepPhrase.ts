/**
 * What a crewmate is doing, in a few words, from the last step hostd recorded.
 *
 * A step is "<tool> <first string argument>" (crates/atlas-host/src/crewmates/runner.rs, from the agent's hooks), e.g.
 * "Read /…/src/app.ts" or "bash git status". The working line shows it instead of a bare "Working", like the onboarding
 * story does.
 */
/** Last path segment; omp's hashline edits name the target "[path#TAG]", so the brackets and tag go first. */
const base = (p: string) => {
	const path = p.replace(/^\[([^#\]]*).*$/, "$1");
	return path.split("/").filter(Boolean).pop() ?? path;
};

const SHELL: [RegExp, string][] = [
	[
		/^(bun|npm|pnpm|yarn)\s+(run\s+)?test\b|^cargo\s+test\b|^pytest\b|^go\s+test\b|^flutter\s+test\b/,
		"Running the tests",
	],
	[
		/^(bun|npm|pnpm|yarn)\s+(install|add|i)\b|^cargo\s+(add|fetch)\b|^pip\s+install\b/,
		"Installing dependencies",
	],
	[/^(cargo\s+(build|check)|bun\s+run\s+build|npm\s+run\s+build|tsc\b|svelte-check\b)/, "Building"],
	[/^gh\b/, "Working on GitHub"],
	[/^git\b/, "Checking git"],
];

export function stepPhrase(step: string | undefined): string {
	const s = (step ?? "").trim();
	if (!s) return "Working";
	const space = s.indexOf(" ");
	const tool = (space < 0 ? s : s.slice(0, space)).toLowerCase();
	const arg = space < 0 ? "" : s.slice(space + 1).trim();

	switch (tool) {
		case "read":
			return arg ? `Reading ${base(arg)}` : "Reading";
		case "edit":
		case "multiedit":
			return arg ? `Editing ${base(arg)}` : "Editing";
		case "write":
			return arg ? `Writing ${base(arg)}` : "Writing";
		case "grep":
			return arg ? `Searching for “${arg}”` : "Searching";
		case "glob":
			return "Looking for files";
		case "bash":
		case "shell":
		case "exec": {
			// "cd apps/store && bun test": the folder change is not what it is doing, the command after it is.
			const cmd = arg.replace(/^cd\s+\S+\s*(&&|;)\s*/, "");
			for (const [re, phrase] of SHELL) if (re.test(cmd)) return phrase;
			const program = cmd.split(/\s+/)[0];
			return program ? `Running ${base(program)}` : "Running a command";
		}
	}
	// Unknown tools (MCP and the like): the last segment of the id, as words.
	const name = tool.split("__").pop()!.replace(/[_-]+/g, " ");
	return `Using ${name}`;
}

/** Shown while the agent is between tool calls (thinking, reading its own output, planning the next move). */
export const IDLE_WORDS = [
	"Thinking",
	"Planning",
	"Looking around",
	"Working",
	"Figuring it out",
	"Almost there",
];

/** How long a real step stays on the line before idle words take over, and how long each idle word stays. */
const STEP_HOLD_MS = 4000;
const IDLE_WORD_MS = 2600;

/**
 * The working line: the latest step while it is fresh, then idle words that change every few seconds. Picked from the
 * clock alone, so every re-render in the same slot shows the same word.
 */
export function workingLabel(step: string | undefined, stepAt: number | undefined, now: number): string {
	if (step && stepAt !== undefined && now - stepAt < STEP_HOLD_MS) return stepPhrase(step);
	return IDLE_WORDS[Math.floor(now / IDLE_WORD_MS) % IDLE_WORDS.length];
}

export interface StepGroup<S> {
	/** The agent's title for this stretch of work; undefined when it never set one. */
	title: string | undefined;
	steps: S[];
	/** Finished: a later group started, or the task is no longer running. */
	done: boolean;
}

/**
 * The chat's sections of a task's work: consecutive steps under the same agent title. Steps before omp's first real
 * title (it reads its memory, checks git) join that first group rather than standing untitled; steps that never got
 * one take `fallback`, the task's title (omp may set it only after its steps).
 */
export function stepGroups<S extends { group?: string }>(
	steps: S[],
	running: boolean,
	fallback?: string,
): StepGroup<S>[] {
	const groups: StepGroup<S>[] = [];
	for (const step of steps) {
		const last = groups.at(-1);
		if (last && (last.title === step.group || step.group === undefined)) last.steps.push(step);
		else if (last && last.title === undefined) ((last.title = step.group), last.steps.push(step));
		else groups.push({ title: step.group, steps: [step], done: false });
	}
	groups.forEach((g, i) => {
		g.done = !running || i < groups.length - 1;
		g.title ??= fallback;
	});
	return groups;
}
