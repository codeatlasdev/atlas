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
