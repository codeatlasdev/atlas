// The first launch: the title sequence, the welcome copy, then "Scan this Mac" in an app that has no projects.
//
// The app starts EMPTY (instance profile: no projects, onboarding on) in a scratch HOME, so discovery's
// `$HOME`-scoped Spotlight query and folder walk can never list the user's real projects (tested in
// crates/atlas-desktop/src/cmds/discover.rs). Timing and selectors are the app's own (apps/desktop/src/lib/onboarding/
// flow.ts: INTRO_TIMING, ACTIONS, testId); recipes/onboarding.test.ts fails if they drift from the numbers and ids below.
//
// No input during the title sequence: any key or click skips it (Hero.svelte `skipIntro`), so the script only
// waits for `[data-intro="done"]` and records the intro as it plays: it runs INTRO_TIMING.durationMs (3900 ms) from the
// first frame, with the hit at INTRO_TIMING.hitMs (1680 ms), which the lint/compose stages use as the sync point.
//
//   bun studio run recipes/onboarding.ts            (add --force to re-record)
//
// SMOKE ONLY until the welcome screen is final: the recording of record is made when Welcome hands over.
import { RECIPE_SCHEMA, type RecipeInput } from "../src/run/recipe.ts";

/** The title sequence's length in ms (flow.ts INTRO_TIMING.durationMs); its hit lands at 1680 ms. */
export const INTRO_MS = 3900;
const hero = (phase: string) => `[data-testid="welcome-${phase}"]`;
const scan = '[data-testid="welcome-action-scan"]';

const recipe: RecipeInput = {
	schema: RECIPE_SCHEMA,
	id: "onboarding",
	title: "First launch: intro and welcome",
	stages: ["capture"],
	captures: [
		{
			id: "intro",
			instance: { projects: false, onboarding: true },
			shots: ["welcome", "scanned"],
			script: {
				version: 1,
				size: { width: 1248, height: 780 },
				fps: 60,
				cursor: true,
				steps: [
					// the sequence starts with the app; `mark` stamps the hit's exact time into the timeline for the sound sync
					{ do: "mark", name: "intro-start" },
					{ do: "wait", selector: '[data-intro="done"]', timeoutMs: INTRO_MS + 6000 },
					{ do: "mark", name: "intro-done" },
					{ do: "wait", selector: hero("welcome"), timeoutMs: 5000 },
					{ do: "pause", ms: 900 },
					{ do: "shot", name: "welcome" },
					{ do: "pointer", target: scan, ms: 800 },
					{ do: "click", selector: scan },
					// an empty Mac: the scan ends in the "nothing found" state (the empty phase)
					{ do: "wait", selector: hero("empty"), timeoutMs: 30_000 },
					{ do: "pause", ms: 900 },
					{ do: "shot", name: "scanned" },
				],
			},
		},
	],
};
export default recipe;
