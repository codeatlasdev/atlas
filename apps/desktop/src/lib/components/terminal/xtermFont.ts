/**
 * Font list for the xterm.js terminal. WKWebView only draws families the web process can load: a user-installed
 * font that is not exposed to it (or not loaded yet) falls back to the default serif for the glyphs while xterm
 * sized its cell on another face, which spreads every character apart (#68). So the list handed to xterm keeps
 * only families that draw fixed-pitch, and always ends with the generic `monospace`.
 *
 * `document.fonts.check` cannot tell: WKWebView answers `true` for any name, installed or not (measured on the
 * incident machine, "SF Mono" included). The advance widths of a narrow and a wide glyph can.
 */
import { fontEntries } from "./fonts.ts";

const GENERIC = new Set(["monospace", "ui-monospace", "system-ui", "sans-serif", "serif"]);

/** Narrow vs wide glyph: equal advances on a fixed-pitch face, far apart on the proportional (Times) fallback. */
const NARROW = "i";
const WIDE = "W";
/** A fixed-pitch face may still differ by rounding; the serif fallback differs by 3x. */
const PITCH_TOLERANCE = 0.1;

export interface FontEnv {
	/** Advance width in px of one character set in `family` (canvas `measureText`). */
	measure(family: string, ch: string): number;
}

/** The CSS list with the generic `monospace` as its last entry, so an unusable family never lands on a serif. */
export function withMonospaceFallback(css: string): string {
	const entries = fontEntries(css);
	if (entries.at(-1)?.toLowerCase() !== "monospace") entries.push("monospace");
	return entries.join(", ");
}

const unquote = (entry: string) => entry.replace(/^(["'])(.*)\1$/, "$2");

function fixedPitch(family: string, env: FontEnv): boolean {
	const narrow = env.measure(family, NARROW);
	const wide = env.measure(family, WIDE);
	return narrow > 0 && wide > 0 && Math.abs(narrow - wide) / wide <= PITCH_TOLERANCE;
}

/** The list reduced to entries that can draw a fixed-pitch cell; generics stay, `monospace` ends it. */
export function usableFamilies(css: string, env: FontEnv): string {
	const kept = fontEntries(css).filter((entry) => {
		const family = unquote(entry);
		if (GENERIC.has(family.toLowerCase())) return true;
		return fixedPitch(family, env);
	});
	return withMonospaceFallback(kept.join(", "));
}

/**
 * Browser environment: a canvas 2d context. The family is set with a `serif` fallback on purpose: a family the webview
 * cannot draw then measures as Times (narrow `i`, wide `W`), which `fixedPitch` rejects; with no fallback the canvas
 * would answer with its own default and could pass for fixed-pitch.
 */
export function browserFontEnv(sizePx = 16): FontEnv {
	const ctx = document.createElement("canvas").getContext("2d");
	return {
		measure(family, ch) {
			if (!ctx) return 0;
			ctx.font = `${sizePx}px "${family.replaceAll('"', "")}", serif`;
			return ctx.measureText(ch).width;
		},
	};
}
