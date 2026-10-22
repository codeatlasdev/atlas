/**
 * Bot presence state and typing label (looks live in look.ts).
 * No side effects, no imports — safe to use in tests and SSR.
 */

export type BotState = "idle" | "working" | "needs_you" | "offline";

/**
 * Typing indicator label.
 * [] → "" (never called with empty)
 * [name] → "name is working"
 * [a, b] → "a and b are working"
 * [a, b, c, ...] → "3 bots are working"
 */
export function typingLabel(names: string[]): string {
	if (names.length === 0) return "";
	if (names.length === 1) return `${names[0]} is working`;
	if (names.length === 2) return `${names[0]} and ${names[1]} are working`;
	return `${names.length} bots are working`;
}
