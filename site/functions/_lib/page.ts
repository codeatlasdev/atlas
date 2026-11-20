// HTML pieces shared by the static build (offline fallback) and the edge function, so both render the same markup.
// Words: the page (src/components/changelog/ChangelogPage.astro) writes its catalog messages into the shell as JSON (`Chrome`), and the
// edge reads them back, so the one source of every sentence is i18n/messages/changelog*. Stories: the build renders each release story
// into its version's section between `<!--story:ID-->` markers; the edge lifts them out of the shell and places them in the section of
// the version it renders (a version without a story keeps its summary).
import type { Kind, Rendered } from "./changelog";

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** Every word the changelog body speaks. Placeholders: `{tag}`, `{n}`. */
export interface Chrome {
	/** BCP 47 tag the dates are written in ("en-US", "pt-BR") */
	dateLocale: string;
	stale: string;
	versionsLabel: string;
	filterLegend: string;
	filterAll: string;
	filterStable: string;
	filterPre: string;
	countOne: string;
	countMany: string;
	release: string;
	latest: string;
	allChanges: string;
	technicalNote: string;
	added: string;
	changed: string;
	fixed: string;
	badgeAlpha: string;
	badgeBeta: string;
	badgeRc: string;
	badgePre: string;
}

export const CHROME_ID = "cl-chrome";

const fmtDate = (iso: string, locale: string) => (iso ? new Date(`${iso}T00:00:00Z`).toLocaleDateString(locale, { year: "numeric", month: "long", day: "numeric", timeZone: "UTC" }) : "");

export interface BodyOpts {
	/** Release tag the changelog was read from ("" for the offline fallback) */
	tag: string;
	/** GitHub could not be reached; this is the last good copy */
	stale: boolean;
	chrome: Chrome;
	/** Release stories by version id (`v1-5-0-alpha-27`): the HTML of a story, already rendered */
	stories?: Readonly<Record<string, string>>;
}

/** The inside of <article data-changelog>: version jump list, then one <section> per version: its story (or its summary), then the
 * full technical list behind a disclosure (still in the HTML for search and anchors). Filter first, `hidden` until the page script runs. */
export function changelogBody(c: { entries: Rendered[] }, o: BodyOpts): string {
	const k = o.chrome;
	const attrs = (e: Rendered) => ` data-version="${esc(e.version)}" data-kind="${e.kind}"${e.superseded ? " data-superseded" : ""}`;
	const badge = (e: Rendered) => (e.kind === "stable" ? "" : ` <span class="cl-badge cl-badge-${e.kind}">${esc({ alpha: k.badgeAlpha, beta: k.badgeBeta, rc: k.badgeRc, pre: k.badgePre }[e.kind])}</span>`);
	const filter =
		`<fieldset class="cl-filter" data-cl-filter hidden><legend>${esc(k.filterLegend)}</legend>` +
		`<label><input type="radio" name="cl-filter" value="all" checked> ${esc(k.filterAll)}</label>` +
		`<label><input type="radio" name="cl-filter" value="stable"> ${esc(k.filterStable)}</label>` +
		`<label><input type="radio" name="cl-filter" value="pre"> ${esc(k.filterPre)}</label>` +
		`<span class="cl-count" data-cl-count data-one="${esc(k.countOne)}" data-many="${esc(k.countMany)}" role="status"></span></fieldset>`;
	const jump = c.entries.map((e) => `<li${attrs(e)}><a href="#${e.id}">${esc(e.version)}</a>${badge(e)}</li>`).join("");
	const stat = (n: number, tpl: string) => (n ? tpl.replace("{n}", String(n)) : "");
	const section = (e: Rendered, i: number) => {
		const story = o.stories?.[e.id];
		const stats = [stat(e.counts.added, k.added), stat(e.counts.changed, k.changed), stat(e.counts.fixed, k.fixed)].filter(Boolean);
		const lead = story
			? `<!--story:${e.id}-->${story}<!--/story:${e.id}-->`
			: e.summaryHtml
				? `<div class="cl-summary">${e.summaryHtml}</div>`
				: "";
		// An entry with nothing to say about itself (no summary, no counts) shows its list: a lone closed disclosure is a dead end.
		const open = !lead && stats.length === 0 ? " open" : "";
		const details = e.detailsHtml
			? `<details class="cl-all" id="${e.id}-all"${open}><summary><span class="cl-all-title">${esc(k.allChanges)}</span>${stats.length ? `<span class="cl-stats">${esc(stats.join(" · "))}</span>` : ""}</summary>` +
				`<p class="cl-tech-note">${esc(k.technicalNote)}</p>${e.detailsHtml}</details>`
			: "";
		return (
			`<section id="${e.id}"${attrs(e)}${i === 0 ? " data-lead" : ""}${story ? " data-story" : ""} aria-labelledby="${e.id}-h" class="cl-entry">` +
			`${i === 0 ? `<p class="cl-latest">${esc(k.latest)}</p>` : ""}<h2 id="${e.id}-h"><a href="#${e.id}" style="text-decoration:none">${esc(e.version)}</a>${badge(e)}</h2>` +
			`<p class="cl-meta">${e.date ? `<time datetime="${e.date}">${esc(fmtDate(e.date, k.dateLocale))}</time>` : ""}<a href="https://github.com/codeatlasdev/atlas/releases/tag/v${esc(e.version)}" rel="noopener">${esc(k.release)}</a></p>` +
			`${lead}${details}</section>`
		);
	};
	const note = o.stale ? `<p class="cl-stale" role="status">${esc(k.stale.replace("{tag}", o.tag))}</p>` : "";
	return `${note}${c.entries.some((e) => e.kind !== "stable") ? filter : ""}<nav aria-label="${esc(k.versionsLabel)}"><details class="cl-jumpbox"><summary>${esc(k.versionsLabel)} <span class="cl-stats">${c.entries.length}</span></summary><ul class="cl-jump">${jump}</ul></details></nav>\n${c.entries.map(section).join("\n")}`;
}

export const OPEN = "<!--CHANGELOG-->";
export const CLOSE = "<!--/CHANGELOG-->";

/** `<script type="application/json" id="cl-chrome">`: the words, written by the page next to the markers. `<` is escaped so the JSON can never end the script. */
export const chromeScript = (k: Chrome) => `<script type="application/json" id="${CHROME_ID}">${JSON.stringify(k).replace(/</g, "\\u003c")}</script>`;

export function readChrome(shell: string): Chrome | null {
	const m = new RegExp(`<script type="application/json" id="${CHROME_ID}">([\\s\\S]*?)</script>`).exec(shell);
	if (!m) return null;
	try {
		const k = JSON.parse(m[1]) as Chrome;
		return typeof k.allChanges === "string" && typeof k.dateLocale === "string" ? k : null;
	} catch {
		return null;
	}
}

/** The stories a built page carries inside its changelog body, by version id. */
export function readStories(shell: string): Record<string, string> {
	const out: Record<string, string> = {};
	for (const m of shell.matchAll(/<!--story:([\w-]+)-->([\s\S]*?)<!--\/story:\1-->/g)) out[m[1]] = m[2];
	return out;
}

/** The static page with its body between the CHANGELOG markers swapped for the changelog of `c`. The page's own words and its stories are
 * read from the shell, so the English and Portuguese pages share one handler. `null` when the shell is not one this knows how to swap. */
export function swapChangelog(shell: string, c: { entries: Rendered[] }, o: { tag: string; stale: boolean }): string | null {
	const a = shell.indexOf(OPEN);
	const b = shell.indexOf(CLOSE);
	const chrome = readChrome(shell);
	if (a < 0 || b < a || !chrome) return null;
	const body = changelogBody(c, { ...o, chrome, stories: readStories(shell.slice(a, b)) });
	return `${shell.slice(0, a)}${OPEN}${body}${shell.slice(b)}`;
}
