/**
 * Layout zoom (⌘+ ⌘− ⌘0 on macOS, Ctrl on other systems): the whole app scales like a browser tab does, in
 * fixed steps, bounded, persisted as `zoom` in settings.json. The webview is scaled by `zoom_set` (Rust,
 * `Webview::set_zoom`), called from `stores/zoom.svelte.ts`, which records the factor here with `applyZoom`.
 *
 * Under page zoom every DOM measurement (`getBoundingClientRect`, `innerWidth`, `clientX`) is in CSS px, but
 * native views (the GPU terminal, embedded browsers, the page stage, swipe zones, window captures) are placed
 * in window points: `nativeRect` converts CSS → points at the one place each rect leaves the page. This module
 * has no runes, so the modules that place native views import it without pulling in the settings store.
 */

/** The steps, like a browser's. A ladder (not ±10 %) keeps in/out exact inverses with no float drift. */
export const ZOOM_LEVELS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3];

export type ZoomAction = "in" | "out" | "reset";

/**
 * The level after `action`. A value off the ladder (a hand-edited settings.json) moves to the next step in the
 * direction pressed, so a key press always changes something; the ends are sticky.
 */
export function zoomStep(current: number, action: ZoomAction): number {
	if (action === "reset") return 1;
	const next =
		action === "in"
			? ZOOM_LEVELS.find((z) => z > current + 1e-9)
			: ZOOM_LEVELS.findLast((z) => z < current - 1e-9);
	return next ?? ZOOM_LEVELS[action === "in" ? ZOOM_LEVELS.length - 1 : 0];
}

interface KeyLike {
	key: string;
	code: string;
	metaKey: boolean;
	ctrlKey: boolean;
	altKey: boolean;
	shiftKey: boolean;
}

/** Physical key (layout independent: `+` is ⇧= on US layouts, its own key on others) → action. */
const CODES: Record<string, ZoomAction> = {
	Equal: "in",
	NumpadAdd: "in",
	Minus: "out",
	NumpadSubtract: "out",
	Digit0: "reset",
	Numpad0: "reset",
};
/** What the key types, for events with no usable `code` (synthetic events, remote input). */
const KEYS: Record<string, ZoomAction> = { "=": "in", "+": "in", "-": "out", "0": "reset" };

/**
 * The zoom action of a key event, or null. ⌘ on macOS, Ctrl elsewhere, with no other modifier; ⇧ is allowed
 * only on the keys that need it to type `+`.
 */
export function zoomAction(e: KeyLike, mac: boolean): ZoomAction | null {
	if (e.altKey || (mac ? e.ctrlKey || !e.metaKey : e.metaKey || !e.ctrlKey)) return null;
	const action = CODES[e.code] ?? KEYS[e.key] ?? null;
	// ⇧0 and ⇧- are other chords; ⇧= (and the numpad plus) is how `+` is typed.
	return e.shiftKey && action !== "in" ? null : action;
}

let applied = 1;

/** The zoom the webview really has now. */
export const appliedZoom = () => applied;

/** Record the factor Rust applied. A value that is not a positive finite number is ignored. */
export function applyZoom(factor: number) {
	if (factor > 0 && Number.isFinite(factor)) applied = factor;
}

export interface Rect {
	x: number;
	y: number;
	w: number;
	h: number;
}

/** A page rect in window points. A zoom that is not a positive finite number is treated as 100 %. */
export function toNativeRect(r: Rect, zoom: number): Rect {
	if (!(zoom > 0) || !Number.isFinite(zoom)) return r;
	return { x: r.x * zoom, y: r.y * zoom, w: r.w * zoom, h: r.h * zoom };
}

/** A page rect (CSS px) as the window points a native view's frame takes, at the zoom the webview has now. */
export const nativeRect = (r: Rect): Rect => toNativeRect(r, applied);
