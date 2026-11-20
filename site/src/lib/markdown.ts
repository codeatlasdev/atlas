// The markdown of a docs page as agents read it (`/docs/<slug>.md`). The files in docs/ are written for GitHub: they link to siblings as
// `other.md` and to the rest of the repo with relative paths. Next to /docs/<slug>.md a sibling link already works as is; everything else
// is made absolute, so the file reads the same anywhere it is copied to. Pure.
import { SITE } from "../data/site";
import type { Locale } from "./i18n";

const REPO = SITE.github;

/** Where a relative markdown link of a docs file points on the site (`.md` of a sibling) or in the repository (anything else). */
export function absoluteLink(url: string, locale: Locale): string {
	if (/^([a-z][a-z0-9+.-]*:|#|\/\/|\/)/i.test(url)) return url;
	const [path, hash = ""] = url.split("#");
	const suffix = hash ? `#${hash}` : "";
	const prefix = locale === "pt-BR" ? "/pt-br" : "";
	// A translation links to `../x.md` (the English file) and to `x.md` (its sibling): both are the page of that doc in this language.
	const sibling = path.match(locale === "pt-BR" ? /^(?:\.\.\/|\.\/)?([a-z0-9-]+)\.md$/i : /^(?:\.\/)?([a-z0-9-]+)\.md$/i);
	if (sibling) return sibling[1] === "README" ? `${SITE.url}${prefix}/docs/introduction.md` : `${SITE.url}${prefix}/docs/${sibling[1]}.md${suffix}`;
	const out: string[] = [];
	for (const p of [...(locale === "pt-BR" ? ["docs", "pt-BR"] : ["docs"]), ...path.split("/")]) {
		if (p === "..") out.pop();
		else if (p && p !== ".") out.push(p);
	}
	const repoPath = out.join("/");
	const isDir = path.endsWith("/") || !/\.[a-z0-9]+$/i.test(repoPath);
	return `${REPO}/${isDir ? "tree" : "blob"}/main/${repoPath}${suffix}`;
}

/**
 * The page as markdown: relative links made absolute (code fences and inline code untouched) and a `# title` first line when the
 * body has none (a page whose title is in its front matter).
 */
export function docMarkdown(body: string, title: string, locale: Locale): string {
	let fence: string | null = null;
	const lines = body.replace(/\r\n/g, "\n").split("\n").map((line) => {
		const f = line.match(/^\s*(`{3,}|~{3,})/)?.[1];
		if (f && fence === null) fence = f[0];
		else if (f && f[0] === fence) fence = null;
		if (f || fence !== null) return line;
		// `code` spans are kept as they are: split on them and rewrite the rest
		return line
			.split(/(`+[^`]*`+)/)
			.map((part, i) => (i % 2 ? part : part.replace(/(!?\[[^\]]*\]\()([^)\s]+)((?:\s+"[^"]*")?\))/g, (_, a, url, z) => `${a}${absoluteLink(url, locale)}${z}`)))
			.join("");
	});
	const text = lines.join("\n").trim();
	return `${/^\s*#\s/.test(text) ? text : `# ${title}\n\n${text}`}\n`;
}
