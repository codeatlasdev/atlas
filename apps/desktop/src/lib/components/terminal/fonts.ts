/** Pure font-list helpers for the GPU terminal: CSS `font-family` list → installed-family candidates. */

/** Generic CSS keywords: they name no installed family, so the native renderer cannot resolve them. */
const GENERIC: Record<string, true> = {
	monospace: true,
	"ui-monospace": true,
	"system-ui": true,
	"sans-serif": true,
	serif: true,
};

/** Entries of a CSS font list, in order: split on commas outside quotes, quotes kept, blanks dropped. */
export function fontEntries(css: string): string[] {
	const parts: string[] = [];
	let cur = "";
	let quote: string | null = null;
	for (const ch of css) {
		if (ch === '"' || ch === "'") quote = quote === ch ? null : (quote ?? ch);
		if (ch === "," && !quote) {
			parts.push(cur);
			cur = "";
		} else cur += ch;
	}
	parts.push(cur);
	return parts.map((p) => p.trim()).filter(Boolean);
}

/** Family names of a CSS list, in order: unquoted, generics dropped. */
export function fontCandidates(css: string): string[] {
	return fontEntries(css)
		.map((p) => p.replace(/^(["'])(.*)\1$/, "$2").trim())
		.filter((p) => p && !GENERIC[p.toLowerCase()]);
}

/**
 * The content box of `el` (inside its border) in the viewport, in CSS px: the native view's rect. The slot's border is
 * the terminal padding band, so this is the slot shrunk by `terminal.padding_x/y` on each side.
 */
export function contentRect(el: HTMLElement) {
	const r = el.getBoundingClientRect();
	return {
		left: r.left + el.clientLeft,
		top: r.top + el.clientTop,
		width: Math.max(1, el.clientWidth),
		height: Math.max(1, el.clientHeight),
	};
}
