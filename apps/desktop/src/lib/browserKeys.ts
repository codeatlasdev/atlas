/**
 * Keys that must work while an embedded browser page has focus.
 *
 * The page is a native child webview: macOS delivers its keys to it, never to the app's webview, so
 * the keymap in `windowKeys.ts` is blind there. Menu key equivalents survive (see menu.ts), the rest
 * (⌘1…⌘9, ⌃Tab, ⌃⌘1…6, user remaps) do not. The child's init script (crates/atlas-desktop/src/
 * browser_grab.js) therefore watches `keydown` for the strokes in `forwardedStrokes()` only, cancels
 * them in the page and reports them natively (`browser_set_keys` pushes the list; the native side
 * emits `browser://key` back). Every other key, plain typing included, stays with the page.
 *
 * The list holds strokes, never command ids or anything secret: the page can learn which chords
 * Atlas owns, nothing else. Pure, so it is unit-tested against the real keymap.
 */
import { strokeOf } from "#lib/keys.ts";

/** Edit chords the page keeps even if a user binds them (canonical stroke spelling, see keys.ts `normalize`). */
const PAGE_KEEPS: Record<string, true> = {
	"cmd-c": true,
	"cmd-v": true,
	"cmd-x": true,
	"cmd-z": true,
	"shift-cmd-z": true,
	"cmd-a": true,
};

/**
 * Strokes of the effective keymap to take from the page. A bare or shift-only stroke is never
 * forwarded (it would swallow typing); ⌘ ⌃ ⌥ chords and F-keys are. Sorted, so equal keymaps
 * produce the same list (and no needless re-push).
 */
export function forwardedStrokes(effective: Map<string, string>): string[] {
	const out: string[] = [];
	for (const stroke of effective.keys()) {
		if (Object.hasOwn(PAGE_KEEPS, stroke)) continue;
		const parts = stroke.split("-");
		const mods = parts.filter((p) => p === "ctrl" || p === "alt" || p === "cmd");
		const key = stroke.endsWith("--") ? "-" : parts[parts.length - 1];
		if (mods.length || /^f\d+$/.test(key)) out.push(stroke);
	}
	return out.sort();
}

/** What the native side sends back for a forwarded key: the fields `strokeOf` reads. */
export interface BrowserKey {
	code: string;
	ctrl: boolean;
	alt: boolean;
	shift: boolean;
	meta: boolean;
}

/** Rebuild the event shape `strokeOf` reads, so the app resolves the stroke with its own keymap code. */
export const strokeOfBrowserKey = (k: BrowserKey): string | null =>
	strokeOf({
		code: k.code,
		ctrlKey: k.ctrl,
		altKey: k.alt,
		shiftKey: k.shift,
		metaKey: k.meta,
	} as KeyboardEvent);
