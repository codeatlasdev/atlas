/**
 * Shared motion tokens and Svelte transition factories for the Bots workspace.
 * All animations respect prefers-reduced-motion: when enabled, durations collapse
 * to near-zero and loops (shimmer, bob) are paused via CSS class.
 */
import { cubicOut, cubicIn } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

// ─── tokens ───────────────────────────────────────────────────────────────
export const dur = {
	fast: 120,
	base: 200,
	slow: 320,
} as const;

// Spring-like easing for entrances (overshoot slightly).
export const spring = (t: number) => {
	const c = cubicOut(t);
	return c + Math.sin(c * Math.PI) * 0.08;
};

// Smooth exit.
export const exit = cubicIn;

/** True when the OS has asked for reduced motion. Read once and cache. */
export const reducedMotion: boolean =
	typeof window !== "undefined" ? window.matchMedia("(prefers-reduced-motion: reduce)").matches : false;

function d(ms: number) {
	return reducedMotion ? 0 : ms;
}

// ─── transitions ──────────────────────────────────────────────────────────

/**
 * Bot message pop-in: scale .96→1, y 8→0, fade.
 * Used on every incoming bot message bubble.
 */
export function msgIn(node: Element, { delay = 0 } = {}): TransitionConfig {
	return {
		delay,
		duration: d(dur.base),
		easing: spring,
		css: (t) => `opacity:${t}; transform:scale(${0.96 + 0.04 * t}) translateY(${(1 - t) * 8}px)`,
	};
}

/**
 * User message slide-up from composer.
 */
export function msgOut(node: Element, { delay = 0 } = {}): TransitionConfig {
	return {
		delay,
		duration: d(dur.fast),
		easing: cubicOut,
		css: (t) => `opacity:${t}; transform:translateY(${(1 - t) * 12}px)`,
	};
}

/**
 * Approval embed slide-in (comes from right, soft).
 */
export function embedIn(node: Element): TransitionConfig {
	return {
		duration: d(dur.slow),
		easing: spring,
		css: (t) => `opacity:${t}; transform:translateX(${(1 - t) * 16}px) scale(${0.97 + 0.03 * t})`,
	};
}

/**
 * Collapse-out (height + fade): used when approval is answered, rows are deleted.
 */
export function collapseOut(node: HTMLElement, { duration: dur2 = dur.base } = {}): TransitionConfig {
	const h = node.offsetHeight;
	const mb = parseFloat(getComputedStyle(node).marginBottom);
	return {
		duration: d(dur2),
		easing: cubicIn,
		css: (t) => `overflow:hidden; height:${t * h}px; margin-bottom:${t * mb}px; opacity:${t}`,
	};
}

/**
 * New list row drop-in with spring (new bot row, new message).
 */
export function rowIn(node: Element, { delay = 0 } = {}): TransitionConfig {
	return {
		delay,
		duration: d(dur.slow),
		easing: spring,
		css: (t) => `opacity:${t}; transform:translateY(${(1 - t) * -10}px) scale(${0.97 + 0.03 * t})`,
	};
}

/**
 * Option card stagger: used in ask cards, stagger by index.
 */
export function optionIn(node: Element, { index = 0 } = {}): TransitionConfig {
	return {
		delay: d(index * 40),
		duration: d(dur.base),
		easing: spring,
		css: (t) => `opacity:${t}; transform:translateY(${(1 - t) * 6}px)`,
	};
}
