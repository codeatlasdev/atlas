/**
 * Split a long crewmate reply into a few chat messages at its own sections.
 *
 * Blocks are separated by blank lines, except inside a fenced code block (```) and between table rows, so a code
 * block or a table is never cut. A new message starts at a section header (a `#` heading or a line that is only bold
 * text, e.g. "**Raiz**"); a closing prose line after the sections ("Se quiser, detalho.") is its own message. Joining
 * the parts with blank lines gives back the original text.
 */
const HEADER = /^(#{1,6}\s|\*\*[^*\n]+\*\*:?\s*$)/;

const isRow = (line: string | undefined) => !!line && line.trimStart().startsWith("|");

function blocks(text: string): string[] {
	const out: string[] = [];
	let cur: string[] = [];
	let fence = false;
	for (const line of text.split("\n")) {
		if (/^\s*```/.test(line)) fence = !fence;
		if (line.trim() === "" && !fence) {
			if (cur.length) out.push(cur.join("\n"));
			cur = [];
			continue;
		}
		// A table row right after a blank line still belongs to the table above.
		if (!cur.length && isRow(line) && isRow(out.at(-1)?.split("\n").at(-1))) cur = [out.pop()!, ""];
		cur.push(line);
	}
	if (cur.length) out.push(cur.join("\n"));
	return out;
}

/** `minChars`: a reply shorter than this stays one message. */
export function replyParts(text: string, minChars = 700): string[] {
	const t = text.trim();
	if (t.length < minChars) return [t];
	const bs = blocks(t);
	const parts: string[] = [];
	bs.forEach((b, i) => {
		const startsSection = HEADER.test(b.split("\n")[0]);
		const closing =
			i === bs.length - 1 && parts.some((p) => HEADER.test(p.split("\n")[0])) && !/^\s*[-*|>`]/.test(b);
		if (!parts.length || startsSection || closing) parts.push(b);
		else parts[parts.length - 1] += `\n\n${b}`;
	});
	return parts;
}
