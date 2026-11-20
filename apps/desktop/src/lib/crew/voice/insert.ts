/**
 * Where dictated text goes in the message box: at the cursor, or over the selection, with the spaces around it fixed
 * so the draft reads as typed ("fix the" + "login bug" never becomes "fix thelogin bug", and a text that starts with
 * a full stop never gets a space before it). Pure: no DOM; the composer reads and writes the textarea.
 */

export interface Edit {
	value: string;
	/** Caret after the inserted text. */
	cursor: number;
}

/** A character after which the next word is glued on: an opening bracket, a quote, a path or mention start. */
const OPENERS: Record<string, true> = { "(": true, "[": true, "{": true, '"': true, "'": true, "`": true, "/": true, "\\": true, "@": true, "#": true };
/** A text that starts with one of these belongs to what comes before it: no space in front. */
const CLOSERS: Record<string, true> = { ".": true, ",": true, ";": true, ":": true, "!": true, "?": true, ")": true, "]": true, "}": true, "%": true };

/**
 * `text` put into `value` over `[start, end)` (a caret when they are equal). Returns the new draft and where the caret
 * goes. A blank `text` changes nothing (a recognizer that heard silence must not eat the selection). Offsets outside
 * the draft are clamped and swapped when reversed.
 */
export function insertDictation(value: string, start: number, end: number, text: string): Edit {
	const said = text.trim();
	const from = Math.max(0, Math.min(start, end, value.length));
	const to = Math.min(value.length, Math.max(start, end, 0));
	if (!said) return { value, cursor: Math.min(Math.max(end, 0), value.length) };
	const before = value.slice(0, from);
	const after = value.slice(to);
	const prev = before.at(-1);
	const next = after.at(0);
	const lead = prev !== undefined && !/\s/.test(prev) && !OPENERS[prev] && !CLOSERS[said[0]] ? " " : "";
	const tail = next !== undefined && !/\s/.test(next) && !CLOSERS[next] ? " " : "";
	const inserted = lead + said;
	return { value: before + inserted + tail + after, cursor: before.length + inserted.length };
}
