# Atlas site

Astro 7 + Svelte 5 islands, static output, bun. Lives at https://atlas.codeatlas.com.br. Develop, media and deploy notes: [docs/site.md](../docs/site.md).

```bash
cd site
bun install
bun run dev        # http://localhost:4321
bun run build      # static site in dist/
bun run check      # astro check + type-check of scripts/ and functions/ (CI runs it)
```

## Layout

| Path | What |
|---|---|
| `src/components/home/Home.astro` | the landing page (wrappers `src/pages/index.astro`, `src/pages/pt-br/index.astro`): hero, the idea (map, ships, crew, captain), Atlas Crew, agents, projects, context (MCP), the basics, Grand Line, questions, install. One chapter per screen; words in `i18n/messages/home_*` |
| `src/data/landing.ts` | landing-page data: the home JSON-LD builder and the cache-busting `mediaUrl()` |
| `src/components/pages/Download.astro` | the download page (wrappers in `src/pages/` and `src/pages/pt-br/`): macOS, Windows and Linux cards side by side (the visitor's OS first); the macOS `.dmg` and the version facts come from the latest release at build time (`src/data/release.ts`, from the release's `manifest.json`), the Windows button is the fixed `releases/latest/download/Atlas-Setup.exe` asset (not in the manifest), plus a SmartScreen note while the files are unsigned, coming-soon platforms (Intel Mac, Windows on Arm, Linux on Arm) with a notify form (`/api/waitlist` + `topic`), checksums, requirements |
| `functions/` | Cloudflare Pages Functions: `release.json`, `changelog/`, `changelog.json` (the release engine, see [docs/site.md](../docs/site.md)), `api/waitlist` |
| `src/components/Rail.astro` | the icon-only section rail (tooltip on hover/focus, progress hairline); state comes from one scroll handler in `src/lib/page.ts` |
| `src/components/Hero.astro` | the first screen: the pitch, the install/download calls to action, and the product (the `loop-panel` loop, poster = LCP image, video started after load, never under reduced motion or Data Saver) |
| `src/components/islands/FilmDialog.svelte` | the "Play the film" button and the dialog that mounts the player on first press. Portrait phones hand the video to native fullscreen (iOS) or get a "turn your phone" prompt (the film is 16:9 until a vertical cut of the whole film exists) |
| `src/lib/motion.ts` | GSAP + ScrollTrigger + **Lenis** (the only smooth-scroll engine) and the word-rise heading reveal |
| `src/lib/page.ts` | everything scroll-driven (rail, nav ink, pins, scrubs, lazy clips); loaded after first paint |
| `src/data/stack.ts` | the demo atlas.yaml shown in the "one file" section (orbit-shop, same as the footage) |
| `src/data/film.ts` | film chapters, thumbnail grid and the ONE rule for every display of the film's length (`filmSeconds`/`filmClock`/`filmWords`: round to whole seconds); nothing on the site types the length by hand (`film.test.ts` enforces it) |
| `src/components/islands/FilmPlayer.svelte` | the custom film player (no bytes but the poster until play) |
| `src/pages/docs/`, `src/pages/changelog/` | rendered from the repo's `docs/*.md` and `CHANGELOG.md`, on the paper register |
| `src/layouts/Base.astro`, `DocsLayout.astro` | head/SEO and docs chrome |
| `public/fonts/` | Archivo (variable, wght+wdth) and JetBrains Mono (variable), both SIL OFL, self-hosted |
| `public/media/` | the film, loops and stills (all real footage, see its README). Heavy files are not in git: run `scripts/fetch-media.sh` |

## Languages (en, pt-BR)

English is the source and lives at `/`; Portuguese lives at `/pt-br/` (Astro i18n, `pt-br` is the URL segment, `pt-BR` the tag in catalogs and `<html lang>`). Rules: `src/lib/i18n.ts` (pure, tested).

- **Every word is a message.** Site text lives in `i18n/messages/<area>/` like the rest of Atlas (areas with `"surface": "site"`: `nav`, `footer`, `home_*`, `page_download`, `page_legal`, `docs_chrome`, `changelog`, ...; procedure in `.agents/skills/i18n/SKILL.md`). Use `m.key()` from `src/lib/paraglide/messages.js`; a sentence with code or a link is ONE message with `{#code}`/`{#link}` tags, drawn by `components/Rich.astro` (`lib/rich.ts`). `src/lib/paraglide/` is generated and gitignored. During the static build `src/middleware.ts` sets the locale of each page; in the browser `src/lib/locale-client.ts` reads `<html lang>` (import it before calling `m.*()` in client code). Long legal prose is markdown per language (`src/data/legal/<locale>/*.md`).
- **Every page has a twin.** `LOCALIZED_PAGES` in `src/lib/i18n.ts` (home, download, changelog, privacy, code signing) plus every docs slug exist at `/x/` and `/pt-br/x/`: the body is a component (`components/home/Home.astro`, `components/pages/*.astro`, `components/changelog/*`) and `src/pages/x.astro` / `src/pages/pt-br/x.astro` are thin wrappers passing nothing. Pages pass `alternates={alternatesOf(path)}` to `Base`, which drives hreflang, the language switch and the sitemap. A test fails when a localized page has no real pt-BR page (the edge redirect would loop with Astro's fallback).
- **Docs.** `docs/pt-BR/<slug>.md` translates `docs/<slug>.md`; front matter is `title`, `source` (`<slug>.md`) and `source_sha` (quoted: an unquoted all-digit sha is read as a number), the git blob sha of the English file when translated (`git hash-object docs/<slug>.md`), plus optionally `translation: machine` for a page a model translated and nobody reviewed (a banner says so until the line is removed), or `translation: pending` with `issue: <GitHub issue URL>` when the English changed and the translation follows (the page is outdated until `source_sha` matches, so it keeps the outdated banner; `bun run i18n:check` fails a change that edits the English page without updating its translation unless the twin says `pending`). Every docs slug has a page under `/pt-br/docs/`: a translation (with a banner when the English file changed since), or the English body with a banner, `lang="en"`, a canonical link to the English page, no search index entry, no hreflang and no sitemap entry. Nav group titles are still English; item titles come from each translation's `title`. In translations, write links as `../x.md` (GitHub reads it; the site maps it to `/pt-br/docs/x/`).
- **Which language a visitor gets** (`functions/_lib/locale.ts`, tested): on an English URL with a twin, an explicit choice wins (`?lang=en|pt-BR`, then the `atlas_lang` cookie the nav's switch writes), then the country of the request (Cloudflare `cf.country`; Portuguese-speaking countries get pt-BR), then `Accept-Language`, then English. Wanting Portuguese is a 302 to `/pt-br/...` (no-store). Crawlers and link previews are never redirected, and a `/pt-br/` URL is always served as asked. Each localized page has its Pages Function (`functions/index.ts`, `functions/download/index.ts`, `functions/docs/[[path]].ts`, ...; the changelog's own handler calls `localeRedirect`); a test pins that every page in `LOCALIZED_PAGES` has one.
- **Release stories** (`src/content/releases/<locale>/<id>.yaml`, art in `components/changelog/art/`): the illustrated, plain-language part of `/changelog/` for a version. Writing one when cutting a release: read the version in `CHANGELOG.md`, pick 3 to 6 highlights by what matters to a person, write `en/<id>.yaml` and `pt-BR/<id>.yaml` (`id` is the anchor, e.g. `v1-5-0-alpha-28`; same highlight ids, order and art in both), add art as `art/<type>.astro` (copy one; animate only under `html.js .art.play` with `prefers-reduced-motion: no-preference`, resting state final) or use `media: { still | clip }`, then `bun test src/lib/story.test.ts`, build and deploy. A version without a story shows its summary and the closed technical list. `CHANGELOG.md` stays the English technical source.

## Motion rules

`prefers-reduced-motion`: no Lenis, no ScrollTrigger, no log movement, no video autoplay (the hero shows its poster); the HTML/CSS already renders every final state. Without JavaScript the page is complete as static HTML. Continuous animation (log rivers) is CSS-only and paused off screen and in hidden tabs.

## Film assets

The film is built from code by the studio in `../studio/` (`bun studio render atlas-1-0 && bun studio publish atlas-1-0` from the repo root). `publish` copies the video, poster, scrub sprite and captions into `public/media/` and writes `public/media/film-chapters.json` (duration, thumbnail grid, chapters), which is the single source for the player's chapters.

## OG images, SEO and agents

Share images are generated at build time, one per page and language (`/og/<en|pt-br>/<key>.png`): `src/lib/og.ts` (the key of a page and the layout, pure, tested), `src/lib/og-cards.ts` (the card of every page: titles from the docs and the `seo_og_*` messages), `src/lib/og-render.ts` (satori + sharp) and the endpoint `src/pages/og/[locale]/[...key].png.ts`. The fonts are static Archivo TTFs in `src/assets/fonts/`, instances of `public/fonts/archivo-var.woff2` made by `scripts/og-fonts.sh` (satori reads neither woff2 nor variable axes; needs `pip install fonttools brotli`). `Base.astro` picks the image of the page by default (`image` prop to override). `apple-touch-icon.png`, `favicon.ico` and `favicon.svg` are generated from `assets/logo.svg` by `bun run icons` at the repo root (white tile for the touch icon); never edit them by hand.

- SEO: `Base.astro` (canonical, hreflang, `og:*`, `twitter:*`, manifest, site-wide `Organization` + `WebSite` JSON-LD), `src/lib/seo.ts` (JSON-LD builders), `DocsLayout.astro` (`BreadcrumbList`, `TechArticle`), `public/robots.txt`, the sitemap in `astro.config.mjs`.
- Agents: `/docs/<slug>.md` and `/pt-br/docs/<slug>.md` (`src/pages/docs/[slug].md.ts`, `src/lib/markdown.ts`), `Accept: text/markdown` at the edge (`functions/_lib/negotiate.ts`, `functions/docs/[[path]].ts`, `functions/pt-br/docs/[[path]].ts`), `/llms.txt`, `/llms-full.txt`, `/pt-br/llms.txt` (`src/lib/llms.ts`; generated, there is no file in `public/`). More in [docs/site.md](../docs/site.md).

## Constraints

The Content-Security-Policy in `public/_headers` allows only same-origin resources: no external fonts, scripts, analytics or embeds. Keep everything self-hosted. There is no generated art on the site by design.
