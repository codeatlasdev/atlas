/**
 * Crewmate presence state (looks live in look.ts).
 * No side effects, no imports — safe to use in tests and SSR.
 */

export type CrewmateState = "idle" | "working" | "needs_you" | "offline";
