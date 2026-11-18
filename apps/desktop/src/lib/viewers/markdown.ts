import { Marked, type Tokens } from "marked";
import { highlightCode } from "@lezer/highlight";
import { highlight } from "#lib/cm.ts";
import { languageOf } from "#lib/viewers/lang.ts";

const esc = (s: string) =>
	s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

let styled = false;

/**
 * The class names `highlightCode` emits only have rules once the HighlightStyle's StyleModule is in the page, which
 * an editor view does when it mounts. Markdown rendered with no editor open (the crew chat, hover docs) adds the
 * rules itself, once.
 */
function mountHighlightStyles() {
	if (styled || typeof document === "undefined" || !highlight.module) return;
	styled = true;
	const el = document.createElement("style");
	el.textContent = highlight.module.getRules();
	document.head.append(el);
}

/** Code to HTML using the editor's own syntax colors (`--cm-syn-*` via the shared HighlightStyle). */
function highlightToHtml(code: string, lang: string): string {
	const language = languageOf(lang);
	if (!language) return esc(code);
	mountHighlightStyles();
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
export function renderMarkdown(text: string, dir: string, untrusted = false): string {
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
			// Language-server docs are third-party text: raw HTML shows as text, images are dropped and
			// links are inert (the tooltip is not a browser).
			...(untrusted && {
				html: ({ text: raw }: Tokens.HTML | Tokens.Tag) => esc(raw),
				image: ({ text: alt }: Tokens.Image) => esc(alt),
				link: ({ text: label }: Tokens.Link) => esc(label),
			}),
		},
	});
	return m.parse(text) as string;
}

/** Renderer overrides for text that came over the network: raw HTML is text, only http(s)/mailto links work. */
const NETWORK_TEXT = {
	html: ({ text: raw }: Tokens.HTML | Tokens.Tag) => esc(raw),
	image: ({ text: alt }: Tokens.Image) => esc(alt),
	link({ href, text: label }: Tokens.Link) {
		return /^(https?:|mailto:)/i.test(href) ? `<a href="${esc(href)}">${label}</a>` : label;
	},
};

/**
 * One changelog bullet's markdown as HTML (code spans, emphasis, links). Inline when it is one
 * paragraph; block when it carries a nested list or several paragraphs.
 */
export function renderNote(text: string): string {
	const m = new Marked({ async: false, gfm: true, renderer: NETWORK_TEXT });
	return (/\n\s*([-*]\s|\n)/.test(text) ? m.parse(text) : m.parseInline(text)) as string;
}
