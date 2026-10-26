# Postmortem: ghost frames in Atlas Desktop

## Symptom

After a space switch (worst with a trackpad gesture), a tab switch, a modal open/close or a window restore, the previous picture, or another space's page, stayed on screen for several frames. Recorded by the maintainer on 2026-10-26: an image viewer of one project drawn over every other project; Settings rows drawn over each other (CleanShot GIFs `CleanShot 2026-10-26 at 00.25.59.gif` and `00.31.27.gif`, kept by the maintainer). Every level of the UI was affected: editor, terminal, image viewer, dialogs.

## Hypotheses

From the study of prior art (wry#1848, tauri#13270/#15490, Electron #31776/#42059, Zed's 120 fps notes, WebKit presentation-update SPI):

| # | Family | Hypothesis | Outcome |
|---|---|---|---|
| H1 | Stale WebKit backing store | WKWebView presents a frame older than the committed DOM | Not needed to explain the reports; not reproduced on its own |
| H2 | Transparent page | Ghost pixels come through a transparent page over native layers | **Ruled out**: the transparent page was reverted in e0f3f265 and the ghosts persisted |
| H3 | Native layer lifetime | A native layer (snapshot, terminal) stays visible after the DOM moved on | **Confirmed by code reading**: the native stage's snapshot layers and covers |
| H4 | Occlusion catch-up | Ghosts only after hidden/occluded states | Partial: restore re-ran the stage's warm-ups (covers raised over a page WebKit had not repainted yet) |
| H5 | Async IPC rects | Native rect and DOM disagree for a frame | Not the cause of whole-page ghosts (it moves one terminal, not the page) |

## Mechanism

The **native stage** (`crates/atlas-desktop/src/page_stage.rs` + `page_layers.rs`, driven by `apps/desktop/src/lib/shell/stageSync.ts` and `routes/(app)/SpacePager.svelte`) kept `WKWebView.takeSnapshot` pictures of every space and composited them as `CALayer`s **above the live webview**:

1. During a gesture it raised a snapshot overlay and slid the pictures; it lowered the overlay only after JS acked that the new page painted, or after a 400 ms watchdog. Any DOM change during that window (a modal, a tab, new content) was hidden behind an old picture.
2. To keep the pictures fresh it ran **warm-ups**: raise a *cover* (`pager_cover`, a `takeSnapshot` with `afterScreenUpdates = false`, i.e. whatever WebKit presented last, including an open modal), move a **parked page of another space on screen** under it, wait two frames, snapshot it, park it, lift the cover. When the cover was stale, came up late, failed (`pager_cover` returned false when an overlay was up, after a 2 s timeout or when `takeSnapshot` gave nothing) or lifted early, the user saw the other space's page or the frozen modal. That is exactly the image viewer of one project over the others, and the Settings rows over each other.

The covers and the slide were pictures of the whole pager rect, which is why every level of the UI showed it and gestures made it worst.

## Fix

- The native stage is gone: `page_stage.rs`, `page_layers.rs` (and its non-macOS stub), `stageSync.ts`, `pageDirty.ts`, the six `pager_*` commands and the warm-up wiring in `SpacePager.svelte`. `swipe_mac.rs` only reports engine frames and commits.
- The frontend pager (`lib/shell/pager.ts`) draws every engine frame as GPU transforms of the live layers. Nothing composites a picture of the UI: the screen only ever shows the current DOM.
- Invariant, in code: outside a gesture no non-current space layer is visible on screen (`lib/shell/layerInvariant.ts`, unit-tested). Debug builds check it after every commit and every rest and log `[layers] non-current layer on screen` as a console error; the gesture bench counts it as an inconsistent end state.
- Frame-level regression gate: `scripts/flicker-sweep.ts` ghost-judges transitions (`ghostFrames` in `lib/debug/flickerSweep.ts`): space switches by click and by replayed gesture, dialogs, tabs and window restore. After each transition is at rest, the next 6 frames must match its settled picture; it prints `ghost frames: N` and exits 1 on any. It also fails on any `[layers]` error from the page.

Native views (GPU terminals, browser tabs) are still parked during a gesture (`data-swiping`), as before.

## Per OS

- **macOS**: verified by the flicker sweep in an agent instance (see `docs/desktop.md`, "Flicker sweep").
- **Linux / Windows**: never had the native stage (it was stubbed); they already ran the DOM pager path. Unverified on device.
