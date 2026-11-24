/**
 * The native terminal's input sink at rest. A focused editable with a collapsed caret keeps WebKit's caret animator
 * running even at 0x0 and opacity 0, and that arms the display link: with the terminal idle, ~80 wakeups a second across
 * the UI and web processes (0.4-0.5 % of a core; docs/perf/terminal-linux-idle.md). A RANGE selection has no caret, so
 * the sink always rests holding one selected placeholder; a typed character or an IME commit replaces it.
 *
 * Programmatic selection does not touch the PRIMARY selection (measured on the private sway: a sentinel survived
 * `select()` and typing), so middle-click paste keeps what the user selected elsewhere.
 */

/** U+200B ZERO WIDTH SPACE: invisible, and a character nobody types (stripped from what reaches the pty). */
export const SEED = "\u200b";

/** What the user typed or composed: the sink's value without the placeholder. */
export function sinkText(value: string): string {
	return value.replaceAll(SEED, "");
}

/**
 * Put the sink back at rest: its value is the placeholder and the placeholder is selected. Writes nothing when it
 * already is (an input event or a keyup on a rested sink costs two property reads). Never while composing: the
 * composition owns the field's text.
 */
export function restSink(el: HTMLTextAreaElement): void {
	if (el.value !== SEED) el.value = SEED;
	if (el.selectionStart !== 0 || el.selectionEnd !== SEED.length) el.setSelectionRange(0, SEED.length);
}
