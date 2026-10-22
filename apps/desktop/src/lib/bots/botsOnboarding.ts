/**
 * State machine for the Bots first-run onboarding flow.
 * Pure: no side effects, no imports from Svelte or the store.
 */

export type OnboardingStep = "intro" | "how1" | "how2" | "how3" | "connect" | "engine" | "ready";

export interface OnboardingState {
	step: OnboardingStep;
	hasHost: boolean;
}

export const INITIAL_STEP: OnboardingStep = "intro";

/** Advance one step forward. Returns "done" when the flow is complete. */
export function nextStep(state: OnboardingState): OnboardingStep | "done" {
	switch (state.step) {
		case "intro":
			return "how1";
		case "how1":
			return "how2";
		case "how2":
			return "how3";
		case "how3":
			return state.hasHost ? "engine" : "connect";
		case "connect":
			return "engine";
		case "engine":
			return "ready";
		case "ready":
			return "done";
		default:
			return "done";
	}
}

/** Go back one step. Returns null if already at intro. */
export function prevStep(state: OnboardingState): OnboardingStep | null {
	switch (state.step) {
		case "how1":
			return "intro";
		case "how2":
			return "how1";
		case "how3":
			return "how2";
		case "connect":
			return "how3";
		case "engine":
			return state.hasHost ? "how3" : "connect";
		case "ready":
			return "engine";
		default:
			return null;
	}
}

/** Skip directly to end. */
export function skipToEnd(_step: OnboardingStep): "done" {
	return "done";
}

/**
 * Whether to auto-show the onboarding.
 * Show when: not seen before AND no bots exist yet.
 */
export function shouldShow(seen: boolean, hasBots: boolean): boolean {
	return !seen && !hasBots;
}

/** Whether a given engine integration is ready (installed). */
export function isEngineReady(agent: { id: string; label: string; installed: boolean }): boolean {
	return agent.installed;
}

/** The four informational content steps (labeled "N of 4"). */
export const INFO_STEPS: OnboardingStep[] = ["intro", "how1", "how2", "how3"];

/** Index within INFO_STEPS (0-based), or -1 if not an info step. */
export function infoStepIndex(step: OnboardingStep): number {
	return INFO_STEPS.indexOf(step);
}

/**
 * Progress fraction [0, 1] for the thin progress bar.
 * Accounts for the skipped connect step when hasHost is true.
 */
export function stepProgress(state: OnboardingState): number {
	const steps: OnboardingStep[] = state.hasHost
		? ["intro", "how1", "how2", "how3", "engine", "ready"]
		: ["intro", "how1", "how2", "how3", "connect", "engine", "ready"];
	const idx = steps.indexOf(state.step);
	if (idx < 0) return 1;
	return (idx + 1) / steps.length;
}
