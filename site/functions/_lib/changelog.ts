// Pure pieces of the changelog engine (no I/O, unit-tested in changelog.test.ts): split CHANGELOG.md into versions,
// drop [Unreleased], and render each entry to HTML. Anything dynamic (fetching, caching) lives in edge.ts.
import { marked, Renderer } from "marked";
import { parseTag, precedence, supersededPrerelease } from "./channels";

export interface Entry {
	/** "1.2.0" */
	version: string;
	/** "2026-10-12" ("" when the heading has no date) */
	date: string;
	/** Anchor id, "v1-2-0" */
	id: string;
	/** Markdown of the section body (without the "## [x] — date" heading) */
	body: string;
}
export interface Parsed {
	/** Everything before the first version heading (title, intro, alpha notice) */
	intro: string;
	entries: Entry[];
}

const HEADING = /^## \[([^\]]+)\](?:\s*[—–-]\s*(\d{4}-\d{2}-\d{2}))?[^\n]*$/gm;

export const versionId = (v: string) => `v${v.replace(/[^0-9A-Za-z]+/g, "-").replace(/^-|-$/g, "")}`;

/** SemVer 2.0 precedence, newest first when used as `sort(cmpVersionDesc)`. A heading that is not SemVer sorts last. */
export function cmpVersionDesc(a: string, b: string): number {
	const x = parseTag(a);
	const y = parseTag(b);
	if (!x || !y) return Number(!x) - Number(!y);
	return precedence(y, x);
}

export type Kind = "stable" | "alpha" | "beta" | "rc" | "pre";
export type Filter = "all" | "stable" | "pre";

/** What a version was cut as: `pre` for a prerelease label Atlas does not ship; `stable` for releases and anything unparsable. */
export function kindOf(version: string): Kind {
	const label = parseTag(version)?.pre[0];
	if (label === undefined) return "stable";
	return label === "alpha" || label === "beta" || label === "rc" ? label : "pre";
}

/** Whether `version` shows under `filter`. "stable" hides prereleases whose own release is in `all` (the stable section is the
 * complete summary; same rule as `atlas_core::github::in_range`); "pre" shows prereleases only. */
export function visibleIn(filter: Filter, version: string, all: Iterable<string>): boolean {
	if (filter === "all") return true;
	const pre = kindOf(version) !== "stable";
	return filter === "pre" ? pre : !supersededPrerelease(version, all);
}

/** Splits the markdown into its intro and one entry per `## [version]` heading. `[Unreleased]` is never public. */
export function parseChangelog(md: string): Parsed {
	const text = md.replace(/\r\n/g, "\n");
	const marks = [...text.matchAll(HEADING)];
	const intro = (marks.length ? text.slice(0, marks[0].index) : text).trim();
	const entries: Entry[] = [];
	marks.forEach((m, i) => {
		const version = m[1].trim();
		if (/^unreleased$/i.test(version)) return;
		const start = (m.index ?? 0) + m[0].length;
		const end = i + 1 < marks.length ? marks[i + 1].index : text.length;
		entries.push({ version, date: m[2] ?? "", id: versionId(version), body: text.slice(start, end).trim() });
	});
	return { intro, entries };
}

const slug = (s: string) => s.toLowerCase().replace(/<[^>]+>/g, "").replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** The site's rules for markdown from a public source: raw HTML is escaped (the repo is not trusted input) and external links open safely. */
function siteRenderer(): Renderer {
	const renderer = new Renderer();
	renderer.html = ({ text }) => esc(text);
	renderer.link = function ({ href, title, tokens }) {
		const text = this.parser.parseInline(tokens);
		const safe = /^(https?:|mailto:|#|\/)/i.test(href) ? href : "#";
		const ext = /^https?:/i.test(safe) && !safe.startsWith("https://atlas.codeatlas.com.br");
		return `<a href="${esc(safe)}"${title ? ` title="${esc(title)}"` : ""}${ext ? ' rel="noopener"' : ""}>${text}</a>`;
	};
	return renderer;
}

/** Markdown to HTML with the site's rules, and `###` headings with ids scoped to their version so anchors never collide. */
export function renderMarkdown(md: string, scope = ""): string {
	const seen = new Map<string, number>();
	const renderer = siteRenderer();
	renderer.heading = function ({ tokens, depth }) {
		const inner = this.parser.parseInline(tokens);
		const base = `${scope ? scope + "-" : ""}${slug(inner)}`;
		const n = (seen.get(base) ?? 0) + 1;
		seen.set(base, n);
		const id = n === 1 ? base : `${base}-${n}`;
		return `<h${depth} id="${id}">${inner}</h${depth}>\n`;
	};
	return marked.parse(md, { renderer, gfm: true, async: false }) as string;
}

/** One paragraph of inline markdown (bold, code, links) with no wrapping tag: the words of a release story. */
export const renderInline = (md: string): string => marked.parseInline(md.trim(), { renderer: siteRenderer(), gfm: true, async: false }) as string;

/** What an entry says about itself, before the technical list: `lead` is the prose above the first `###` heading or the first list minus the standard alpha notice; `rest` is the technical list (Added/Changed/Fixed). */
export function splitBody(body: string): { lead: string; rest: string } {
	// The summary ends at whichever comes first: a `###` heading or the first list (older entries have bullets right under the notice).
	const at = Math.min(...[body.search(/^###\s/m), body.search(/^(?:[-*+]|\d+\.)\s/m)].filter((i) => i >= 0), body.length);
	const lead = body
		.slice(0, at)
		.split(/\n{2,}/)
		.filter((p) => p.trim() && !/^Atlas is in \*\*(?:alpha|beta)\*\*/.test(p.trim()))
		.join("\n\n");
	return { lead, rest: body.slice(at).trim() };
}

export interface Counts {
	added: number;
	changed: number;
	fixed: number;
}

/** Top-level bullets under the `### Added`, `### Changed` and `### Fixed` headings: the numbers a collapsed entry can show honestly. */
export function countChanges(rest: string): Counts {
	const counts: Counts = { added: 0, changed: 0, fixed: 0 };
	let current: keyof Counts | null = null;
	for (const line of rest.split("\n")) {
		const h = /^###\s+(.*?)\s*$/.exec(line);
		if (h) current = (h[1].toLowerCase() in counts ? h[1].toLowerCase() : null) as keyof Counts | null;
		else if (current && /^[-*+]\s/.test(line)) counts[current]++;
	}
	return counts;
}

export interface Rendered {
	version: string;
	date: string;
	id: string;
	/** The whole entry (summary and technical list), as `/changelog.json` has always served it */
	html: string;
	/** The prose above the technical list (empty for entries that have none) */
	summaryHtml: string;
	/** The technical list: Added, Changed, Fixed. Heading ids are the same as in `html` */
	detailsHtml: string;
	counts: Counts;
	kind: Kind;
	/** A prerelease whose own stable release is in the same changelog (hidden by the Stable-only filter) */
	superseded: boolean;
}

export function renderChangelog(md: string): { introHtml: string; entries: Rendered[] } {
	const { intro, entries } = parseChangelog(md);
	// The intro repeats the page's own h1 ("# Changelog"); the page template draws that.
	const introHtml = renderMarkdown(intro.replace(/^# .*\n+/, ""), "intro");
	const versions = entries.map((e) => e.version);
	return {
		introHtml,
		entries: entries
			.sort((a, b) => cmpVersionDesc(a.version, b.version))
			.map((e) => {
				const { lead, rest } = splitBody(e.body);
				return {
					version: e.version,
					date: e.date,
					id: e.id,
					html: renderMarkdown(e.body, e.id),
					summaryHtml: lead ? renderMarkdown(lead, e.id) : "",
					detailsHtml: rest ? renderMarkdown(rest, e.id) : "",
					counts: countChanges(rest),
					kind: kindOf(e.version),
					superseded: supersededPrerelease(e.version, versions),
				};
			}),
	};
}
