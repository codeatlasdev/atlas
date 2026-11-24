# Native GPU terminal on Linux (Wayland) against xterm.js: measured

The Linux native terminal of [#247](https://github.com/codeatlasdev/atlas/issues/247) (PR #774) against the xterm.js terminal the same build uses with the setting off, on the same machine in the same session (2026-11-23). The design it implements (GTK3, `wl_subsurface` below a transparent page, Vulkan only) is `docs/terminal-linux-design.md` on its own branch.

## Method and limits

- Machine: Ryzen 7 7735HS (16 threads), Radeon 680M (RADV `REMBRANDT`, amdgpu), Mesa 26.2.5, WebKitGTK 2.54.1, sway 1.12, Linux 7.2.15, 5.5 GB RAM shared with other agents. **Every number below was taken at 1-minute load under 2** (windows over it are discarded, not footnoted).
- Build: release profile with `debug-assertions` on `atlas-desktop` only (the release app has no debug bridge; this one has it), bundled frontend (`tauri/custom-protocol`). Same binary for both columns; the setting is `ATLAS_GPU_TERMINAL=1` (native) or `0` (xterm.js).
- Display: a private headless sway on the real GPU (`WLR_BACKENDS=headless WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128`), 1440x900 at scale 1, private D-Bus with `--config-file` and no servicedirs, a fresh `HOME` per run. `hyprctl clients -j` listed 1 window before and after every session (the user's own).
- Scripts: [terminal-linux-gpu-tools/](terminal-linux-gpu-tools/) (`up.sh`/`cfg.sh`/`app.sh` start the private display and the instance, `mode.sh`/`ab.sh` the key and memory runs, `flood.sh` runs `scripts/linux-idle-bench.ts`, `gpumem.sh` reads DRM memory, `capture.sh` the screenshots, `k2p/` the key-to-photon tool). Paths inside are the ones used on this machine.
- Raw data: [terminal-linux-gpu-clean/](terminal-linux-gpu-clean/) (every window of the clean table, one JSON line each, discarded windows marked `"valid": false`). The first, noisier set (load above 2 in some windows) was replaced by it and removed.

## Clean numbers (every window at 1-minute load < 2)

The first numbers of this document were taken with other agents loading the machine (some windows above 2). They are replaced by this table: **three runs per column, native and xterm.js alternated** (`terminal-linux-gpu-tools/suite.sh`, `runall` order gpu, xterm, gpu, xterm, gpu, xterm), a window starts only when the 1-minute load is under 2 (`lib.sh` `wait_load`), an idle or key window that ends over 2 and a key run with a missed key are **discarded and repeated** (the raw files list them as `"valid": false`; they are in no cell). The flood window raises the load itself (`yes | head` plus hostd), so only its load *before* is judged. Same release build as #774 (`debug-assertions` on `atlas-desktop` for the debug bridge, bundled frontend), same private headless sway on the Radeon 680M, 1440x900 at scale 1. Raw data: [terminal-linux-gpu-clean/](terminal-linux-gpu-clean/) (`cpu.jsonl`, `mem.jsonl`, `k2p.jsonl`).

| | native, 3 runs | xterm.js, 3 runs |
|---|---|---|
| Flood CPU per MB (`terminal-output`, 30 s) | 33.1 / 35.2 / 33.1 ms | 149.1 / 149.7 / 150.6 ms |
| Flood throughput | 105 / 93 / 105 passes of 20 MB (70 / 62 / 70 MB/s) | 19 / 19 / 19 passes (12.7 MB/s) |
| Idle terminal, total CPU (30 s) | 1.6 / 1.6 / 1.6 % | 0.6 / 0.7 / 0.6 % |
| Idle, app + web process + hostd | 0.7 + 0.6 + 0.3 | 0.1 + 0.2 + 0.3 (0.4 in run 2) |
| Key to photon p50 (100 keys) | 13.28 / 13.33 / 13.41 ms | 22.12 / 22.15 / 22.16 ms |
| Key to photon p95 | 28.05 / 28.48 / 29.04 ms | 31.06 / 30.06 / 30.33 ms |
| VRAM, 0 terminals | 347.5 MB (3 runs) | 334.7 MB (3 runs) |
| VRAM, 1 terminal | 367.1 / 368.1 / 369.1 MB | 349.2 / 349.1 / 349.1 MB |
| VRAM, 4 terminals | 408.8 / 409.8 / 410.8 MB | 367.7 / 365.6 / 364.6 MB |

(Idle and flood windows judged at load before: native idle 0.40 / 1.63 / 0.49, xterm.js 1.85 / 1.48 / 0.55; two native and one xterm.js idle windows were discarded and repeated.) The clean runs agree with the first ones to the digit that matters: **per MB the native terminal costs a fifth** (33-35 ms against 149-151), **an idle native terminal costs about 1 point of a core more than xterm.js** (1.6 % against 0.6-0.7 %: the app thread 0.7, the web process 0.6), the median key lands a frame earlier (13.3 against 22.1 ms; the p95 is the compositor's frame phase, 28-29 against 30-31), and the native terminal holds **about 20 MB of VRAM for the first terminal and 20 MB per terminal for the next three** (xterm.js: 14.5 and 6 MB). Per terminal that is 3.6x xterm.js's marginal cost at 4: the number [#254](https://github.com/codeatlasdev/atlas/issues/254) exists to bring down.

## Not measured here

Hyprland, GNOME or KWin; NVIDIA or Intel; a real display's refresh; scale 1.5 and 2 CPU (they are captured, not benched); several terminals flooding at once; IME; a resize film at 60 fps (the resize and the dock slide are captured as stills, see below).

## Screenshots

[terminal-linux-gpu-shots/](terminal-linux-gpu-shots/): an HTML menu (the project's "..." menu, z-index above the page) drawn over a running terminal at scale 1, 1.5 and 2 (the output is 1440x900, 2160x1350 and 2880x1800 physical pixels; the webp are resized to 1440 wide), the 1:1 crop of glyphs at scale 2, the window resized to 900x560 by sway, and the left dock sliding mid-animation. In all of them the menu is over the terminal's text and the terminal fills its slot.

## Fallback proof (no blank terminal)

With the setting on (`ATLAS_GPU_TERMINAL=1`), a terminal tab running `echo hello-from-<case>`:

| session | what happened | result |
|---|---|---|
| sway on pixman (no linux-dmabuf; RADV finds nothing to present to) | `term_capability` says available (the adapter is fine); `term_open` fails: Vulkan lists **no surface format**. Before the fix this was `caps.formats[0]` panicking on the main thread ("index out of bounds" in the toast) | xterm.js drew the text; toast: "The fast terminal could not start (the surface lists no color format: the adapter cannot present to this display). Terminals use the classic renderer until you restart Atlas." |
| no Vulkan driver (`VK_ICD_FILENAMES=/nonexistent`) | `term_capability`: unavailable, "no usable GPU: No suitable graphics adapter found ... vulkan drivers/libraries could not be loaded"; the page toasts the reason once at launch (the toast lives a few seconds: caught by polling, `fallback-no-vulkan-toast.webp`) | xterm.js drew the text |

Screenshots: `fallback-pixman-toast.webp`, `fallback-no-vulkan-toast.webp`. Tests: `pick_format` (no format is an `Err`), `refusal`, `useNativeTerminal` (every way out mounts xterm.js), `NativeView` with a failing `term_open` and a `term://unusable` event (one toast, `gpu.failed`), `capability::decide`/`incident` for each Rust-side reason.

### Forced failures (a seam, not a missing driver)

`ATLAS_TERM_FAULT` (debug-assertions builds only: `term/linux/fault.rs`) makes the renderer creation fail, so the two paths the missing-driver cases above do not reach are proved live on the GPU sway, with the setting on (`ATLAS_GPU_TERMINAL=1`) and a terminal running `echo hello-from-<case>`:

| `ATLAS_TERM_FAULT` | what the code does | result |
|---|---|---|
| `open` | every `Renderer::new_raw` fails: `term_open` fails | xterm.js drew the text; toast "The fast terminal could not start (injected fault: renderer creation refused). Terminals use the classic renderer until you restart Atlas." ([screenshot](terminal-linux-gpu-shots/fallback-injected-open-failure.png)) |
| `rebuild` | the first creation works (the terminal opens native), every later one fails; three "Reload terminal" in a row | the third failed rebuild sends `term://unusable`; the toast "The fast terminal could not start (injected fault: renderer rebuild refused). ..." showed, every terminal went back to xterm.js and the text was drawn ([screenshot](terminal-linux-gpu-shots/fallback-injected-rebuild-failure.png)) |

A seam test (`fault::tests`) pins the spec: `open` fails every call, `rebuild` only the later ones, nothing else injects.
