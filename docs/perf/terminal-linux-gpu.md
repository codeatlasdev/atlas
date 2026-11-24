# Native GPU terminal on Linux (Wayland) against xterm.js: measured

The Linux native terminal of [#247](https://github.com/codeatlasdev/atlas/issues/247) (PR #774) against the xterm.js terminal the same build uses with the setting off, on the same machine in the same session (2026-11-23). The design it implements (GTK3, `wl_subsurface` below a transparent page, Vulkan only) is `docs/terminal-linux-design.md` on its own branch.

## Method and limits

- Machine: Ryzen 7 7735HS (16 threads), Radeon 680M (RADV `REMBRANDT`, amdgpu), Mesa 26.2.5, WebKitGTK 2.54.1, sway 1.12, Linux 7.2.15, 5.5 GB RAM shared with other agents. **Load was not quiet on every run**: each row says its load. Rows with load above 2 are marked; do not read a 10 % difference out of them.
- Build: release profile with `debug-assertions` on `atlas-desktop` only (the release app has no debug bridge; this one has it), bundled frontend (`tauri/custom-protocol`). Same binary for both columns; the setting is `ATLAS_GPU_TERMINAL=1` (native) or `0` (xterm.js).
- Display: a private headless sway on the real GPU (`WLR_BACKENDS=headless WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128`), 1440x900 at scale 1, private D-Bus with `--config-file` and no servicedirs, a fresh `HOME` per run. `hyprctl clients -j` listed 1 window before and after every session (the user's own).
- Scripts: [terminal-linux-gpu-tools/](terminal-linux-gpu-tools/) (`up.sh`/`cfg.sh`/`app.sh` start the private display and the instance, `mode.sh`/`ab.sh` the key and memory runs, `flood.sh` runs `scripts/linux-idle-bench.ts`, `gpumem.sh` reads DRM memory, `capture.sh` the screenshots, `k2p/` the key-to-photon tool). Paths inside are the ones used on this machine.
- Raw data: [terminal-linux-gpu-flood-idle.jsonl](terminal-linux-gpu-flood-idle.jsonl) (one `linux-idle-bench` window per line), [terminal-linux-gpu-mem.jsonl](terminal-linux-gpu-mem.jsonl), [terminal-linux-gpu-k2p.jsonl](terminal-linux-gpu-k2p.jsonl) (every key).

## Flood and idle CPU

`scripts/linux-idle-bench.ts` scenarios `terminal-output` (`yes <log line> | head -c 20M` in a loop, 30 s window after 10 s settle) and `terminal-idle` (an interactive `bash`, 30 s). Two alternated runs per column, total CPU of the app + web process + hostd (100 = one core).

| | run | load before | 20 MB passes in 30 s | throughput | total CPU | app | web process | hostd | **CPU per MB** |
|---|---|---|---|---|---|---|---|---|---|
| native | 1 | 1.47 | 94 | 63 MB/s | 218.8 % | 105.2 | 1.0 | 112.4 | **34.9 ms** |
| native | 2 | 1.96 | 105 | 70 MB/s | 229.1 % | 109.7 | 1.1 | 118.1 | **32.7 ms** |
| xterm.js | 1 | 1.33 | 19 | 12.7 MB/s | 189.7 % | 59.7 | 105.5 | 24.5 | **149.8 ms** |
| xterm.js | 2 | 2.05 | 18 | 12.0 MB/s | 193.8 % | 61.2 | 107.6 | 25.0 | **161.5 ms** |

CPU per MB = total CPU seconds in the window / MB the flood wrote (passes x 20). The flood is unthrottled, so each terminal runs at its own ceiling: the page's terminal is held to ~12 MB/s by its acknowledgement window and its single web-process core, the native one reads at 63-70 MB/s. **Per MB the native terminal costs about a fifth** (33-35 ms against 150-162 ms), and the web process, which is the one-core bottleneck of the xterm.js path, is at 1 % instead of 106 %. hostd is the new ceiling: 112-118 % of a core feeding the native reader (the same 18-19 ms per MB it spends feeding xterm.js, at five times the rate).

| idle terminal, 30 s | load before | total CPU | app | web process | hostd | PSS (sum) |
|---|---|---|---|---|---|---|
| native, run 1 | 0.99 | 1.6 % | 0.7 | 0.6 | 0.3 | 347 MB |
| native, run 2 | 1.53 | 1.7 % | 0.7 | 0.7 | 0.3 | 433 MB |
| xterm.js, run 1 | 1.82 | 0.3 % | 0.1 | 0.1 | 0.1 | 370 MB |
| xterm.js, run 2 | 1.54 | 0.7 % | 0.1 | 0.2 | 0.4 | 438 MB |
| xterm.js, run 3 | 1.97 | 0.6 % | 0.1 | 0.2 | 0.3 | 366 MB |

**An idle native terminal costs about 1 point of a core more than xterm.js** (1.6-1.7 % against 0.3-0.7 %): the app thread 0.7 % (the cursor blink draws a frame every 530 ms, and the 1 s scale/toplevel tick) and the web process 0.6-0.7 % (a transparent window the compositor blends). The design doc's acceptance for an idle terminal was 0.5 % of a core; this is not met by the whole-instance total and is the number to move before default-on. PSS differs by run more than by mode.

## Key to photon

`k2p/` types `a` and BackSpace alternately through the compositor's virtual keyboard (`zwp_virtual_keyboard_v1`, the same input path a keyboard takes), stamps `CLOCK_MONOTONIC` just before the request is flushed, and captures the output with `wlr-screencopy` `copy_with_damage`: every composited frame arrives with the compositor's own `CLOCK_MONOTONIC` commit timestamp. The latency of a key is the timestamp of the first frame whose pixels in the prompt region differ from the settled frame before the key, minus the key stamp. A control run with no keys saw 0 changes in 4 s, so a blinking cursor is not counted as an echo (the region excludes it by settling 350 ms and by the xterm.js cursor not blinking in this build; the native cursor is solid while the terminal has focus). The clock check printed a frame stamp 1.5-3.6 ms before it was read, so the clocks agree. The headless output refreshes at 60 Hz, so every value is quantised by 16.7 ms frames.

n = 100 keys per run, 0 missed, one terminal at 1440x900.

| | run | load | p50 | p95 | max | min |
|---|---|---|---|---|---|---|
| native | 1 | 0.46 | **13.4 ms** | 27.9 | 29.7 | 11.4 |
| xterm.js | 1 | 0.72 | **22.1 ms** | 30.5 | 32.0 | 16.4 |
| native | 2 (load > 2) | 3.66 | **14.1 ms** | 29.4 | 30.9 | 12.0 |
| xterm.js | 2 (load > 2) | 4.10 | **22.8 ms** | 30.8 | 49.7 | 15.7 |

The native terminal's median key lands one composited frame earlier than xterm.js (13-14 ms against 22-23 ms); the p95 is two frames for both (28-31 ms), i.e. in this method the tail is the compositor's frame phase, not the terminal. The in-process figure of the design doc (0.8 / 2.2 ms to the wgpu present call) plus the compositor's next frame is consistent with the native p50. Key-to-photon on the real display (Hyprland, a 60-165 Hz panel) is not measured here.

## GPU memory per terminal

From `/proc/<pid>/fdinfo` (`drm-total-vram`, `drm-total-gtt` of the amdgpu client, each client counted once) summed over the app, its WebKit web process and hostd, with 0, 1 and 4 terminal tabs open (all tabs mounted, one in front). Two runs per column; the first row of each pair is the run with the lower load.

| terminals | native VRAM | native GTT | xterm.js VRAM | xterm.js GTT |
|---|---|---|---|---|
| 0 | 359.9 / 357.9 MB | 32.1 / 32.1 MB | 342.7 / 332.5 MB | 32.1 / 32.1 MB |
| 1 | 377.9 / 379.0 MB | 42.0 / 35.8 MB | 357.5 / 347.3 MB | 48.5 / 46.4 MB |
| 4 | 420.7 / 421.7 MB | 40.9 / 43.0 MB | 369.3 / 366.2 MB | 48.5 / 46.4 MB |

(VRAM in MiB converted from KiB, rounded.) Per terminal: **native ~15 MB of VRAM each** (the app's client grows 227.6 → 240.0 → 277.1 MB: 12.4 MB per terminal, a glyph atlas, a colour atlas, buffers and a swapchain), **xterm.js ~7.5 MB** (the web process's client, 119.5 → 134.4 → 146.1 MB: +14.8 for the first, +3.9 for each next, its WebGL addon sharing one context). The native terminal uses about twice the GPU memory per terminal, and all of it stays allocated while a terminal is hidden: [#254](https://github.com/codeatlasdev/atlas/issues/254) (release the GPU of hidden terminals) is what brings that down.

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
