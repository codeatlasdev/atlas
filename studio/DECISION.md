# Decision: HyperFrames as the render engine

Status: accepted, 2026-10-12. Owner: whoever maintains `studio/`.

Atlas needs one place that builds every published video (the site film, its vertical cut, release videos) from code, re-renders deterministically on a laptop or in CI, and is easy for coding agents to extend. This records what was compared, what was measured, and why the studio renders with **HyperFrames**.

## The requirement that decides it: licensing

Atlas is a public repository under `MIT OR Apache-2.0`. Anyone may clone it, run the studio and render a film, and the repo itself is not a company with a headcount.

| Engine | License | Fits a public MIT/Apache repo? |
|---|---|---|
| **Remotion** 4.0.534 | Remotion License: *source-available, not OSI open source* ([FAQ](https://www.remotion.dev/docs/license/faq#is-remotion-open-source)). Free for individuals, non-profits and for-profit organizations **up to 3 people**; a Company License is required above that ([LICENSE.md](https://github.com/remotion-dev/remotion/blob/main/LICENSE.md), [pricing](https://www.remotion.dev/docs/license/pricing)): $25/month per person who writes Remotion code (including through agentic coding tools) or $0.01 per render with a $100/month minimum for automations. `npx remotion render`, `renderMedia()` and `<Player>` all count as "automation". | **No.** The studio would pull a proprietary-licensed dependency into an MIT/Apache repo. Every contributor and every company that forks it inherits a headcount test and possible fees. We cannot offer "clone and render" under our own license. |
| **HyperFrames** 0.8.149 | [Apache-2.0](https://github.com/heygen-com/hyperframes/blob/main/LICENSE), no per-render fees, no commercial-use threshold ([README](https://github.com/heygen-com/hyperframes#why-hyperframes)). | **Yes.** Same family as our own license; `cargo deny`-style review is trivial. |
| **Motion Canvas** 3.17.2 | MIT ([repo](https://github.com/motion-canvas/motion-canvas)). | Yes. |
| **Revideo** 0.11.0 | MIT ([repo](https://github.com/redotvideo/revideo)); last npm publish 2025-02. | Yes, but see below. |

Remotion is an excellent tool and the ecosystem leader (its own comparison says so: [HyperFrames or Remotion?](https://hyperframes.heygen.com/guides/hyperframes-vs-remotion)). It is ruled out here by the license, not by quality. The sibling `ac` repo's studio uses Remotion legitimately for a private product; that does not transfer to a public MIT/Apache project.

## Comparison for this use

Weighted by what Atlas films actually are: real app recordings and terminal takes inside branded frames, animated title and code cards, captions from narration timing, voiceover with a ducked music bed, 16:9 plus 9:16 plus 1:1, reproducible re-renders.

| Criterion | HyperFrames | Remotion | Motion Canvas | Revideo |
|---|---|---|---|---|
| Authoring model | HTML + CSS + a seekable GSAP timeline; the DOM is the frame | React components; pure function of the frame number | Generator functions in TSX, canvas-drawn | Fork of Motion Canvas; TSX, canvas |
| Real video clips with crop/zoom/pan | `<video>` + CSS transform; framework owns seek/decode | `<OffthreadVideo>`; mature | Video node; weaker for long UI footage | Video node; core use case (its README targets video editing) |
| Rich text, syntax-highlighted code, UI chrome | Native HTML/CSS (shiki output drops in as HTML) | Native React/CSS | Canvas text only; code via a plugin | Canvas text only |
| Brand fonts/tokens shared with the website | Same CSS, same woff2 files | Same | Re-implemented on canvas | Re-implemented on canvas |
| Determinism | Seek-based; the renderer jumps the timeline to each frame. **Measured:** two renders of one composition decode to identical pixels (ffmpeg PSNR = inf on all 180 frames) | Deterministic if you only read the frame number | Deterministic | Deterministic |
| Agent authoring | Plain HTML files; `check`/`lint` validate the contract; ships 21 agent skills (`/hyperframes`, `/hyperframes-core`, `/media-use`, ...) that the maintainers' agents already load | JSX project; strong docs, agent skills exist | Small community | Small, slowing |
| Local render, macOS arm64 | **Measured:** a 6 s 1080p30 scene with a 1520 px UI clip, title, and audio: 8.5-9.9 s. Hardware-GPU Chrome probe passed on M4 Pro | **Measured:** the same scene, 21.5 s including bundling; needed `linker = "hoisted"` and an extra `@babel/parser` under bun | Browser-based editor; headless render exists | Headless render exists |
| Headless / CI | `hyperframes render` (non-interactive, `--docker` for byte-reproducible containers, `--json`) | `remotion render`; Lambda is the mature cloud path | Puppeteer render | Node render API |
| Preview | `hyperframes preview` (Studio: edit DOM, timeline, save back to source) | Remotion Studio | Editor | Editor |
| Maintenance (npm `modified`, 2026-10) | active, weekly releases | active | active (2026-07) | stale (2025-02) |
| License fit | Apache-2.0 | proprietary tiers | MIT | MIT |

Why not Motion Canvas or Revideo: both draw to a canvas, so every card, caption and code block would be re-implemented off the brand CSS and the website fonts, and a UI-recording-heavy film is the weak side of both. Revideo has not published since 2025-02.

## What we adopt, and how we keep the downside small

1. **Engine:** `hyperframes` (CLI + engine) pinned exactly in `studio/package.json`; a pin bump goes through `bun studio check` plus a frame-diff against the reference film (see `AGENTS.md`).
2. **Authoring on top of it:** films are **typed data** (`studio/films/<id>/film.ts`); typed scene builders emit one HTML composition per film and aspect ratio (`studio/.build/`, gitignored). Authors and agents edit TypeScript with types and errors; the HTML contract rules that bite (paused single timeline, no wall clocks, clip nesting, unique ids, media `id`s) are enforced in one place, the compiler, instead of in every scene.
3. **Audio outside the browser:** HyperFrames renders the silent picture. Narration and music are mixed with `ffmpeg` (sidechain ducking, `loudnorm`) and the integrated loudness is **measured and asserted** (-14 LUFS, tolerance 0.5 LU). The in-browser Web Audio mixer is not used, because we want a measured number, not a preview-matches-render promise.
4. **No lock-in at the data layer:** film data, narration, chapters and captions are plain JSON/TS and independent of the engine. If HyperFrames stopped being viable, only `studio/src/compile/` and the render command change; the films do not.
5. **Not used:** HyperFrames cloud/Lambda/Cloud Run rendering, telemetry-bearing `feedback`/`events` commands (`HYPERFRAMES_NO_TELEMETRY`/`DO_NOT_TRACK` is set by the studio CLI), the Framey desktop app.

## Risks accepted

- HyperFrames is young (0.8.x); its API can move. Mitigated by the exact pin and the reference-film regression check.
- Chrome is required (`hyperframes doctor` finds a puppeteer-cached `chrome-headless-shell`); first render downloads it.
- Frame-identical output across machines is only promised by `--docker`; locally we assert decoded-frame equality between two renders on the same machine and a loudness/duration/beat tolerance across machines.

## Sources

- Remotion: [License FAQ](https://www.remotion.dev/docs/license/faq), [LICENSE.md](https://github.com/remotion-dev/remotion/blob/main/LICENSE.md), [Terms](https://www.remotion.dev/docs/terms)
- HyperFrames: [repo](https://github.com/heygen-com/hyperframes), [docs](https://hyperframes.heygen.com/introduction), [vs Remotion](https://hyperframes.heygen.com/guides/hyperframes-vs-remotion)
- Motion Canvas: [repo](https://github.com/motion-canvas/motion-canvas) (MIT). Revideo: [repo](https://github.com/redotvideo/revideo) (MIT).
- Our own measurements (this repo, M4 Pro, ffmpeg 9.0.2, Node 24): HyperFrames 8.5 s and 9.9 s for two renders of a 6 s scene vs Remotion 21.5 s, identical decoded frames between the two HyperFrames renders.
