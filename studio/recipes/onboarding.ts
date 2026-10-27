// The first launch: the welcome deck, then "Scan this Mac" in an app that has no projects.
//
// The app starts EMPTY (instance profile: no projects, onboarding on) in a scratch HOME, so discovery's
// `$HOME`-scoped Spotlight query and folder walk can never list the user's real projects (tested in
// crates/atlas-desktop/src/cmds/discover.rs). Selectors are the app's own (apps/desktop/src/lib/onboarding/flow.ts:
// ACTIONS, testId; the deck's step dots in deck/Deck.svelte); recipes/onboarding.test.ts fails if they drift from the ids below.
//
// The deck has no timed intro to wait out: it opens on its first slide and waits for a click. The script lingers on
// each slide for a beat, then takes the last dot (the project steps) and scans.
//
//   bun studio run recipes/onboarding.ts            (add --force to re-record)
//
// SMOKE ONLY until the welcome screen is final: the recording of record is made when Welcome hands over.
import { RECIPE_SCHEMA, type RecipeInput } from "../src/run/recipe.ts";

const hero = (phase: string) => `[data-testid="welcome-${phase}"]`;
const scan = '[data-testid="welcome-action-scan"]';
/** The deck's step dots: one per slide, the last one is the project steps. */
const dots = "nav.dots button";
const lastDot = "nav.dots button:last-child";

const recipe: RecipeInput = {
	schema: RECIPE_SCHEMA,
	id: "onboarding",
	title: "First launch: deck and welcome",
	stages: ["capture"],
	captures: [
		{
			id: "deck",
			instance: { projects: false, onboarding: true },
			shots: ["welcome", "scanned"],
			script: {
				version: 1,
				size: { width: 1248, height: 780 },
				fps: 60,
				cursor: true,
				steps: [
					{ do: "mark", name: "deck-start" },
					{ do: "wait", selector: dots, timeoutMs: 10_000 },
					{ do: "pause", ms: 1500 },
					{ do: "click", selector: lastDot },
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
