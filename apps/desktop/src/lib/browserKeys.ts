/**
 * Keys that must work while an embedded browser page has focus.
 *
 * The page is a native child webview: its keys never reach the app's webview, so the keymap in `windowKeys.ts` is blind
 * there. macOS: an AppKit `keyDown` monitor (browser_keys_mac.rs); menu key equivalents survive (see menu.ts), the rest
 * (⌘1…⌘9, ⌃Tab, ⌃⌘1…6, user remaps) do not. Windows: WebView2's `AcceleratorKeyPressed` on the child (browser_keys_win.rs).
 * Both watch only the strokes in `forwardedStrokes()`, keep them from the page and report them natively
 * (`browser_set_keys` pushes the list; the native side emits `browser://key` back). Every other key, plain typing,
 * Ctrl+F and F5 included, stays with the page. Linux (WebKitGTK) has no such hook yet: its browser tabs keep their keys.
 *
 * The list holds strokes, never command ids or anything secret: the page can learn which chords
 * Atlas owns, nothing else. Pure, so it is unit-tested against the real keymap.
 */
import { IS_MAC, strokesOf } from "#lib/keys.ts";

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

/**
 * Rebuild the event shape `strokesOf` reads, so the app resolves the stroke with its own keymap code: every binding the
 * key can mean, in precedence order (off macOS Ctrl is ⌘ first, then itself; the native side forwards the real modifiers).
 */
export const strokesOfBrowserKey = (k: BrowserKey, mac = IS_MAC): string[] =>
	strokesOf(
		{
			code: k.code,
			ctrlKey: k.ctrl,
			altKey: k.alt,
			shiftKey: k.shift,
			metaKey: k.meta,
		} as KeyboardEvent,
		mac,
	);
