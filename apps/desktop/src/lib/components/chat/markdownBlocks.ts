import { marked, type Token, type Tokens } from "marked";

const OPTIONS = { async: false, breaks: true } as const;

function html(tokens: Token[], links: Tokens.Generic["links"]): string[] {
	return tokens
		.filter((t) => t.type !== "space")
		.map((t) => marked.parser(Object.assign([t], { links }), OPTIONS));
}

/**
 * One markdown text as one HTML string per top-level block. A message that streams grows at its tail only, so every block
 * before the last one comes out identical to the previous call: the view keys blocks by their HTML and leaves those DOM
 * nodes alone, and this keeps the HTML of those closed blocks too, so each call lexes only the open last block (what
 * `marked.lexer` costs grows with the text it is given, and a long answer used to be lexed whole on every flush).
 *
 * The result always equals lexing the whole text. A text that is not an extension of the previous one starts over. A
 * reference definition (`[a]: url`) is the one construct that rewrites blocks that came before it (and a title line can
 * join a definition that was already closed), so a text with a definition is lexed whole.
 */
export function markdownBlocks(): (text: string) => string[] {
	let raw = ""; // the text of the closed blocks
	let closed: string[] = []; // their HTML
	return (text) => {
		if (text.includes("\r")) text = text.replace(/\r\n?/g, "\n"); // as the lexer does, so token offsets are text offsets
		const reuse = raw !== "" && text.startsWith(raw);
		const lexed = marked.lexer(reuse ? text.slice(raw.length) : text, OPTIONS);
		const { links } = lexed;
		if (lexed.some((t) => t.type === "def")) {
			raw = "";
			closed = [];
			if (!reuse) return html(lexed, links);
			const whole = marked.lexer(text, OPTIONS);
			return html(whole, whole.links);
		}
		if (!reuse) ((raw = ""), (closed = []));
		const open = lexed.findLastIndex((t) => t.type !== "space"); // the block that may still grow
		if (open > 0) {
			const done = lexed.slice(0, open);
			raw += done.map((t) => t.raw).join("");
			closed = closed.concat(html(done, links));
		}
		return closed.concat(html(lexed.slice(Math.max(open, 0)), links));
	};
}
