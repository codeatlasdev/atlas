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

/** Cell rect of a padded terminal: the slot shrunk by `px`/`py` on each side, never below 1 × 1. */
export function insetRect(
	r: { left: number; top: number; width: number; height: number },
	px: number,
	py: number,
) {
	return {
		x: r.left + px,
		y: r.top + py,
		w: Math.max(1, r.width - 2 * px),
		h: Math.max(1, r.height - 2 * py),
	};
}
