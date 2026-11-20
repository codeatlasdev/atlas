// /llms.txt, /pt-br/llms.txt and /llms-full.txt (https://llmstxt.org): an index of the docs for models and agents, generated from
// docs/_nav.json and the pages, so it can never list a page that is gone. English prose by policy (llms.txt is machine-facing and not
// translated); the Portuguese index lists the pages that have a real translation, with their Portuguese titles. Pure.
import { SITE } from "../data/site";
import { docPath, localizePath, type Locale } from "./i18n";

export interface LlmsDoc {
	slug: string;
	title: string;
	description?: string;
}
export interface LlmsGroup {
	title: string;
	items: LlmsDoc[];
}

const url = (path: string) => new URL(path, SITE.url).href;
/** `/docs/<slug>.md` in a language: the raw markdown of a docs page (src/pages/docs/[slug].md.ts). */
export const markdownPath = (slug: string, locale: Locale): string => localizePath(`/docs/${slug}.md`, locale);

const POSITIONING =
	"Atlas is a free, open-source workspace for working with AI agents. Each agent gets its own worktree (its own copy of your project). Atlas Crew keeps agents working on your own Linux server, day and night. Anything important waits for a person's approval. Grand Line is where it is going: every project an island, every computer a ship, every crew with a captain. The GPU terminal, the native app (macOS, Windows, Linux) and privacy (no accounts, no telemetry) are the quality floor, not the headline.";

const FACTS = [
	`Source: ${SITE.github} (MIT or Apache-2.0). Releases: ${SITE.releases}`,
	`Download: ${url("/download/")}`,
	`Install: macOS and Linux: \`${SITE.install}\`. Windows (PowerShell): \`${SITE.installWindows}\`, or download \`Atlas-Setup.exe\` (${SITE.releases}/download/Atlas-Setup.exe; one click, per user, no admin rights). \`atlas desktop\` installs the desktop app.`,
	"Platforms: macOS 13+ on Apple silicon; Windows 10 (1809+) and 11 on x64; Linux x86_64 (glibc 2.35+; .deb, .rpm, Arch package, AppImage). Coming soon: Intel Macs, Windows on Arm, Linux on Arm. macOS only: the native GPU terminal, capture/recording and agent test instances.",
];

const entry = (d: LlmsDoc, locale: Locale) => `- [${d.title}](${url(markdownPath(d.slug, locale))})${d.description ? `: ${d.description}` : ""}`;

/** The index of the docs. `groups` are the nav groups in order, with the titles in the language of `locale`. */
export function llmsTxt(groups: LlmsGroup[], locale: Locale): string {
	const intro =
		locale === "en"
			? [`> ${POSITIONING}`, "", ...FACTS, "", `Every docs page is plain markdown at \`/docs/<slug>.md\` (listed below), or ask for \`Accept: text/markdown\` on \`/docs/<slug>/\`. All English docs in one file: ${url("/llms-full.txt")}. Brazilian Portuguese translations: ${url("/pt-br/llms.txt")}.`]
			: [`> Brazilian Portuguese (pt-BR) translations of the Atlas docs, as markdown. ${POSITIONING}`, "", `Pages that are not listed here have no translation yet: they are English only, see ${url("/llms.txt")}.`];
	const sections = groups.filter((g) => g.items.length).map((g) => `## ${g.title}\n\n${g.items.map((d) => entry(d, locale)).join("\n")}`);
	const optional = [`- [Changelog](${url(localizePath("/changelog/", locale))})`, `- [CodeAtlas](${SITE.codeatlas})`];
	if (locale === "en") optional.unshift(`- [Full docs in one file](${url("/llms-full.txt")})`, `- [Portuguese (Brazil) docs](${url("/pt-br/llms.txt")})`);
	return `# Atlas\n\n${intro.join("\n")}\n\n${sections.join("\n\n")}\n\n## Optional\n\n${optional.join("\n")}\n`;
}

/** Every English docs page in nav order in one file, each after a rule with its address. `pages` carry the markdown of docsMarkdown(). */
export function llmsFull(pages: { slug: string; markdown: string }[]): string {
	const parts = pages.map((p) => `<!-- ${url(docPath(p.slug, "en"))} -->\n\n${p.markdown.trim()}`);
	return `# Atlas documentation\n\n> ${POSITIONING}\n\nThe whole English documentation of ${SITE.url} in one file, in the order of the sidebar. Each page starts with a comment naming its address.\n\n${parts.join("\n\n---\n\n")}\n`;
}
