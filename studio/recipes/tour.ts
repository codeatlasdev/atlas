// A 10 s scripted tour of the app: the live Panel and a command typed into a real terminal. Everything is driven
// through the app's own stores (no OS input), so it records the same with the window behind other windows.
// Re-run it any time, no agent needed:  bun studio run recipes/tour.ts      (add --force to re-record)
import { RECIPE_SCHEMA, type RecipeInput } from "../src/run/recipe.ts";

const recipe: RecipeInput = {
	schema: RECIPE_SCHEMA,
	id: "tour",
	title: "Product tour",
	stages: ["capture"],
	captures: [
		{
			id: "clip",
			shots: ["panel", "terminal"],
			script: {
				version: 1,
				size: { width: 1248, height: 780 },
				fps: 60,
				cursor: true,
				steps: [
					{ do: "worktree", project: "orbit-shop" },
					{ do: "tab", kind: "services", id: "panel" },
					{ do: "pause", ms: 600 },
					{ do: "shot", name: "panel" },
					{ do: "caption", text: "Every service, one place", ms: 1800 },
					{ do: "zoom", target: "main.content", scale: 1.3, ms: 900 },
					{ do: "pointer", target: "main.content", ms: 700 },
					{ do: "run", command: "echo atlas studio", typeDelayMs: 55 },
					{ do: "wait", terminal: { contains: "atlas studio", timeoutMs: 8000 } },
					{ do: "pause", ms: 900 },
					{ do: "shot", name: "terminal" },
				],
			},
		},
	],
};
export default recipe;
