# Atlas media

All footage is real: recorded from the Atlas Desktop debug build (window capture via ScreenCaptureKit, no input sent to the user's desktop), the real `atlas dev` TUI recorded with `vhs` (`demo/demo.tape` on the demo stack `demo/atlas.yaml`), and the real `atlas mcp` server. Demo projects live under `/tmp` (orbit-shop and friends); the agent CLI in the agents scenes is a scripted stand-in that posts the same hook events as the omp extension.

## Provenance

- **Backgrounds:** Magnific FLUX.2 pro stills in the site's cobalt palette (`#1b33f0` / `#0b1672`, no violet): `cobalt_a` (light field), `cobalt_b` (orbital rings), `cobalt_c` (planet horizon), `cobalt_v` (vertical). Prompts and seeds (21-24) are in `../../../studio/src/backgrounds.ts` (`bun studio bg`). They replace the v1 violet nebula set.
- **Cards and YAML scene:** Archivo + JetBrains Mono (the site fonts), YAML highlighted with shiki on a custom cobalt theme; rendered as HTML cards by the studio (`../../../studio/src/scenes/`).
- **TUI scene:** the current `atlas dev` log UX (restart separators, `]e` error jumps, `}`/`{` link focus with URL and `path:line`, `/` find with `n/N` counter, JSON compaction, `v` level filter, `V` time range), a dense 8 s cut of the vhs take (`demo/demo.tape`, 1600x1000, JetBrains Mono, Catppuccin Mocha).
- **Music:** ElevenLabs Music (`music_v1`, instrumental, generated for this film). Licence caveat: generated music, used under the ElevenLabs plan terms of the account that generated it; verify those terms cover your distribution before using the film outside this site.
- **Voice:** ElevenLabs text-to-speech, model `eleven_v4`, premade voice "Eric - Smooth, Trustworthy" (`cjVigY5qzO86Huf0OWal`), delivery tag `[confident, warm, modern product launch narrator]`, stability 0.5 / similarity 0.7. English narration written from CHANGELOG/docs claims only (`../../../studio/films/atlas-1-0/film.ts`, both cuts). Synthetic voice: disclose it as AI-generated wherever required.
- **Mix:** voice compressed and normalised to -14 LUFS; music at -20 LUFS bed level, sidechain-ducked by the voice (about 10-12 dB under speech, back to full in the gaps); `../../../studio/src/audio/mix.ts`. The studio measures the loudness and the peak of the DELIVERED mp4 and webm (AAC/Opus overshoot the mixed wav by 0.7-0.9 dB, which once shipped files at -0.8/-0.6 dBFS) and fails the render outside -14 LUFS ±0.7 or above a -1 dBFS peak; its report (`studio/out/<film>/<cut>.report.json`) holds the measured values. The 1.0 files published before the studio measured -14.6 and -14.3 LUFS.
- **Build:** the studio, `../../../studio/` (`bun studio render atlas-1-0`, then `bun studio publish atlas-1-0`): HyperFrames renders the picture from typed film data, ffmpeg mixes and measures the audio. It replaces the ad-hoc `site/scripts/film/` scripts. Raw recordings, the vhs take, stills, voice and music are cached outside git in `studio/.cache/` with a sha256 manifest. `manifest.json` here lists the content hash of each published file; `film-chapters.json` is the chapter list the player reads. Keys come from the `ELEVENLABS_API_KEY` / `MAGNIFIC_API_KEY` environment variables. See `../../../studio/AGENTS.md` and `../../../studio/DECISION.md`.

| File | Spec |
|---|---|
| `film.mp4` / `film.webm` / `film-poster.jpg` / `film.vtt` | 1920x1080, 30fps, 77.9s, H.264+AAC / VP9+Opus, English voiceover over music; `film.vtt` captions are the narration text |
| `film-poster.jpg` | also the hero image of the repository README. Re-render only it with `bun studio still atlas-1-0`, publish with `bun studio publish atlas-1-0 --stills` (both lint it first: the window must be whole, with its titlebar, corners and headroom). Before 2026-10-12 it was cropped at the window's top (`crop y:40`) |
| `film-thumbs.jpg` / `film-thumbs.vtt` | scrub preview sprite (rendered by the studio) |
| `film-chapters.json` / `manifest.json` | chapter list for the player (written by `bun studio publish`); content hashes of the published files |
| `film-vertical.mp4` / `film-vertical-poster.jpg` | 1080x1920, 30s, social cut with a shorter voiceover; a keyframe at least every 2 s |
| `loop-{workspace,panel,logs,terminal,agents,mcp}.mp4` / `.webm` / `-poster.jpg` | 1280x800 (16:10), silent, 9-14s, <600KB. Recorded by `recipes/site-media.ts` (`bun studio run`), cut and encoded by `bun studio loops` (`studio/src/site-media.ts` says what each is made of). The poster is the loop's first frame |
| `loop-tui.mp4` / `.webm` / `-poster.jpg` | 1280x800, 24 fps, silent, 8s, <600KB, the current `atlas dev` log UX; cut from the vhs take (`bun studio capture tui tui`) by `bun studio loops` |
| `shot-{workspace,panel,logs,terminal,agents}.jpg` | 2000px wide stills of the real app: the window PNG of the matching `shot` step of the recipe |

The app footage and stills show Atlas Dark, the app's default theme (the capture's seeded settings set no theme). Captures are 1248x780 windows (2496x1560 frames at 2x), the largest the agent display admits; `bun studio loops` scales them to the loops' 1280x800.

## How the lint judges them

`bun studio lint site/public/media` must report 0 errors. Loops and `.webm` siblings use the `site-loop` / `site-sibling` profiles, and `shot-*.jpg` and the loops' posters use `site-shot`: all `captured`, so the app's own UI text (13 px at 1x, 6-10 px when a full window is shown at 506 px) is not judged against a legibility floor that only authored titles can meet. Framing (the window whole, with headroom) and the audio/spec rules apply to every file. The film posters are composed frames and keep the strict `still` profile.
