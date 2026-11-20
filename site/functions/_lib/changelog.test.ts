import { describe, expect, test } from "bun:test";
import { cmpVersionDesc, countChanges, kindOf, parseChangelog, renderChangelog, renderInline, renderMarkdown, splitBody, versionId, visibleIn } from "./changelog";
import { changelogBody, chromeScript, readChrome, readStories, swapChangelog, type Chrome } from "./page";
import { toRelease } from "./release";

const MD = `# Changelog

Intro with [a link](https://semver.org).

> **Alpha.**

## [Unreleased]

### Added

- secret work in progress

## [1.2.0] — 2026-10-12

### Added

- **Bold** and \`code\`
- A <script>alert(1)</script> tag

### Fixed

- Thing

## [1.10.0] — 2026-11-01

### Added

- ten

## [1.2.0-rc.1] — 2026-10-01

- rc

## [1.1.2]

- no date
`;

describe("parseChangelog", () => {
	test("never exposes [Unreleased]", () => {
		const { entries } = parseChangelog(MD);
		expect(entries.map((e) => e.version)).not.toContain("Unreleased");
		expect(JSON.stringify(renderChangelog(MD))).not.toContain("secret work");
	});
	test("splits intro and versions, with dates and anchor ids", () => {
		const p = parseChangelog(MD);
		expect(p.intro).toContain("Intro with");
		expect(p.entries.map((e) => [e.version, e.date, e.id])).toEqual([
			["1.2.0", "2026-10-12", "v1-2-0"],
			["1.10.0", "2026-11-01", "v1-10-0"],
			["1.2.0-rc.1", "2026-10-01", "v1-2-0-rc-1"],
			["1.1.2", "", "v1-1-2"],
		]);
	});
	test("handles CRLF and an empty file", () => {
		expect(parseChangelog(MD.replace(/\n/g, "\r\n")).entries).toHaveLength(4);
		expect(parseChangelog("")).toEqual({ intro: "", entries: [] });
	});
});

describe("cmpVersionDesc", () => {
	test("orders numerically, newest first, release before its pre-release", () => {
		const v = ["1.2.0", "1.10.0", "1.2.0-rc.1", "1.1.2", "0.9.9"];
		expect(v.sort(cmpVersionDesc)).toEqual(["1.10.0", "1.2.0", "1.2.0-rc.1", "1.1.2", "0.9.9"]);
	});
	test("prereleases follow SemVer precedence, not string order (alpha.10 > alpha.9, rc > beta > alpha)", () => {
		const v = ["1.4.0-alpha.9", "1.4.0-beta.1", "1.4.0-alpha.10", "1.4.0", "1.4.0-rc.1", "1.5.0-alpha.1", "1.3.0"];
		expect(v.sort(cmpVersionDesc)).toEqual(["1.5.0-alpha.1", "1.4.0", "1.4.0-rc.1", "1.4.0-beta.1", "1.4.0-alpha.10", "1.4.0-alpha.9", "1.3.0"]);
	});
	test("a version that is not SemVer sorts last instead of throwing", () => expect(["garbage", "1.0.0"].sort(cmpVersionDesc)).toEqual(["1.0.0", "garbage"]));
	test("versionId is a stable anchor", () => expect(versionId("1.2.0")).toBe("v1-2-0"));
});

describe("kindOf", () => {
	test("labels the prerelease a version was cut as", () => {
		expect(["1.4.0", "1.4.0-alpha.2", "1.4.0-beta.1", "1.4.0-rc.1", "1.4.0-nightly.1", "garbage"].map(kindOf)).toEqual(["stable", "alpha", "beta", "rc", "pre", "stable"]);
	});
});

describe("visibleIn (changelog filter)", () => {
	const all = ["1.5.0-alpha.1", "1.4.0", "1.4.0-rc.1", "1.4.0-beta.2", "1.4.0-alpha.3", "1.3.0"];
	const view = (f: "all" | "stable" | "pre") => all.filter((v) => visibleIn(f, v, all));
	test("All keeps everything", () => expect(view("all")).toEqual(all));
	test("Stable only hides prereleases whose stable exists (1.4.0 hides its alpha/beta/rc); 1.5.0-alpha.1 stays", () => expect(view("stable")).toEqual(["1.5.0-alpha.1", "1.4.0", "1.3.0"]));
	test("Prereleases shows only prereleases, including superseded ones (they are history, not hidden)", () => expect(view("pre")).toEqual(["1.5.0-alpha.1", "1.4.0-rc.1", "1.4.0-beta.2", "1.4.0-alpha.3"]));
	test("Prereleases of a line with no stable yet all show", () => expect(["1.4.0-beta.1", "1.4.0-alpha.1", "1.3.0"].filter((v) => visibleIn("pre", v, ["1.4.0-beta.1", "1.4.0-alpha.1", "1.3.0"]))).toEqual(["1.4.0-beta.1", "1.4.0-alpha.1"]));
});

describe("renderMarkdown", () => {
	test("escapes raw HTML from the source", () => {
		const html = renderMarkdown("- A <script>alert(1)</script> tag");
		expect(html).not.toContain("<script>");
		expect(html).toContain("&lt;script&gt;");
	});
	test("scopes heading ids to the version so anchors never collide", () => {
		const a = renderMarkdown("### Added\n\n### Added", "v1-2-0");
		expect(a).toContain('id="v1-2-0-added"');
		expect(a).toContain('id="v1-2-0-added-2"');
	});
	test("rejects javascript: links and marks external ones", () => {
		expect(renderMarkdown("[x](javascript:alert(1))")).toContain('href="#"');
		expect(renderMarkdown("[x](https://example.com)")).toContain('rel="noopener"');
	});
	test("renders GFM tables", () => expect(renderMarkdown("| a | b |\n|---|---|\n| 1 | 2 |")).toContain("<table>"));
});

const CHROME: Chrome = {
	dateLocale: "en-US",
	stale: "GitHub is unreachable right now, so this is the last copy we fetched ({tag}). It refreshes by itself.",
	versionsLabel: "Versions",
	filterLegend: "Show",
	filterAll: "All",
	filterStable: "Stable only",
	filterPre: "Prereleases",
	countOne: "1 version shown",
	countMany: "{n} versions shown",
	release: "Release page and downloads",
	latest: "Latest",
	allChanges: "Every change in this version",
	technicalNote: "The full technical list.",
	added: "Added: {n}",
	changed: "Changed: {n}",
	fixed: "Fixed: {n}",
	badgeAlpha: "Alpha",
	badgeBeta: "Beta",
	badgeRc: "RC",
	badgePre: "Pre-release",
};
const PT: Chrome = { ...CHROME, dateLocale: "pt-BR", allChanges: "Todas as mudanças desta versão", filterAll: "Todas", release: "Página da versão e downloads", latest: "Mais recente", added: "Novidades: {n}" };

describe("splitBody", () => {
	test("the prose above the first ### heading is the summary; the alpha notice is not", () => {
		const { lead, rest } = splitBody("Atlas is in **alpha**: expect bugs.\n\n**The big one.** It does a thing.\n\n### Added\n\n- x\n");
		expect(lead).toBe("**The big one.** It does a thing.");
		expect(rest).toBe("### Added\n\n- x");
	});
	test("an entry that is only a bullet list has no summary and all of it is the list", () => {
		expect(splitBody("- a\n- b")).toEqual({ lead: "", rest: "- a\n- b" });
	});
	test("a prose-only entry is all summary", () => expect(splitBody("Just words.")).toEqual({ lead: "Just words.", rest: "" }));
	test("an entry whose first block is a list under no heading keeps the list as details", () => {
		const { lead, rest } = splitBody("Intro line.\n\n- a\n\n### Fixed\n\n- b");
		expect(lead).toBe("Intro line.");
		expect(rest.startsWith("- a")).toBe(true);
	});
});

describe("countChanges", () => {
	test("counts top-level bullets under Added, Changed and Fixed only", () => {
		const c = countChanges("### Added\n\n- a\n  - nested\n- b\n\n### Fixed\n\n- c\n\n### Highlights\n\n- not counted");
		expect(c).toEqual({ added: 2, changed: 0, fixed: 1 });
	});
});

describe("renderInline", () => {
	test("renders bold, code and safe links without a wrapping tag, and escapes raw HTML", () => {
		expect(renderInline("**a** `b` [c](https://example.com)")).toBe('<strong>a</strong> <code>b</code> <a href="https://example.com" rel="noopener">c</a>');
		expect(renderInline("<script>x</script>")).not.toContain("<script>");
		expect(renderInline("[x](javascript:alert(1))")).toContain('href="#"');
	});
});

describe("changelogBody", () => {
	const c = renderChangelog(MD);
	const body = (md: ReturnType<typeof renderChangelog>, o: { tag?: string; stale?: boolean; chrome?: Chrome; stories?: Record<string, string> } = {}) =>
		changelogBody(md, { tag: o.tag ?? "v1.10.0", stale: o.stale ?? false, chrome: o.chrome ?? CHROME, stories: o.stories });
	test("lists every public version once, newest first", () => {
		const html = body(c);
		expect(html.indexOf('href="#v1-10-0"')).toBeLessThan(html.indexOf('href="#v1-2-0"'));
		expect(html).not.toContain("Unreleased");
		expect(html).not.toContain("secret work");
	});
	const pre = renderChangelog("# C\n\n## [1.4.0] — 2026-11-01\n\n- s\n\n## [1.4.0-beta.1] — 2026-10-20\n\n- b\n\n## [1.5.0-alpha.1] — 2026-11-05\n\n- a\n");
	test("prerelease sections carry a kind badge and data attributes for the filter; stable ones do not get a badge", () => {
		const html = body(pre, { tag: "v1.5.0-alpha.1" });
		expect(html).toContain('<section id="v1-5-0-alpha-1" data-version="1.5.0-alpha.1" data-kind="alpha"');
		expect(html).toContain('<span class="cl-badge cl-badge-alpha">Alpha</span>');
		expect(html).toContain('<span class="cl-badge cl-badge-beta">Beta</span>');
		expect(html).toContain('data-kind="stable"');
		expect(html.match(/cl-badge /g)).toHaveLength(4); // jump list + heading, for the two prereleases
	});
	test("the stable-only filter shows up only when there is a prerelease to filter, and defaults to All", () => {
		const html = body(pre, { tag: "v1.5.0-alpha.1" });
		expect(html).toContain("data-cl-filter");
		expect(html).toContain('value="all" checked');
		expect(html).toContain('data-cl-count data-one="1 version shown" data-many="{n} versions shown" role="status"'); // screen readers hear how many versions the filter leaves
		expect(body(c)).toContain("data-cl-filter"); // MD has a 1.2.0-rc.1 entry
		expect(body(renderChangelog("# C\n\n## [1.0.0] — 2026-01-01\n\n- x\n"), { tag: "v1.0.0" })).not.toContain("data-cl-filter");
	});
	test("says so when serving the last good copy", () => {
		expect(body(c, { tag: "v1.2.0", stale: true })).toContain("last copy we fetched (v1.2.0)");
		expect(body(c, { tag: "v1.2.0" })).not.toContain("cl-stale");
	});

	const STORYLESS = "# C\n\n## [2.0.0] — 2026-11-01\n\nAtlas is in **alpha**: x.\n\n**Big news.** It matters.\n\n### Added\n\n- one\n- two\n\n### Fixed\n\n- three\n\n## [1.9.0] — 2026-10-01\n\n### Added\n\n- old\n";
	const sl = renderChangelog(STORYLESS);
	test("a version without a story shows its summary and the full list behind a closed disclosure, still in the HTML", () => {
		const html = body(sl, { tag: "v2.0.0" });
		const sec = html.slice(html.indexOf('<section id="v2-0-0"'), html.indexOf('<section id="v1-9-0"'));
		expect(sec).toContain('<div class="cl-summary"><p><strong>Big news.</strong> It matters.</p>');
		expect(sec).not.toContain("Atlas is in");
		expect(sec).toContain('<details class="cl-all" id="v2-0-0-all"><summary>');
		expect(sec).toContain("Every change in this version");
		expect(sec).toContain("Added: 2 · Fixed: 1");
		expect(sec).toContain('id="v2-0-0-added"'); // heading anchors are unchanged
		expect(sec).toContain("<li>one</li>");
		expect(sec).not.toContain("<details class=\"cl-all\" id=\"v2-0-0-all\" open");
	});
	test("an entry with neither summary nor counts opens its list (a lone closed disclosure is a dead end)", () => {
		const html = body(renderChangelog("# C\n\n## [1.0.0] — 2026-01-01\n\n- x\n"), { tag: "v1.0.0" });
		expect(html).toContain('id="v1-0-0-all" open');
	});
	test("the lead section is the newest and is marked", () => {
		const html = body(sl, { tag: "v2.0.0" });
		expect(html).toContain('<section id="v2-0-0" data-version="2.0.0" data-kind="stable" data-lead');
		expect(html.match(/data-lead/g)).toHaveLength(1);
	});
	test("a story is placed in the section of ITS version, replacing the summary, between story markers; the disclosure stays", () => {
		const html = body(sl, { tag: "v2.0.0", stories: { "v1-9-0": "<div>STORY 1.9</div>", "v7-7-7": "<div>NOBODY</div>" } });
		const old = html.slice(html.indexOf('<section id="v1-9-0"'));
		expect(old).toContain("<!--story:v1-9-0--><div>STORY 1.9</div><!--/story:v1-9-0-->");
		expect(old).toContain('data-story');
		expect(old).toContain('id="v1-9-0-all"');
		expect(html).not.toContain("NOBODY"); // a story for a version that is not rendered goes nowhere
		const newest = html.slice(html.indexOf('<section id="v2-0-0"'), html.indexOf('<section id="v1-9-0"'));
		expect(newest).toContain("cl-summary"); // no story: the summary stays
		expect(newest).not.toContain("STORY");
	});
	test("a story section is not auto-opened even without counts", () => {
		const html = body(renderChangelog("# C\n\n## [1.0.0] — 2026-01-01\n\n- x\n"), { tag: "v1.0.0", stories: { "v1-0-0": "<p>s</p>" } });
		expect(html).not.toContain(" open");
	});
	test("anchors keep the same ids with or without a story", () => {
		const ids = (h: string) => [...h.matchAll(/id="([^"]+)"/g)].map((m) => m[1]).filter((i) => !i.endsWith("-all"));
		expect(ids(body(sl, { stories: { "v1-9-0": "<p>s</p>" } }))).toEqual(ids(body(sl)));
	});
	test("words come from the chrome: Portuguese labels and dates", () => {
		const html = body(sl, { chrome: PT });
		expect(html).toContain("Todas as mudanças desta versão");
		expect(html).toContain("Novidades: 2 · Fixed: 1");
		expect(html).toContain("Página da versão e downloads");
		expect(html).toContain('<time datetime="2026-11-01">1 de novembro de 2026</time>');
	});
});

describe("swapChangelog (the edge)", () => {
	const shell = (k: Chrome, inner: string) => `<html><head></head><body><article>${chromeScript(k)}<!--CHANGELOG-->${inner}<!--/CHANGELOG--></article><footer>f</footer></body></html>`;
	const fresh = renderChangelog("# C\n\n## [3.0.0] — 2026-12-01\n\nNewest summary.\n\n### Added\n\n- n\n\n## [2.0.0] — 2026-11-01\n\nOlder summary.\n\n### Added\n\n- o\n");
	const built = shell(PT, '<section id="v2-0-0"><!--story:v2-0-0--><p>HISTÓRIA 2.0</p><!--/story:v2-0-0--></section>');
	test("the chrome travels in the shell and reads back identically, even with < in it", () => {
		const k = { ...CHROME, technicalNote: "a </script> b" };
		const html = chromeScript(k);
		expect(html.slice(html.indexOf(">") + 1, html.lastIndexOf("</script>"))).not.toContain("</script>");
		expect(readChrome(`<x>${html}</x>`)).toEqual(k);
		expect(readChrome("<html></html>")).toBeNull();
	});
	test("stories already in the shell are read back by version id", () => {
		expect(readStories(built)).toEqual({ "v2-0-0": "<p>HISTÓRIA 2.0</p>" });
	});
	test("the edge swaps the body for the new releases, keeps the shell and its language, and puts the shell's story in its version's section", () => {
		const out = swapChangelog(built, fresh, { tag: "v3.0.0", stale: false })!;
		expect(out.startsWith("<html>")).toBe(true);
		expect(out).toContain("<footer>f</footer>");
		expect(out).toContain("Todas as mudanças desta versão"); // the shell is Portuguese, so the edge speaks Portuguese
		const v2 = out.slice(out.indexOf('<section id="v2-0-0"'));
		expect(v2).toContain("<p>HISTÓRIA 2.0</p>");
		const v3 = out.slice(out.indexOf('<section id="v3-0-0"'), out.indexOf('<section id="v2-0-0"'));
		expect(v3).toContain("Newest summary."); // no story for 3.0.0 yet: summary + disclosure
		expect(v3).not.toContain("HISTÓRIA");
		expect(out).toContain("<!--CHANGELOG-->");
		expect(readStories(out)["v2-0-0"]).toBe("<p>HISTÓRIA 2.0</p>"); // swapping twice keeps working
		expect(swapChangelog(out, fresh, { tag: "v3.0.0", stale: false })).toBe(out);
	});
	test("a shell without markers or without its chrome is left alone (null: the static page is served)", () => {
		expect(swapChangelog("<html></html>", fresh, { tag: "v3", stale: false })).toBeNull();
		expect(swapChangelog("<!--CHANGELOG--><!--/CHANGELOG-->", fresh, { tag: "v3", stale: false })).toBeNull();
	});
	test("Unreleased is never rendered, in any language", () => {
		const md = "# C\n\n## [Unreleased]\n\n### Added\n\n- secret\n\n## [1.0.0] — 2026-01-01\n\nShip.\n";
		expect(swapChangelog(built, renderChangelog(md), { tag: "v1.0.0", stale: false })).not.toContain("secret");
	});
});

describe("toRelease", () => {
	const m = {
		version: "1.2.0",
		date: "2026-10-12T17:13:26.190884+00:00",
		assets: {
			"aarch64-apple-darwin": {
				cli: { url: "https://github.com/x/releases/download/v1.2.0/atlas-1.2.0-aarch64-apple-darwin.tar.gz", sha256: "aa", size: 1 },
				desktop_dmg: { url: "https://github.com/x/releases/download/v1.2.0/Atlas-1.2.0-aarch64-apple-darwin.dmg", sha256: "bb", size: 2 },
			},
		},
	};
	test("maps the manifest to the fields the page needs", () => {
		const r = toRelease("v1.2.0", m);
		expect([r.version, r.date, r.dmg?.name, r.dmg?.sha256, r.cli?.name]).toEqual(["1.2.0", "2026-10-12", "Atlas-1.2.0-aarch64-apple-darwin.dmg", "bb", "atlas-1.2.0-aarch64-apple-darwin.tar.gz"]);
		expect(r.notes).toBe("https://github.com/codeatlasdev/atlas/releases/tag/v1.2.0");
	});
	test("tolerates a release without a dmg", () => expect(toRelease("v1", { version: "1", date: "2026-01-01", assets: {} }).dmg).toBeUndefined());
});
