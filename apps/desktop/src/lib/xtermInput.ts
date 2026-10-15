import type { Terminal } from "@xterm/xterm";

/**
 * xterm.js treats its hidden textarea's value as "input still to be sent" but never empties it: capitals and
 * spaces (which it lets the browser insert) pile up there until Enter or blur. A keyCode-229 keydown outside a
 * composition (what WebKitGTK + IBus/IME send for dead keys: `´` then `e` on a pt-BR layout) makes
 * `CompositionHelper._handleAnyTextareaChanges` diff the whole textarea and emit it, so every accent replays
 * what was typed before plus runs of spaces (issue #23; upstream xtermjs/xterm.js#6078, #6012).
 *
 * Two guards, both needed:
 * - `imeStagingKey`: a 229 keydown outside a composition is the IM staging text (a dead key, the vowel after
 *   it); the text itself arrives through `input`/composition events, so xterm must not also diff the textarea.
 *   The terminal's one custom key handler returns `false` for it.
 * - `keepTextareaEmpty`: empties the textarea once a key or composition is settled, leaving nothing to replay.
 *   It runs in a timer queued after xterm's own `setTimeout(0)` reads (keydown/compositionend queue theirs
 *   first; timers at equal delay run in order), and never while a composition is open, which would cancel the
 *   IME's preedit.
 */
export function imeStagingKey(e: Pick<KeyboardEvent, "type" | "keyCode">, composing: boolean): boolean {
	return e.type === "keydown" && e.keyCode === 229 && !composing;
}

/** Keeps the textarea empty; `composing()` is true between `compositionstart` and `compositionend`. */
export function keepTextareaEmpty(term: Terminal): { composing: () => boolean; stop: () => void } {
	const ta = term.textarea;
	if (!ta) return { composing: () => false, stop: () => {} };
	let composing = false;
	let timer: ReturnType<typeof setTimeout> | undefined;
	const sweep = () => {
		clearTimeout(timer);
		timer = setTimeout(() => {
			if (!composing) ta.value = "";
		}, 0);
	};
	const on = (type: string, fn: () => void) => {
		ta.addEventListener(type, fn);
		return () => ta.removeEventListener(type, fn);
	};
	const offs = [
		on("compositionstart", () => (composing = true)),
		on("compositionend", () => {
			composing = false;
			sweep();
		}),
		on("keyup", sweep),
		on("input", sweep),
	];
	return {
		composing: () => composing,
		stop: () => {
			clearTimeout(timer);
			offs.forEach((off) => off());
		},
	};
}
