/** Recording the dock's summon key in Settings: a key press → a stroke Rust accepts (`dock::accelerator`). */
import { list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
import { strokeOf } from "#lib/keys.ts";

export type KeyRecording =
	| { kind: "stroke"; stroke: string }
	| { kind: "cancel" }
	/** Not a complete key yet (a modifier alone, or a key the keymap grammar has no name for). */
	| { kind: "wait" }
	| { kind: "refuse"; reason: string };

/**
 * What a global hotkey needs: ⌘, ⌃ or ⌥ held with a key. Without one it would swallow that key in every app, and
 * Rust refuses it as well. Shift alone does not count.
 */
export function dockKeyFrom(e: KeyboardEvent, mac: boolean): KeyRecording {
	if (e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey) return { kind: "cancel" };
	const stroke = strokeOf(e, mac);
	if (!stroke) return { kind: "wait" };
	if (!/(^|-)(ctrl|alt|cmd)-/.test(stroke)) {
		// i18n-ignore: key caps of the modifiers, not prose
		const caps = mac ? ["⌘", "⌃", "⌥"] : ["Ctrl", "Alt"];
		return {
			kind: "refuse",
			reason: m.tray_dock_key_needs_modifier({ modifiers: list(caps, "disjunction") }),
		};
	}
	return { kind: "stroke", stroke };
}
