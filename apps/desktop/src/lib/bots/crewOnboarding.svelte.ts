/**
 * Whether the Crew onboarding is showing, shared so it can be (re)opened from anywhere: the workspace's "?"
 * button, the command palette and Settings → Crew ("Replay onboarding"), not only on the first run.
 */
import { ws } from "#lib/stores/workspace.svelte.ts";

export const SEEN_KEY = "bots:onboarding:seen";

export const crewOnboarding = $state({ open: false });

/** Show the onboarding again from the first step, in the Crew workspace. */
export function replayCrewOnboarding() {
	localStorage.removeItem(SEEN_KEY);
	ws.appMode = "bots";
	crewOnboarding.open = true;
}
