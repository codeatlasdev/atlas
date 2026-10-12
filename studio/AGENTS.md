# Atlas Studio: notes for coding agents

`studio/` builds every published Atlas video from code: the site film and its vertical cut, and "What's new" release videos. A film is **typed data**; scene builders turn it into HTML; [HyperFrames](https://github.com/heygen-com/hyperframes) (Apache-2.0) renders the picture; `ffmpeg` mixes and measures the audio. Why HyperFrames and not Remotion: [DECISION.md](DECISION.md) (license).

```
studio/
  films/<id>/film.ts       a film: cuts (aspect ratios), scene list, narration lines, chapters, music
  src/
    lib/types.ts           THE contract (Film, Cut, Scene, …). Read it first.
    scenes/                scene builders: title, code, app, terminal (+ base.ts: field, fades, title block)
    compile.ts             Film cut -> HyperFrames project in .build/<film>/<aspect>/ (pure function of data + asset hashes)
    prepare.ts             crop/scale/trim recordings into small H.264 proxies, cached by input hash
    render.ts              compile -> `hyperframes check` -> render -> mix -> mux -> webm/vtt/poster/sprite
    audio/voice.ts         ElevenLabs TTS cached by text hash;  audio/mix.ts  ducking + MEASURED loudness assert
    capture/               `capture app|tui`: the real app (isolated instance) and the vhs take
    publish.ts release.ts preview.ts check.ts cli.ts
  assets/                  fonts (the site's own, OFL), logo.svg
  clips/library.json       tagged recordings that release videos pick from
  .cache/ .build/ out/     gitignored: manifest + takes + proxies, compiled projects, renders
```

## Commands (from the repo root; `bun studio help`)

| Command | What |
|---|---|
| `bun studio check` | **The gate for studio changes.** tsc, brand tokens vs the site CSS, film data, a 1 s-per-scene smoke render of every cut (real compile + HyperFrames lint + render), and a determinism check. ~1-2 min. |
| `bun studio preview <film> [--aspect 9:16] [--open]` | Compiles, serves HyperFrames Studio on :3002, rebuilds on save of film.ts / scenes / brand. |
| `bun studio render <film> [--aspect …] [--draft]` | Full render to `studio/out/<film>/` (mp4, webm, vtt, poster, scrub sprite, `*.report.json` with measured duration/LUFS/hashes). `--draft` is a fast low-quality silent-quality pass. |
| `bun studio voice <film> [--offline] [--models]` | Synthesize narration. Cached by hash of (line id, voice, model, settings, text); only changed lines hit the API. `--models` lists the account's TTS models (the film pins `model`; bump it deliberately). |
| `bun studio capture app <scene>` / `tui <take>` / `list` | Record the real app or the vhs take into `.cache/` and register it (sha256) in `.cache/manifest.json`. |
| `bun studio assets [list\|verify\|import <dir>]` | Inspect or verify the manifest; `import` seeds it from the 1.0 working dir. |
| `bun studio publish <film> [--release vX.Y.Z]` | Copy renders into `site/public/media/`, write `manifest.json` (content hashes) and `film-chapters.json`; `--release` also `gh release upload`s the mp4/webm. |
| `bun studio release <version\|unreleased> [--as 1.1] [--voice] [--aspect …] [--draft]` | "What's new" video (16:9 + 9:16) from the CHANGELOG.md section. |

## Add a scene to a film

1. Open `films/<id>/film.ts`. Scenes are plain objects in `cut.scenes`; copy a neighbour. Types: `title` (headline/subtitle/url + optional Atlas `mark`), `code` (shiki card, optional `typing`), `app` (a recording: `clip: "rec:<name>"`, `from`, `crop`, `box`, optional `push`/`pan`/`cursor`/`overlay`), `terminal` (a vhs take cut into `segments`, `speed`, `width`).
2. Durations are seconds; the film's cut is `transitionIn: dip 0.25 s` (default) / `crossfade` / `cut`. Total length = sum of durations (minus crossfade overlaps).
3. If the scene needs new footage: `bun studio capture app <scene>` (adds `rec:<scene>` to the manifest).
4. Move the narration: `voice.lines[].at` is the second a line starts; `chapters[].t` is the site player's chapter list. `publish` writes them to `site/public/media/film-chapters.json`; the site reads that file, so you never edit `site/src/data/film.ts` by hand.
5. `bun studio preview <film>` to look, `bun studio check` before you finish.

## Add a scene TYPE

Add the type to `src/lib/types.ts` (`Scene` union), a builder `src/scenes/<type>.ts` returning `{ html, css, js }` (`js` is GSAP on the scene's paused timeline `tl`, scene-local seconds), and one `case` in `compile.ts`'s `buildScene`. Reuse `base.ts` (`titleHtml`, `alphaIn`, `markHtml`).

## Add a film

`films/<id>/film.ts` default-exports a `Film`. It is discovered automatically (`bun studio films`). One film may have several cuts (16:9, 9:16, 1:1); each cut has its own scene list and narration.

## Rules that keep renders deterministic (HyperFrames contract)

- No `Date.now()`, `performance.now()`, unseeded `Math.random()`, network fetches or input state in scene code. Motion is GSAP on a **paused** timeline registered as `window.__timelines["<scene-id>"]`; `compile.ts` already does the registration, scene builders only add tweens.
- Never tween `visibility`/`display`/`autoAlpha` on a `.clip`; fade a child (`#root-fade` does the scene fades).
- A `<video data-start>` must not sit inside another element with `data-start` (lint error). Scenes are sub-compositions, so the scene's video is its own timed child; push/pan act on its non-timed wrapper `#clip`.
- Every `<audio>` needs an id. The studio renders the picture **silent** and mixes audio with ffmpeg so loudness is measured, not assumed.
- Text must fit its box: use `max-width`/wrapping, never `<br>` in body text.
- Fonts are the site's own files in `assets/fonts` (copied into each project). Brand tokens are in `src/brand/tokens.ts`; `check` fails if they drift from `site/src/styles/global.css`.

## Media is not in git

`studio/.cache/` (recordings, takes, stills, voice, music, proxies) and `studio/out/` are gitignored. The manifest records sha256 + source of every cached asset. A fresh clone has nothing cached: `bun studio assets` shows what is missing and each missing key's error says which command makes it (`capture app`, `capture tui`, `voice`, `bg`). The published media is fetched with `scripts/fetch-media.sh`.

## Secrets

Only from the environment: `ELEVENLABS_API_KEY` (voice), `MAGNIFIC_API_KEY` (cobalt stills). The CLI also reads `KEY=value` lines from the files in `ATLAS_STUDIO_ENV` (colon-separated, default `~/.config/atlas-site/secrets.env`) without overriding the environment. Never commit a key, never print one. HyperFrames telemetry is disabled by the CLI (`HYPERFRAMES_NO_TELEMETRY=1`).

## Capturing the real app: rules that matter

- Never send input to the user's desktop; drive the app only through the debug bridge (`atlas desktop debug eval`). Capture runs a **separate** app instance with its own `HOME` and `ATLAS_HOSTD_DIR` and records only its window (CGWindowID).
- Never kill the user's apps or the container runtime; the capture tool only stops the pids it spawned, verified by command line.
- Never `rm -rf`/`git clean` at the repo root. Scratch lives in a `mkdtemp` dir and every delete asserts the path is inside it.
- It needs `target/debug/atlas` and `target/debug/bundle/macos/Atlas.app` (see root AGENTS.md "Deploying a local build"), `vhs` for TUI takes, and macOS Screen Recording permission for the terminal that runs it.

## Pinned engine

`hyperframes` is pinned exactly in `package.json`. To bump it: change the pin, `bun install`, `bun studio check`, then re-render `atlas-1-0` and compare frames against the previous render (SSIM per scene; see DECISION.md). Do not bump casually: HyperFrames is 0.x.
