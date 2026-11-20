---
title: "The website"
description: "Develop, build and deploy the Atlas website (site/)."
---

# The website

`site/` is an Astro 7 + Svelte 5 static site served at https://atlas.codeatlas.com.br. It renders `docs/*.md` and `CHANGELOG.md` directly. See [site/README.md](https://github.com/codeatlasdev/atlas/blob/main/site/README.md) for the file layout and motion rules.

```bash
cd site
bun install
../scripts/fetch-media.sh               # one-off: the film and loops are not in git (see below)
bun run dev                            # http://localhost:4321
bun run build                          # static site in site/dist
```

## Media is not in git

The film, loops and larger stills (`site/public/media/*.mp4`, `*.webm`, and `demo.gif`) are about 28 MB and live only on the deployed site. `scripts/fetch-media.sh` downloads whatever is missing from the live site into `site/public/media/`; it is idempotent and skips files you already have. Posters and stills stay in git (all about 330 KB or less). The media is built from code by the studio in `studio/` (`bun studio render` / `publish`; see [Contributing](contributing.md#making-videos)).

Run `scripts/fetch-media.sh` before `bun run build` or `bun run site:deploy` on any machine that has not built the site before, otherwise the deployed site would ship without the film.

## Languages

Every page exists in English at `/` and in Brazilian Portuguese at `/pt-br/`: the home, download, changelog, privacy and code signing pages (`LOCALIZED_PAGES` in `site/src/lib/i18n.ts`) and every docs page. Words are catalog messages in `i18n/messages/` like the rest of Atlas; layout and rules are in [site/README.md](https://github.com/codeatlasdev/atlas/blob/main/site/README.md#languages-en-pt-br).

Which language a visitor gets is decided at the edge (`site/functions/_lib/locale.ts`) on an English URL that has a twin: `?lang=en|pt-BR` (stored), then the `atlas_lang` cookie the language switch writes, then the request's country (Portuguese-speaking countries get pt-BR, even with an English browser), then `Accept-Language`, then English. Wanting Portuguese is a `302` to the `/pt-br/` twin; crawlers and link previews are never redirected, and a `/pt-br/` URL is always served as asked.

**Release stories.** `/changelog/` opens each version with a plain-language story when one exists (`site/src/content/releases/<locale>/<id>.yaml`, illustrations in `site/src/components/changelog/art/`), and keeps the full technical list from `CHANGELOG.md` behind a disclosure under the same anchors. The built page carries its own words and stories, so the edge handler that swaps in the newest release's changelog serves both `/changelog/` and `/pt-br/changelog/`. A version without a story shows its summary and the list. How to write one when cutting a release: site/README.md, "Release stories".

## Design constraints

- The Content-Security-Policy in `site/public/_headers` allows only same-origin resources: no external fonts, scripts, analytics or embeds. Keep everything self-hosted.
- Cloudflare Pages concatenates matching `_headers` rules, so a header set on `/*` cannot be overridden on a sub-path; set `Cache-Control` only on specific paths.
- `prefers-reduced-motion`: no smooth scroll, no ScrollTrigger, no video autoplay. Without JavaScript the page is complete static HTML.

## Search, share images and agents (SEO, OG, AX)

Everything here is generated at build time from the docs and the message catalog; nothing is hand-listed.

- **Share images.** Every page has an Open Graph image (1200x630 PNG) per language at `/og/<en|pt-br>/<key>.png` (`home`, `download`, `changelog`, `privacy`, `code-signing-policy`, `docs/<slug>`): the title, a one-line subtitle, the logo, the address and the language's words. `site/src/pages/og/[locale]/[...key].png.ts` renders them with satori (layout) and sharp (PNG), from the static Archivo TTFs in `site/src/assets/fonts/`; there is no browser and no screenshot. The words are the `seo_og_*` messages plus the page titles; a docs card uses the doc's `description` (English) and the translation's title (pt-BR, translated pages only). `Base.astro` points each page at its own image (`og:image`, `twitter:image`, alt = title); a page that is a copy (an untranslated `/pt-br/docs/` page) shares its canonical page's image. To change the look, edit `site/src/lib/og.ts`. After changing the variable font run `site/scripts/og-fonts.sh` to rebuild the TTFs (needs `pip install fonttools brotli`).
- **Head and structured data.** `Base.astro` writes the canonical, hreflang (both ways plus `x-default`, from `alternates`), `og:*` with `og:locale` and `og:locale:alternate`, `twitter:card`, the web manifest and, on every page, JSON-LD `Organization` + `WebSite` (`inLanguage`). Pages add their own through the `jsonLd` prop: `SoftwareApplication` on the home and download pages, `BreadcrumbList` + `TechArticle` on docs pages (`dateModified` is the date of the last git commit of the file shown, left out when git has no history). Builders: `site/src/lib/seo.ts`. The 404 is `noindex` and has no canonical. `robots.txt` allows everything, AI crawlers included, on purpose; the sitemap lists every real translation with `xhtml:link` alternates.
- **Markdown for agents.** Every docs page is also plain markdown at `/docs/<slug>.md` and `/pt-br/docs/<slug>.md` (the translation when it exists), with relative links made absolute (`site/src/lib/markdown.ts`) and advertised by `<link rel="alternate" type="text/markdown">`. At the edge, `GET /docs/<slug>/` with `Accept: text/markdown` (named, and ranked above or before `text/html`; browsers and `*/*` still get HTML) answers with the same markdown and `Vary: Accept` (`site/functions/_lib/negotiate.ts`, `site/functions/docs/[[path]].ts`). The language redirect never applies to a request for markdown.
- **`llms.txt`.** `/llms.txt` (index with the positioning and every page's `.md` link), `/llms-full.txt` (all English docs in one file) and `/pt-br/llms.txt` (the translated pages) are generated from `docs/_nav.json` by `site/src/lib/llms.ts`. They stay English by policy (machine-facing). `_headers` sets the content types.

## Deploying (maintainers)

The site is a Cloudflare Pages project. Deploys run locally, like releases:

```bash
export CLOUDFLARE_API_TOKEN=…     # Pages and D1 edit permissions
export CLOUDFLARE_ACCOUNT_ID=…
bun run site:deploy                # build + wrangler pages deploy site/dist
scripts/deploy-site.sh --branch x  # preview deployment
```

`site:deploy` uploads `site/dist`, which is built from `site/public/`, so the media must be present locally (run `scripts/fetch-media.sh` first).

### Release-notes signup (`/api/waitlist`)

`site/functions/api/waitlist.ts` (`POST /api/waitlist`) stores emails in a D1 database bound as `DB` (see `site/wrangler.toml`; schema in `site/functions/schema.sql`). It accepts same-origin requests only, validates and lower-cases the email, ignores a hidden honeypot field, rate-limits per salted hash of the client IP (raw IPs are never stored or logged) and never logs request data. The site presents it as "Get release notes by email"; the endpoint name and contract are unchanged, plus an optional `topic` (`windows`, `linux`, `macos-intel`, `mobile-link`; anything else is dropped) that the download page sends so a signup records the platform someone is waiting for. A repeat signup with a topic appends it to the stored `topics` list. Existing databases need `ALTER TABLE waitlist ADD COLUMN topics TEXT NOT NULL DEFAULT ''` once (also in `site/functions/schema.sql`) **before** deploying this function.

### Download page

### The release engine (the site is never behind the app)

`/download/` and `/changelog/` do not depend on the last site deploy. Cloudflare Pages Functions (`site/functions/`) resolve the **latest published release at the edge**:

| Endpoint | What it serves |
|---|---|
| `/release.json[?channel=stable\|beta\|alpha]` | tag, version, date, `.dmg` and CLI assets with sha256 and size (from the release's `manifest.json`), plus `channel` and `prerelease`. Default: the default channel (`alpha` while Atlas is in alpha; `DEFAULT_CHANNEL` in `site/functions/_lib/channels.ts`, pinned to atlas-core's `#[default]` by a test); `?channel=stable` is the newest tag without a prerelease label; any other value is `400 {"error":"invalid_channel","valid":["stable","beta","alpha"]}`. `/download/` hydrates its primary button from it, so the button always points at the newest `.dmg` of the default channel (the newest alpha, or Stable's when nothing is newer); Stable, Beta and the other channels are cards below it |
| `/channels.json` | `{ stage, stage_note, channels: [{ id, label, promise, risk, risk_label, cadence, audience, switch_to, release }] }`: every [release channel](versioning.md) with its copy and what it offers right now (`release` is a `/release.json` body without the channel fields, or `null`). The copy is `crates/atlas-core/data/channels.json`, the one file the CLI and Atlas Desktop use too. `/download/` hydrates its "Other channels" cards (every channel except the default, which is the primary download) from it. A channel other than Stable with nothing newer than Stable returns Stable's own release, so the page shows "No Beta release yet" when `release.tag` equals Stable's tag |
| `/changelog/` | the static page shell with its body replaced by the changelog **of the default channel's newest release** (`CHANGELOG.md` read from `raw.githubusercontent.com` at that tag), one `<section id="v1-2-0">` per version plus a version jump list. `[Unreleased]` is never rendered. Prerelease versions get an Alpha / Beta / RC badge and a filter (All / Stable only / Prereleases, `?show=stable\|pre`) that works client-side; Stable only hides a prerelease whose own stable is in the changelog. `?channel=stable\|beta\|alpha` shows the history at that channel's newest tag |
| `/changelog.json[?channel=…]` | the same entries as data: `{ tag, entries: [{ version, date, id, html, kind, superseded }] }`; `kind` is `stable`, `alpha`, `beta`, `rc` or `pre`, `superseded` marks a prerelease whose stable is listed. Entries are ordered by SemVer precedence (`alpha.10` is newer than `alpha.9`) |

Channels (`functions/_lib/channels.ts`, a TypeScript port of `atlas_core::channel`; both sides are pinned by the shared vectors in `testdata/channels/resolve.json`): Stable is the newest tag without a prerelease label among `releases.atom` (older pages with `?after=<tag>`, at most 5, when a run of alphas fills the 10-entry window) and the `releases/latest` redirect, which is only a candidate because it is the newest alpha while Alpha is the default; a request with no `?channel=` resolves the default channel (`DEFAULT_CHANNEL`). Beta and Alpha read `github.com/codeatlasdev/atlas/releases.atom` (the 10 newest published releases, no API, cached 60 seconds with the same last-good fallback), add the Stable tag as a candidate, take the highest SemVer precedence among the tags the channel accepts, and only offer a tag once its `manifest.json` exists (the tag is pushed before the release is built, so the resolver walks down past unpublished ones). If the feed cannot be read, Beta and Alpha degrade to Stable instead of failing. The last good copy is kept per channel.

How it stays fresh and alive (`functions/_lib/edge.ts`): the redirect of `github.com/codeatlasdev/atlas/releases/latest` (no API quota, no token; the newest default-channel build, a hint only) and the feed are cached for 60 seconds, so a new release shows within about a minute; everything that hangs off a tag is cached per tag. Responses carry `Cache-Control: public, max-age=60, s-maxage=60, stale-while-revalidate=86400, stale-if-error=604800`. If GitHub is down the last good copy is served (header `x-atlas-source: stale-last-good`, and the changelog says so); with nothing cached at all, the page falls back to the body built at the last deploy. Append `?fresh=1` to any of the endpoints to skip the 60-second tag cache (manual cache-bust). The pure parts (changelog parser/renderer, manifest mapping, outage and freshness rules) are unit-tested: `bun test functions` in `site/`, part of `bun run check`.

`scripts/release.sh` also runs `bun run site:deploy` after publishing (skipped with a warning when the Cloudflare variables are missing; a failed deploy never fails the release), which refreshes the built fallback and the docs. See [PRIVACY.md](https://github.com/codeatlasdev/atlas/blob/main/PRIVACY.md).
