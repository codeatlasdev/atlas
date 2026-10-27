import { expect, test } from "bun:test";
import { ACTIONS, testId } from "../../apps/desktop/src/lib/onboarding/flow.ts";
import { parseRecipeV1 } from "../src/run/recipe.ts";
import recipe from "./onboarding.ts";

// This file is excluded from studio's tsc (it imports the app's source); bun runs it.
test("the recipe's selectors are the welcome screen's own, not stale copies", () => {
	const selectors = parseRecipeV1(recipe).captures[0]!.script.steps.flatMap((s) => ("selector" in s && s.selector ? [s.selector] : []));
	expect(selectors).toContain(`[data-testid="${ACTIONS.scan}"]`);
	for (const phase of ["welcome", "empty"]) expect(selectors).toContain(`[data-testid="${testId(phase)}"]`);
	expect(selectors).toContain("nav.dots button:last-child");
});

test("the app starts empty with onboarding on, so nothing of this Mac can appear", () => {
	expect(parseRecipeV1(recipe).captures[0]!.instance).toEqual({ projects: false, onboarding: true });
});
