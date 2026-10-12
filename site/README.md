# Atlas site

Astro 5 + Svelte 5 islands, static output, bun. Lives at https://atlas.codeatlas.com.br. Develop, media and deploy notes: [docs/site.md](../docs/site.md).

```bash
cd site
bun install
bun run dev        # http://localhost:4321
bun run build      # static site in dist/
```

## Layout

| Path | What |
|---|---|
| `src/pages/index.astro` | the landing page: one chapter per screen (copy lives next to the markup) |
| `src/components/Hero.astro` | the live board (atlas.yaml, dependency graph, log river). Server-rendered in its final state |
| `src/lib/hero.ts` | the boot choreography (GSAP) that rewinds and plays the board once |
| `src/lib/motion.ts` | GSAP + ScrollTrigger + **Lenis** (the only smooth-scroll engine) and the word-rise heading reveal |
| `src/lib/page.ts` | everything scroll-driven (rail, nav ink, pins, scrubs, lazy clips); loaded after first paint |
| `src/data/stack.ts` | the demo stack shown in the hero (orbit-shop, same as the footage) |
| `src/data/film.ts` | film chapters, duration and thumbnail grid for the player |
| `src/components/islands/FilmPlayer.svelte` | the custom film player (no bytes but the poster until play) |
| `src/pages/docs/`, `src/pages/changelog/` | rendered from the repo's `docs/*.md` and `CHANGELOG.md`, on the paper register |
| `src/layouts/Base.astro`, `DocsLayout.astro` | head/SEO and docs chrome |
| `public/fonts/` | Archivo (variable, wght+wdth) and JetBrains Mono (variable), both SIL OFL, self-hosted |
| `public/media/` | the film, loops and stills (all real footage, see its README). Heavy files are not in git: run `scripts/fetch-media.sh` |

## Motion rules

`prefers-reduced-motion`: no Lenis, no ScrollTrigger, no board replay, no log movement, no video autoplay; the HTML/CSS already renders every final state. Without JavaScript the page is complete as static HTML. Continuous animation (log rivers) is CSS-only and paused off screen and in hidden tabs.

## Film assets

The film is built from code by the studio in `../studio/` (`bun studio render atlas-1-0 && bun studio publish atlas-1-0` from the repo root). `publish` copies the video, poster, scrub sprite and captions into `public/media/` and writes `public/media/film-chapters.json` (duration, thumbnail grid, chapters), which is the single source for the player's chapters.

## OG image

`scripts/og.ts` screenshots the built hero (needs `playwright` in the folder you run it from; it is not a site dependency). It also writes `apple-touch-icon.png`, `favicon.ico` and `favicon.svg` come from `assets/logo.svg` (white tile for the touch icon).

## Constraints

The Content-Security-Policy in `public/_headers` allows only same-origin resources: no external fonts, scripts, analytics or embeds. Keep everything self-hosted. There is no generated (Magnific) art on the site by design; `scripts/art.ts` remains if a future need arises and records prompts in `art/prompts.json`.
