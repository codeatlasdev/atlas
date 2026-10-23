/**
 * Keys the live mirror forwards (`bun run desktop:mirror`): what a browser keydown means to a terminal, as the bytes
 * a real terminal would send for it. Pure, so the mapping is tested without a pty.
 */

export interface Key {
	key: string;
	ctrl?: boolean;
	alt?: boolean;
	meta?: boolean;
	shift?: boolean;
}

const ESC = "\x1b";
const NAMED: Record<string, string> = {
	Enter: "\r",
	Backspace: "\x7f",
	Tab: "\t",
	Escape: ESC,
	ArrowUp: `${ESC}[A`,
	ArrowDown: `${ESC}[B`,
	ArrowRight: `${ESC}[C`,
	ArrowLeft: `${ESC}[D`,
	Home: `${ESC}[H`,
	End: `${ESC}[F`,
	Delete: `${ESC}[3~`,
	PageUp: `${ESC}[5~`,
	PageDown: `${ESC}[6~`,
};

/** Bytes for the pty, or null for keys a terminal ignores (⌘ shortcuts belong to the app, bare modifiers). */
export function keyBytes(k: Key): string | null {
	if (k.meta) return null;
	if (k.key === "Tab" && k.shift) return `${ESC}[Z`;
	const named = NAMED[k.key];
	if (named !== undefined) return k.alt ? ESC + named : named;
	if ([...k.key].length !== 1) return null; // Shift, Control, F-keys…
	if (k.ctrl) {
		const c = k.key.toUpperCase().charCodeAt(0);
		// Ctrl+@..Ctrl+_ map to 0..31 (Ctrl+C = 3); other characters have no control code.
		if (c >= 0x40 && c <= 0x5f) return String.fromCharCode(c - 0x40);
		if (k.key === " ") return "\0";
		return null;
	}
	return k.alt ? ESC + k.key : k.key;
}
