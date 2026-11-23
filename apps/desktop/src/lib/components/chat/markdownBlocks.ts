import { marked } from "marked";

const OPTIONS = { async: false, breaks: true } as const;

/**
 * A markdown text as one HTML string per top-level block. A message that streams grows at its tail only, so every
 * block before the last one comes out identical to the previous call: the view keys blocks by their HTML and leaves
 * those DOM nodes alone, instead of rebuilding (and re-laying out) the whole message on every delta.
 */
export function markdownBlocks(text: string): string[] {
	const tokens = marked.lexer(text, OPTIONS);
	return tokens
		.filter((t) => t.type !== "space")
		.map((t) => marked.parser(Object.assign([t], { links: tokens.links }), OPTIONS));
}
