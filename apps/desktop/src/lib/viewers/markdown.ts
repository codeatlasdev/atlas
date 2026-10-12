import { Marked, type Tokens } from "marked";
import { highlightCode } from "@lezer/highlight";
import { highlight } from "#lib/cm.ts";
import { languageOf } from "#lib/viewers/lang.ts";

const esc = (s: string) =>
	s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Code to HTML using the editor's own syntax colors (`--cm-syn-*` via the shared HighlightStyle). */
export function highlightToHtml(code: string, lang: string): string {
	const language = languageOf(lang);
	if (!language) return esc(code);
	let out = "";
	highlightCode(
		code,
		language.parser.parse(code),
		highlight,
		(text, classes) => {
			out += classes ? `<span class="${classes}">${esc(text)}</span>` : esc(text);
		},
		() => (out += "\n"),
	);
	return out;
}

/** True for URLs that must not be rewritten to a file read. */
const ABSOLUTE = /^([a-z][a-z0-9+.-]*:|#|\/\/)/i;

/** Resolve `rel` against the markdown file's directory (`dir` relative to the worktree root). */
export function resolveRelative(dir: string, rel: string): string {
	const out = dir ? dir.split("/") : [];
	for (const seg of rel.split(/[?#]/)[0].split("/")) {
		if (seg === "..") out.pop();
		else if (seg && seg !== ".") out.push(seg);
	}
	return out.join("/");
}

/**
 * Render markdown to HTML. Relative image sources become `data-src` (resolved to the file's
 * directory) so the viewer can load them through hostd; code blocks are syntax highlighted.
 */
export function renderMarkdown(text: string, dir: string): string {
	const m = new Marked({
		async: false,
		gfm: true,
		renderer: {
			code({ text: code, lang }: Tokens.Code) {
				const l = (lang ?? "").split(/\s/)[0];
				return `<pre class="code"><code${l ? ` data-lang="${esc(l)}"` : ""}>${highlightToHtml(code, l)}</code></pre>\n`;
			},
			image({ href, title, text: alt }: Tokens.Image) {
				const t = title ? ` title="${esc(title)}"` : "";
				if (ABSOLUTE.test(href)) return `<img src="${esc(href)}" alt="${esc(alt)}"${t}>`;
				return `<img data-src="${esc(resolveRelative(dir, decodeURI(href)))}" alt="${esc(alt)}"${t}>`;
			},
		},
	});
	return m.parse(text) as string;
}
