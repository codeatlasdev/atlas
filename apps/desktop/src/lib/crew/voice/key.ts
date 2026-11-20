/**
 * The microphone key of the message box: ⌘⇧M on macOS, Ctrl+Shift+M elsewhere. It only means something while the
 * message box has the focus (the global keymap never sees it), and it is a hold-to-talk key, so both halves are here:
 * the press that starts it and the key-up that ends it (any key of the chord coming up ends the hold).
 */

/** The stroke in the keymap's spelling, for `label()`. */
export const DICTATION_STROKE = "cmd-shift-m";

interface Down {
	code: string;
	shiftKey: boolean;
	metaKey: boolean;
	ctrlKey: boolean;
	altKey: boolean;
}

/** The chord went down: M with Shift and the platform's main modifier, nothing else (Alt types symbols on a Mac). */
export function isDictationKey(e: Down, mac: boolean): boolean {
	return (
		e.code === "KeyM" && e.shiftKey && !e.altKey && (mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey)
	);
}

/** A key came up that was part of the chord. */
export function endsDictationHold(e: { code: string; key: string }): boolean {
	return e.code === "KeyM" || e.key === "Shift" || e.key === "Meta" || e.key === "Control";
}

/** `ms` of sound as `m:ss`. */
export function clock(ms: number): string {
	const s = Math.floor(ms / 1000);
	return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
