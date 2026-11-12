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
| `src/pages/index.astro` | the landing page: one chapter per screen (copy lives next to the markup) |
| `src/data/landing.ts` | landing-page data: FAQ, JSON-LD, log-river rows and the cache-busting `mediaUrl()` |
| `src/pages/download.astro` | the download page: macOS, Windows and Linux cards side by side (the visitor's OS first); the macOS `.dmg` and the version facts come from the latest release at build time (`src/data/release.ts`, from the release's `manifest.json`), the Windows button is the fixed `releases/latest/download/Atlas-Setup.exe` asset (not in the manifest), plus a SmartScreen note while the files are unsigned, coming-soon platforms (Intel Mac, Windows on Arm, Linux on Arm) with a notify form (`/api/waitlist` + `topic`), checksums, requirements |
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

## Motion rules

`prefers-reduced-motion`: no Lenis, no ScrollTrigger, no log movement, no video autoplay (the hero shows its poster); the HTML/CSS already renders every final state. Without JavaScript the page is complete as static HTML. Continuous animation (log rivers) is CSS-only and paused off screen and in hidden tabs.

## Film assets

The film is built from code by the studio in `../studio/` (`bun studio render atlas-1-0 && bun studio publish atlas-1-0` from the repo root). `publish` copies the video, poster, scrub sprite and captions into `public/media/` and writes `public/media/film-chapters.json` (duration, thumbnail grid, chapters), which is the single source for the player's chapters.

## OG image

`scripts/og.ts` screenshots the built hero (needs `playwright` in the folder you run it from; it is not a site dependency, `scripts/playwright.d.ts` declares the few calls it makes for the type-check). `apple-touch-icon.png`, `favicon.ico` and `favicon.svg` are generated from `assets/logo.svg` by `bun run icons` at the repo root (white tile for the touch icon); never edit them by hand.

## Constraints

The Content-Security-Policy in `public/_headers` allows only same-origin resources: no external fonts, scripts, analytics or embeds. Keep everything self-hosted. There is no generated art on the site by design.
