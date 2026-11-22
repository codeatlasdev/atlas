# Terminal output on a binary channel: before and after

Cause 3 of [linux-webkit-study.md](linux-webkit-study.md) (a flooded xterm.js terminal: 383 % / 425 % of a core, web process 205 → 1,153 MB), fixed by `pty_stream.rs` / `pty_feed.rs` / `lib/ptyChannel.ts` (design in [terminal.md](../terminal.md)). Same machine, same method, same scenario (`scripts/linux-idle-bench.ts --scenario terminal-output`: `yes … | head -c 20M` in a loop, unthrottled, 10 s settle, 30 s window), debug builds of both sides, private headless sway per run (GPU = `gles2` on `renderD128`, llvmpipe = `pixman`). **before** = `e915c5cb` (main + the study), **after** = this branch. Runs alternate before, after, before, after, before, after on a fresh instance each. Raw data: `terminal-binary-channel-gpu.jsonl`, `terminal-binary-channel-llvmpipe.jsonl`.

## terminal-output, CPU % of one core over 30 s (mean of 3) and PSS MB at the end of the window

| display | | total | app | web | hostd | web PSS | total PSS |
|---|---|---|---|---|---|---|
| GPU | before | **380.0** | 152.3 | 102.3 | 125.2 | 1,462 | 1,703 |
| GPU | after | **239.8** | 61.0 | 128.9 | 49.7 | 368 | 598 |
| llvmpipe | before | **417.9** | 169.9 | 123.8 | 124.2 | 1,302 | 1,480 |
| llvmpipe | after | **280.2** | 77.4 | 146.3 | 56.5 | 389 | 649 |

Idle (welcome screen, same builds): 0.2 % and 329 / 257 MB (GPU, before / after) and 467 / 497 MB (llvmpipe): no change at rest.

Load (1-minute average from other agents' builds): GPU 3.0-4.2, llvmpipe 3.0-8.1; the llvmpipe rows 2 and 3 of both variants and the GPU `after-3` row are over the "valid below 4" line and had 33-333 MB swapped out, so read them for CPU (which barely moves with load, before 415/415/423, after 279/269/293) and not for PSS.

## Throughput, 200 MB flood (GPU display, load 2.5-3.2, two runs each)

| | MB/s the program could print | CPU-seconds per 100 MB (app + web + hostd) |
|---|---|---|
| before | 20.6, 18.6 | 18.0, 18.1 |
| after | 23.6, 24.1 | 13.8, 14.1 |

A program that prints flat out keeps the page's main thread parsing at ~90-100 % either way (xterm.js parses 5-35 MB/s); what changed is that the work is no longer wasted on bytes that are queued in WebKit, dropped past the backlog and sent again, and that hostd, the app and the web process no longer encode and decode base64 (hostd 125 → 50 %, app 152 → 61 %). The 200 % target is not met in this scenario: the total is 240 % (GPU) and 280 % (llvmpipe) of a core, of which the web process alone is 129-146 % and the app + hostd 110-134 % in a **debug** build (opt-level 0 inflates exactly those two; the study says the same). A release build must re-run this before a budget is set.

## Release build (`--release`, the profile the shipped app uses)

The debug numbers above inflate the app and hostd (opt-level 0). Same scenario, same method (30 s window after a 10 s settle, private headless sway, a fresh instance per run, runs alternating before/after), **release** builds of both sides: **before** = `e915c5cb`, **after** = `ab27b4e2` (#734). Two build details: `atlas-desktop` is built with `debug-assertions` on (`--config profile.release.package.atlas-desktop.debug-assertions=true`, opt-level stays 3), because an agent instance (`ATLAS_AGENT`, the debug socket) only exists when `debug_assertions` is on and a release app is always the user's; the frontend is the bundled build (`tauri/custom-protocol`). Released with `CARGO_BUILD_JOBS=2` (5.5 GB of memory).

| display | | total CPU | app | web | hostd | web PSS | total PSS |
|---|---|---|---|---|---|---|
| GPU | before | **403.6 %** | 97.8 | 114.4 | 191.1 | 2,198 MB | 2,273 MB |
| GPU | after | **230.5 %** | 56.9 | 127.0 | 46.5 | **317 MB** | 493 MB |
| llvmpipe | before | **440.7 %** | 112.6 | 127.6 | 200.2 | 2,161 MB | 2,228 MB |
| llvmpipe | after | **271.8 %** | 72.4 | 144.5 | 54.8 | **349 MB** | 580 MB |

**The < 200 % target does not hold in release either**: 230 % (GPU) and 272 % (llvmpipe). Going from debug to release moved the *before* hostd from 125 to 191-200 % and the app from 152 to 98-113 %, but the *after* side hardly moved (app 61 → 57 %, hostd 50 → 47 % on GPU): the new path was not CPU-bound in app and hostd to begin with. What is left is the web process, 127 % (GPU) and 145 % (llvmpipe), which is xterm.js parsing a flat-out flood on one thread (89-99 % of one core in the page's main thread) plus the compositor and `ReceiveQueue` threads; that is not the channel's cost. Web PSS after the flood is 317-349 MB against 2.2 GB before (the before side grows further in release because more output piles up in the page in the same time).

Every run (`load` = 1-minute load average at the start, after waiting for it to fall under 2, and at the end of the window; the end figure contains the app's own 2.3-4 cores, so it says little about other jobs):

| display | run | start load | end load | total CPU | app | web | hostd | web PSS |
|---|---|---|---|---|---|---|---|---|
| GPU | before 1 | 1.68 | 5.62 | 420.7 | 103.2 | 115.0 | 202.2 | 2,282 |
| GPU | after 1 | 1.98 | 3.00 | 231.5 | 57.8 | 127.6 | 46.1 | 322 |
| GPU | before 2 | 1.87 | 6.21 | 372.1 | 87.4 | 113.9 | 170.4 | 1,921 |
| GPU | after 2 | 1.88 | 4.37 | 231.5 | 57.0 | 127.0 | 47.4 | 302 |
| GPU | before 3 | 1.89 | 4.97 | 418.0 | 102.7 | 114.4 | 200.7 | 2,390 |
| GPU | after 3 | 1.96 | 2.58 | 228.6 | 56.0 | 126.5 | 46.0 | 326 |
| llvmpipe | before 1 | 1.91 | 5.87 | 442.2 | 112.8 | 131.5 | 197.6 | 1,905 |
| llvmpipe | after 1 | 1.94 | 5.96 | 288.4 | 72.3 | 148.7 | 67.3 | 354 |
| llvmpipe | before 2 | 1.33 | 5.18 | 443.2 | 112.7 | 128.9 | 201.3 | 2,221 |
| llvmpipe | after 2 | 1.90 | 3.52 | 264.4 | 72.7 | 141.8 | 49.8 | 342 |
| llvmpipe | before 3 | 1.99 | 4.30 | 436.7 | 112.2 | 122.5 | 201.7 | 2,356 |
| llvmpipe | after 3 | 1.42 | 2.84 | 262.5 | 72.2 | 142.9 | 47.4 | 350 |

Start loads are all under 2. The end loads of the *before* runs are over 4 by their own doing (they burn 3.7-4.4 cores), so the usual "discard at >= 4" would discard every before run, and I kept them; the CPU of the same variant moved by less than 6 % between runs whose end load differs by 3. llvmpipe before 2 swapped 900 MB (its PSS is the web process's resident part only); llvmpipe before 2 and after 3 were re-run after an instance failed to open its window (`main window not found`), the first attempts left no data. **Discarded**: a first GPU set (`terminal-binary-channel-release-gpu-discarded.jsonl`) run without waiting for the load to fall: start loads 3.0-8.7, total CPU 404 / 231 / 230 / 367 / 232 %, same picture.

Raw data: `terminal-binary-channel-release-gpu.jsonl`, `terminal-binary-channel-release-llvmpipe.jsonl`.
