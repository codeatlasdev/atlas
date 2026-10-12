import { css, parseAnsi } from "#lib/ansi.ts";
import { findLinks, type LinkSpan } from "#lib/logs.ts";

export interface Seg {
	text: string;
	css: string;
	link?: LinkSpan | { kind: "osc"; target: string };
}

/**
 * A raw output line as styled segments: ANSI runs split at the path:line / URL spans found
 * in the plain text (a link may cross run boundaries), OSC 8 hyperlinks kept as links.
 */
export function segments(raw: string): Seg[] {
	const runs = parseAnsi(raw);
	const text = runs.map((r) => r.text).join("");
	const links = findLinks(text);
	if (!links.length && runs.every((r) => !r.style?.link))
		return runs.map((r) => ({ text: r.text, css: css(r.style) }));
	const out: Seg[] = [];
	let pos = 0;
	let k = 0;
	for (const run of runs) {
		const style = css(run.style);
		const osc = run.style?.link;
		let i = 0;
		while (i < run.text.length) {
			const abs = pos + i;
			while (k < links.length && links[k].end <= abs) k++;
			const inLink = k < links.length && links[k].start <= abs;
			let edge = Infinity;
			if (k < links.length) edge = inLink ? links[k].end : links[k].start;
			const end = Math.min(run.text.length, edge - pos);
			out.push({
				text: run.text.slice(i, end),
				css: style,
				link: osc ? { kind: "osc", target: osc } : inLink ? links[k] : undefined,
			});
			i = end;
		}
		pos += run.text.length;
	}
	return out;
}
