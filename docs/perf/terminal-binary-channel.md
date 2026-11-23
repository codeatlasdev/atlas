# Terminal output on a binary channel: before and after

Cause 3 of [linux-webkit-study.md](linux-webkit-study.md) (a flooded xterm.js terminal: 383 % / 425 % of a core, web process 205 → 1,153 MB), fixed by `pty_stream.rs` / `pty_feed.rs` / `lib/ptyChannel.ts` (design in [terminal.md](../terminal.md)). Same machine, same method, same scenario (`scripts/linux-idle-bench.ts --scenario terminal-output`: `yes … | head -c 20M` in a loop, unthrottled, 10 s settle, 30 s window), debug builds of both sides, private headless sway per run (GPU = `gles2` on `renderD128`, llvmpipe = `pixman`). **before** = `e915c5cb` (main + the study), **after** = this branch. Runs alternate before, after, before, after, before, after on a fresh instance each. Raw data: `terminal-binary-channel-gpu.jsonl`, `terminal-binary-channel-llvmpipe.jsonl`.

## The re-attach loop, and the numbers after its fix

**Every number below the next heading was measured with a bug**: each stream ended in EOF right after hostd's replay, and the page re-attached, replayed and re-parsed ~10 times a second (a plain terminal as much as a chat tab). The `chat-stream` scenario showed it (main 31-39 % total, this branch 208-233 %, nothing else running); the cause was not flow control.

- **Not the acks.** A build that logs why each stream ends: `drop_stream=false` on every stream (the 2 MiB window was never full, the 60 s no-acknowledgement path never ran), `read = Ok(0)` (EOF) every time, 1-2 ms after the replay frame. Every acknowledgement is sent: with the fix, one chat stream gets 671 acks in 22 s, the first one for the 418 bytes of replay, 325 KB in all (`ChatTab`'s `write` calls `done()` at once).
- **Cause.** `pty_stream` moved only `raw.reader` into the feed task; closures capture disjoint fields in edition 2024, so the rest of the connection, including its write half, dropped when the command returned. hostd reads the end of a raw connection's incoming side as "the client left" (`server.rs`, `raw()`: "any line or EOF ends it") and closes it. hostd's trace: `client line/eof None` on every connection, right after the replay write. The fix moves the whole connection into the task; a test against the real hostd (`a_stream_stays_open_after_the_replay_and_carries_live_output`) fails without it with frame kinds `[1, 2]` (replay, dropped) and passes with it.
- **Attaches** in 15 s after the terminal settled (debug hostd, pixman): chat tab 1,695 before the fix and 0 after (1 in all); plain terminal (`sh` printing a line every 50 ms) 1,374 and 0. The backoff added in `bc521aa0` stays: it limits the damage of any other host that closes streams at once, but it was only ever a symptom limiter here.

`chat-stream`, GPU, release, 3 alternated runs (start load < 2 on every run; `terminal-binary-channel-fixed-chat-gpu.jsonl`). **before** = `e915c5cb` (main + study), **after** = `b7553418`:

| | total CPU | app | web | hostd | web PSS |
|---|---|---|---|---|---|
| before (main) | 30.3 % | 10.7 | 16.8 | 2.6 | 250 MB |
| after, with the loop (`ab27b4e2`, 3 runs) | 208-233 % | 81-87 | 120-137 | 6.3-7.6 | |
| after, fixed | **34.9 %** | 13.5 | 18.4 | 2.7 | 251 MB |

| run | start load | end load | total CPU |
|---|---|---|---|
| before 1 | 1.70 | 1.81 | 30.5 |
| after 1 | 1.66 | 1.14 | 35.0 |
| before 2 | 1.13 | 0.92 | 30.2 |
| after 2 | 0.93 | 0.77 | 34.7 |
| before 3 | 0.71 | 0.45 | 30.3 |
| after 3 | 0.38 | 0.35 | 34.9 |

The +4.6 points that remained were the acknowledgements. Per frame the two paths do the same work: at most one frame per 16 ms (`FRAME_EVERY`; the fixture's ~30 updates a second make ~30 a second here, ~485 bytes each (main's `pty_batch` has the same 16 ms batch; I did not count its events), below Tauri's 1 KiB raw threshold, so a frame is one `eval` in the page, as main's event is), `decodeFrame` is a `subarray` (no copy), and ChatTab's `TextDecoder` is the same as main's after `atob`. The one extra call per frame is `pty_stream_ack`, an IPC round trip into Rust: 671 of them in 22 s (~30 a second) to credit 325 KB. Test by removal, release GPU, 3 alternated runs each, start load < 2 (gate), `terminal-binary-channel-chat-acks-gpu.jsonl`:

| variant | total CPU (runs) | app | web | app main thread | app `ReceiveQueue` | web `ReceiveQueue` |
|---|---|---|---|---|---|---|
| main (`e915c5cb`) | 30.3 % (30.4, 30.4, 30.2) | 10.6 | 16.8 | 8.4 | 1.3 | 1.3 |
| one ack per frame (`4e4a5de4`) | 34.6 % (34.8, 34.5, 34.6) | 13.5 | 18.3 | 10.8 | 1.9 | 1.9 |
| **no acks** (diagnostic only: `ack` made a no-op) | **30.1 %** (30.1, 30.2, 30.0) | 10.4 | 16.8 | 8.3 | 1.3 | 1.3 |
| acks batched, 64 KiB or 100 ms | 31.5 % (31.3, 31.7, 31.5) | 11.2 | 17.4 | 8.9 | 1.5 | 1.5 |

With no acks the path costs exactly what main's does, so the whole delta is ack traffic (app main thread +2.4, and the `ReceiveQueue` threads on both sides that carry IPC). Acknowledgements are now batched (`ptyStream.ts`: a batch goes out at 64 KiB, which a flood reaches at once, or 500 ms after its first byte; only bytes xterm has parsed are ever in a batch). Parsed bytes held back are < 64 KiB whatever the clock does (a test with a clock that never fires), against Rust's 2 MiB window, and a throttled hidden webview cannot sit on credit. 3 alternated runs, `terminal-binary-channel-chat-acks-500-gpu.jsonl` (main 30.1 %, batched 100 ms 31.5 %, batched 500 ms 30.8 %); 500 ms is the one kept. **Final set** of main against this PR (`0b9c5cbf`, code identical to the head), release, GPU, a fresh instance per run, each run started only after the 1-minute load was under 2, start and end load of every run in `terminal-binary-channel-chat-final-gpu.jsonl` (`startLoad`, `endLoad`; all samples under 2):

| run | start load | end load | total CPU | app | web | hostd |
|---|---|---|---|---|---|---|
| main 1 | 0.22 | 0.39 | 31.2 | 10.8 | 17.5 | 2.6 |
| #734 1 | 0.44 | 0.66 | 30.8 | 10.8 | 17.1 | 2.6 |
| main 2 | 0.63 | 0.71 | 30.0 | 10.6 | 16.6 | 2.5 |
| #734 2 | 0.74 | 0.45 | 30.7 | 10.7 | 17.1 | 2.6 |
| main 3 | 0.75 | 0.62 | 30.1 | 10.6 | 16.6 | 2.6 |
| #734 3 | 0.57 | 0.62 | 30.9 | 10.9 | 17.2 | 2.5 |
| **main, mean** | | | **30.4 %** | 10.7 | 16.9 | 2.6 |
| **#734, mean** | | | **30.8 %** | 10.8 | 17.1 | 2.6 |

+0.4 points over main (app +0.1, web +0.2), from +4.6; the run-to-run spread of main alone is 1.2 points (30.0-31.2), so the remainder is inside the noise of this scenario. A flood is not slowed: `terminal-output`, release GPU, 3 alternated runs of the unbatched build against the batched one (start load 0.5-2.0, end 1.4-2.3; `terminal-binary-channel-acks-flood-gpu.jsonl`): 169.9 % (169.5, 171.4, 168.7) against 169.8 % (169.5, 169.3, 170.7), web PSS 307 against 305 MB.

`terminal-output`, release, 3 alternated runs per row, fresh instance each (`terminal-binary-channel-fixed-gpu.jsonl`, `-fixed-llvmpipe.jsonl`), same method as the release section below:

| display | | total CPU | app | web | hostd | web PSS | total PSS |
|---|---|---|---|---|---|---|---|
| GPU | before | **420.2 %** | 102.7 | 114.7 | 202.5 | 2,415 MB | 2,498 MB |
| GPU | after, fixed | **170.2 %** | 58.7 | 85.1 | 26.4 | **307 MB** | 518 MB |
| llvmpipe | before | **439.1 %** | 113.7 | 123.1 | 201.9 | 2,635 MB | 2,717 MB |
| llvmpipe | after, fixed | **197.3 %** | 72.8 | 98.5 | 26.0 | **330 MB** | 572 MB |

The < 200 % target **holds** in release on both displays once the loop is gone (170 % GPU; 197 % llvmpipe, 3 % under). Every run:

| display | run | start load | end load | total CPU |
|---|---|---|---|---|
| GPU | before 1 | 1.67 | 4.30 | 416.9 |
| GPU | after 1 | 1.94 | 2.46 | 170.0 |
| GPU | before 2 | 1.89 | 4.35 | 422.2 |
| GPU | after 2 | 1.92 | 1.32 | 170.4 |
| GPU | before 3 | 1.29 | 4.03 | 421.5 |
| GPU | after 3 | 1.95 | 1.13 | 170.1 |
| llvmpipe | before 1 | 1.28 | 3.78 | 440.0 |
| llvmpipe | after 1 | 1.87 | 3.35 | 197.2 |
| llvmpipe | before 2 | 1.96 | 4.86 | 435.2 |
| llvmpipe | after 2 | 1.95 | 2.51 | 197.6 |
| llvmpipe | before 3 | 1.99 | 5.84 | 442.0 |
| llvmpipe | after 3 | 1.89 | 2.56 | 197.2 |

Start loads are all under 2; end loads contain the app's own cores (as in the release section). The tables from here down are the numbers **with the loop**; they stay as the record of what was measured, and their "target not met" conclusions no longer apply.

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
