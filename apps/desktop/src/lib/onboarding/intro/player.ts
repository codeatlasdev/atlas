/** When the globe's paint loop runs, and the canvas buffer's size. Pure: Globe.svelte feeds it page state. */

export type Loop = "animate" | "static" | "pause";

/** Whether the paint loop runs, draws one still frame, or stops. */
export function loopMode(s: { visible: boolean; focused: boolean; reducedMotion: boolean }): Loop {
	if (!s.visible || !s.focused) return "pause";
	return s.reducedMotion ? "static" : "animate";
}

/**
 * Whether the page counts as focused for pausing animation. An agent instance (`window.__ATLAS_AGENT__`, set by the
 * debug bridge) is never focused by design (it must not steal focus), but nobody is working in another window above
 * it either: its animations must run so they can be shot and recorded.
 */
export const pageIsFocused = (hasFocus: boolean, agentInstance: boolean): boolean =>
	hasFocus || agentInstance;

export interface Backing {
	w: number;
	h: number;
	dpr: number;
}

/** Pixel size of the canvas buffer for a CSS box: device pixels, ratio capped at 2 (4K buffers cost more than they show). */
export function backing(cssW: number, cssH: number, dpr: number): Backing | null {
	if (!(cssW > 0) || !(cssH > 0)) return null;
	const r = dpr > 0 && Number.isFinite(dpr) ? Math.min(dpr, 2) : 1;
	return { w: Math.round(cssW * r), h: Math.round(cssH * r), dpr: r };
}

/** The buffer must be re-sized when it is not what the box needs (checked every frame, so layout settling is caught). */
export const needsResize = (canvas: { width: number; height: number }, want: Backing | null): boolean =>
	want !== null && (canvas.width !== want.w || canvas.height !== want.h);
