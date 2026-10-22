/**
 * Pure helpers for BotFace rendering.
 * No side effects, no imports — safe to use in tests and SSR.
 */

export type BotState = "idle" | "working" | "needs_you" | "offline";

const AVATAR_COLORS = [
	"#6366f1",
	"#8b5cf6",
	"#ec4899",
	"#f43f5e",
	"#f97316",
	"#eab308",
	"#22c55e",
	"#14b8a6",
	"#3b82f6",
];

/**
 * Deterministic color from bot id/seed.
 * Returns one of 9 stable hues (same AVATAR_COLORS array used in BotView).
 */
export function botColor(seed: string): string {
	let h = 0;
	for (let i = 0; i < seed.length; i++) h = ((h * 31 + seed.charCodeAt(i)) >>> 0) % AVATAR_COLORS.length;
	return AVATAR_COLORS[h];
}

/**
 * Shape variant 0–3 from seed (affects blob squish ratio).
 */
export function botVariant(seed: string): 0 | 1 | 2 | 3 {
	let h = 0;
	for (let i = 0; i < seed.length; i++) h = ((h * 17 + seed.charCodeAt(i)) >>> 0) % 4;
	return h as 0 | 1 | 2 | 3;
}

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
