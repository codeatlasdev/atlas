/**
 * State machine for the Bots first-run onboarding flow.
 * Pure: no side effects, no imports from Svelte or the store.
 */

export type OnboardingStep =
  | "intro"
  | "how1"
  | "how2"
  | "how3"
  | "connect"
  | "engine"
  | "done";

export interface OnboardingState {
  step: OnboardingStep;
  hasHost: boolean;
}

export const INITIAL_STEP: OnboardingStep = "intro";

/** Advance one step forward. */
export function nextStep(state: OnboardingState): OnboardingStep {
  switch (state.step) {
    case "intro":   return "how1";
    case "how1":    return "how2";
    case "how2":    return "how3";
    case "how3":    return state.hasHost ? "engine" : "connect";
    case "connect": return "engine";
    case "engine":  return "done";
    default:        return "done";
  }
}

/** Go back one step. Returns null if already at intro. */
export function prevStep(state: OnboardingState): OnboardingStep | null {
  switch (state.step) {
    case "how1":    return "intro";
    case "how2":    return "how1";
    case "how3":    return "how2";
    case "connect": return "how3";
    case "engine":  return state.hasHost ? "how3" : "connect";
    default:        return null;
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

/** Dot pager: which steps have pager dots and what's the active index. */
export const PAGER_STEPS: OnboardingStep[] = ["how1", "how2", "how3"];

export function pagerIndex(step: OnboardingStep): number {
  return PAGER_STEPS.indexOf(step);
}
