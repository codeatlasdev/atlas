/**
 * A pinned screenshot (`/shots/pin`): a borderless window floating above everything that shows one capture. Zoom and
 * opacity are the page's; the window itself is Rust's (`shots_pin_*`). This module is the pure model
 * `reduce(state, event) → {state, effects}` the page drives with keys, wheel and pointer events, plus the sizing
 * math. `pin.test.ts` pins the rules.
 *
 * Zoom is relative to the image's 100% size (its pixel size over the display's backing scale = its size in points).
 * It runs from 25% to 400%. The window opens at 100%, scaled down only when that would be larger than 80% of the
 * display (`fitZoom`); a double click goes back to 100%. Opacity runs from 20% to 100% (a pin you cannot see at all
 * cannot be found to close it).
 */

export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 4;
export const MIN_OPACITY = 0.2;
export const MAX_OPACITY = 1;
/** One `+` / `-` key press multiplies / divides the zoom by this. */
export const ZOOM_STEP = 1.25;
/** One `[` / `]` key press moves the opacity by this. */
export const OPACITY_STEP = 0.1;
/** Share of the display a pin may fill when it first opens. */
export const FIT_SHARE = 0.8;
/** Zoom per wheel delta unit: a mouse notch (~100) is about ±20%; a pinch reports small deltas, so it is scaled up. */
const WHEEL_ZOOM = 0.002;
const PINCH_ZOOM = 0.01;
/** Opacity per wheel delta unit with Alt held. */
const WHEEL_OPACITY = 0.002;
/** A single wheel event never moves more than this many delta units (a fast flick reports thousands). */
const MAX_DELTA = 100;

export interface Size {
	w: number;
	h: number;
}

/** Zoom clamped to 25%–400%; garbage (NaN, ±Infinity) falls back to 100%. */
export function clampZoom(z: number): number {
	if (!Number.isFinite(z)) return 1;
	return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, z));
}

/** Opacity clamped to 20%–100% and rounded to whole percents, so ten `]` presses never drift to 0.9999999. */
export function clampOpacity(o: number): number {
	if (!Number.isFinite(o)) return MAX_OPACITY;
	return Math.round(Math.min(MAX_OPACITY, Math.max(MIN_OPACITY, o)) * 100) / 100;
}

/** One key step of zoom: `+1` zooms in, `-1` out. */
export function stepZoom(z: number, dir: 1 | -1): number {
	return clampZoom(dir === 1 ? z * ZOOM_STEP : z / ZOOM_STEP);
}

/** One key step of opacity: `+1` more opaque, `-1` more transparent. */
export function stepOpacity(o: number, dir: 1 | -1): number {
	return clampOpacity(o + dir * OPACITY_STEP);
}

/** Zoom after a wheel event: scrolling up (negative delta) zooms in. `pinch` is the trackpad gesture (ctrl + wheel). */
export function wheelZoom(z: number, deltaY: number, pinch: boolean): number {
	const d = Math.max(-MAX_DELTA, Math.min(MAX_DELTA, Number.isFinite(deltaY) ? deltaY : 0));
	return clampZoom(z * Math.exp(-d * (pinch ? PINCH_ZOOM : WHEEL_ZOOM)));
}

/** Opacity after an Alt + wheel event: scrolling up makes the pin more opaque. */
export function wheelOpacity(o: number, deltaY: number): number {
	const d = Math.max(-MAX_DELTA, Math.min(MAX_DELTA, Number.isFinite(deltaY) ? deltaY : 0));
	return clampOpacity(o - d * WHEEL_OPACITY);
}

/**
 * The zoom a pin opens at: 100%, or less when the image at 100% would be larger than [`FIT_SHARE`] of the display
 * (the aspect is kept, and a small image is never scaled up). Rust's `surface::pin_fit` does the same sum to size
 * the window before this page has loaded; `pin.test.ts` pins the shared numbers.
 */
export function fitZoom(image: Size, display: Size): number {
	const w = Math.max(1, image.w);
	const h = Math.max(1, image.h);
	return Math.min(1, (display.w * FIT_SHARE) / w, (display.h * FIT_SHARE) / h);
}

/** The window size at `zoom`: whole points, never below 1. */
export function sizeAt(image: Size, zoom: number): Size {
	return { w: Math.max(1, Math.round(image.w * zoom)), h: Math.max(1, Math.round(image.h * zoom)) };
}

/** The zoom a window of `applied` size shows: Rust may clamp a request, and the label must tell the truth. */
export function zoomOf(image: Size, applied: Size): number {
	return image.w > 0 ? clampZoom(applied.w / image.w) : 1;
}

/** `125%`. */
export function percent(v: number): string {
	return `${Math.round(v * 100)}%`;
}

export interface PinState {
	/** The image's size at 100%, in points. */
	image: Size;
	zoom: number;
	opacity: number;
}

export type PinEvent =
	| { type: "zoomIn" }
	| { type: "zoomOut" }
	| { type: "wheel"; deltaY: number; alt: boolean; pinch: boolean }
	| { type: "opacityUp" }
	| { type: "opacityDown" }
	/** Double click. */
	| { type: "reset" }
	/** Rust answered a resize with the size it really applied. */
	| { type: "applied"; size: Size }
	| { type: "close" };

export type PinEffect =
	/** `shots_pin_resize`; the page answers with `applied`. */
	{ type: "resize"; size: Size } | { type: "close" };

export interface Reduced {
	state: PinState;
	effects: PinEffect[];
}

/** A pin that shows an image of `image` points at `zoom`. */
export function initialState(image: Size, zoom = 1): PinState {
	return { image, zoom: clampZoom(zoom), opacity: MAX_OPACITY };
}

function zoomed(s: PinState, zoom: number): Reduced {
	// No effect when the zoom did not change (already at a limit): no resize round trip for nothing.
	if (zoom === s.zoom) return { state: s, effects: [] };
	return { state: { ...s, zoom }, effects: [{ type: "resize", size: sizeAt(s.image, zoom) }] };
}

export function reduce(s: PinState, ev: PinEvent): Reduced {
	switch (ev.type) {
		case "zoomIn":
			return zoomed(s, stepZoom(s.zoom, 1));
		case "zoomOut":
			return zoomed(s, stepZoom(s.zoom, -1));
		case "reset":
			return zoomed(s, 1);
		case "wheel":
			// Alt + wheel fades the pin; the plain wheel and the pinch zoom it.
			return ev.alt
				? { state: { ...s, opacity: wheelOpacity(s.opacity, ev.deltaY) }, effects: [] }
				: zoomed(s, wheelZoom(s.zoom, ev.deltaY, ev.pinch));
		case "opacityUp":
			return { state: { ...s, opacity: stepOpacity(s.opacity, 1) }, effects: [] };
		case "opacityDown":
			return { state: { ...s, opacity: stepOpacity(s.opacity, -1) }, effects: [] };
		case "applied":
			return { state: { ...s, zoom: zoomOf(s.image, ev.size) }, effects: [] };
		case "close":
			return { state: s, effects: [{ type: "close" }] };
	}
}

/** The keys a pin answers to, or `null` for any other key (which the page leaves alone). */
export function keyEvent(ev: {
	key: string;
	metaKey?: boolean;
	ctrlKey?: boolean;
	altKey?: boolean;
}): PinEvent | null {
	// A browser or app shortcut (⌘-, ⌃+) is not ours.
	if (ev.metaKey || ev.ctrlKey) return null;
	switch (ev.key) {
		case "+":
		case "=":
			return { type: "zoomIn" };
		case "-":
		case "_":
			return { type: "zoomOut" };
		case "[":
			return { type: "opacityDown" };
		case "]":
			return { type: "opacityUp" };
		case "Escape":
			return { type: "close" };
		default:
			return null;
	}
}
