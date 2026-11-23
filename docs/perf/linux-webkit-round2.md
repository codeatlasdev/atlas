# Atlas Desktop on Linux, round 2: where the RAM goes, and what is left of the CPU

Roadmap [#289](https://github.com/codeatlasdev/atlas/issues/289); round 1 is `docs/perf/linux-webkit-study.md`. Everything below was measured on one machine on 2026-11-23, against `main` at 4872b85d (debug build, bundled frontend, the same private headless sway on the real GPU, `renderD128`, as in round 1; no window was shown on the user's screen, `hyprctl clients` listed the same windows before and after). Ryzen 7 7735HS, 5.5 GB RAM **shared with other agents' builds**, WebKitGTK 2.54.1.

## Answer in six lines

1. **The largest memory item is not WebKit, it is the language servers**: a TypeScript server for a real project (`apps/desktop` of this repo) is **1.1-1.3 GB** (tsserver 1.09 GB, its syntax server 170 MB, `typingsInstaller` 57 MB, the language server 32 MB), one per (server, worktree root), and each one stayed alive 5 minutes after its last file closed. A 40-minute browsing soak over 3 small projects and 5 worktrees held **~1.0 GB of `node`** the whole time. Fix: `fix/lsp-idle-cap` (only the most recently idled server waits out its grace).
2. **The only monotonic grower is hostd**: 21 → 69 MB in 40 minutes of opening, printing in and closing terminals, **0.23 MB per closed terminal**. Cause (verified by a red test, then by the same churn on the fixed build): the desktop's single, app-lifetime connection kept an `Arc<Session>` for every `pty_attach` until it closed, so every closed terminal kept its 2 MiB ring and buffers. With the fix, 120 cycles are +6 MB (flat after the first 40), not +28 MB. Fix: `perf/webkit-round2` (this PR).
3. **The web process does not leak**: 386 MB mean over minutes 10-40 (342-428), JS heap 40-52 MB flat, DOM 1,450-1,493 nodes flat, 0 canvases left. It is 172 MB at start and 388 MB two minutes into use (lazily loaded code: `FunctionCodeBlock` +16 MB of JIT'd code blocks, 8 → 38 MB of JS heap), then flat. The UI process grows 79 → 106 MB in the first 10 minutes and stays there.
4. glibc arenas are **not** the cause of hostd's growth: over 120 cycles `MALLOC_ARENA_MAX=2` grew +26.5 MB, `MALLOC_MMAP_THRESHOLD_=65536` +27.3, jemalloc (LD_PRELOAD) +34.6, against +28.1 for the default. The data was live, not fragmented.
5. The stack-running "warm-up" is not a warm-up: it is a steady **2.5-3 %** (4.4 % over the first 120 s including the first 15 s of JIT), and its cause is the 3-second chart sample: the JSC sampling profiler puts 90 % of the page's JS time under Svelte's `flush` → zrender (ECharts canvas) `update`; there is no timer or IPC call in that state (0 `invoke`s, 12 `setTimeout(0)` in 30 s).
6. Chat stream 22 % and terminal flood 195.6 % (debug build, load ≤ 0.8 at start): at round 1's level. Not reduced here (see "Not done").

## Method

* **Metric.** `PSS + SwapPss` per process from `/proc/<pid>/smaps_rollup` (and `Anonymous + SwapPss` where file-backed pages matter). On this machine other agents' `rustc` jobs (1 GB each) kept 3-4 GB in swap and the kernel drops *file-backed* pages of an idle process (shared libraries) from RSS without any swap accounting: a bare PSS of a freshly started Atlas read 15 MB for the web process (109 MB swapped) in one run and 205 MB in round 1. Every table below therefore uses PSS + SwapPss, and growth is judged on anonymous memory.
* **Per-mapping breakdown**: `smaps.py` (category by mapping name: `[anon:WKFastMalloc]`, `[anon:JSGigacage]`, JIT, libs, fonts, `[heap]`, thread arenas…). **JS heap**: `WEBKIT_INSPECTOR_HTTP_SERVER=127.0.0.1:<port>` on the private instance works: the Web Inspector protocol over a WebSocket (`Runtime.evaluate`, `Heap.gc`, `Heap.snapshot`, `Memory.startTracking`, `ScriptProfiler`). Heap snapshots (JSC JSON format) were diffed per constructor. **Page state**: `window.__atlas` through `atlas desktop debug eval` (DOM nodes, canvases, tab, pty, editor counts).
* **Soak** (`soak.ts`, 40 min, 63 cycles, every 30 s a sample, heap snapshot every 10 min): per cycle, for each of 3 projects (5 worktrees in total): select the project, step through its worktrees, open 4 editor tabs and close them, open a terminal printing 30,000 lines and close it, (first project) start and stop a chat stream from the fake omp; then dock toggles, settings open/close and the crew view.
* Not available: `perf` (round 1), jemalloc heap profiling (the Arch build has no profiler), `/proc/<pid>/mem` of hostd (ptrace_scope 1).

## Memory per process: welcome screen, then in use

Welcome screen, fresh instance (PSS MB; the web process under swap pressure, so the `lib` rows are a lower bound):

| process | PSS + swap | what it is made of |
|---|---|---|
| web | 125 (PSS 15 + 109 swapped in that run) | `WKFastMalloc` (bmalloc/libpas) 25 MB private, `JSGigacage` 4, JIT code 1, shared libs 25 (`libwebkit2gtk` 12, `libjavascriptcoregtk` 8, `libgallium` 2); 51 threads |
| UI (atlas-desktop) | 56 | executable 11 (debug build), shared libs 11 (`libwebkit2gtk`, `libgtk-3`, `libLLVM`, `libgallium`), `[heap]` 4, anonymous 4; 43 threads |
| network | 11 | libs 8 (`libwebkit2gtk` 3.3, `libsqlite3`, `libcrypto`), anonymous 1.3 |
| hostd | 10 | executable 7, anonymous 1; 22 threads |

JS heap at welcome: 8.3 MB in 66,000 objects (strings 2.7 MB in 14,000, `UnlinkedFunctionCodeBlock` 1.0 MB, `FunctionExecutable` 0.7 MB, `FunctionCodeBlock` 0.6 MB, `Structure` 0.5 MB), 131 loaded modules, 316 DOM nodes, `Memory.trackingUpdate`: javascript 15 MB, other 24-38 MB. The page itself is small: the rest of the web process is WebKit.

The 40-minute soak (PSS + SwapPss, MB; `soak.ts`, one instance, other agents' builds running on the machine):

| minute | UI | web | network | hostd | language servers (node) | DOM nodes | `ws.ptys` |
|---|---|---|---|---|---|---|---|
| 0 | 79 | 172 | 10 | 20 | 0 | 332 | 0 |
| 2 | 93 | 388 | 11 | 26 | 1,057 | 1,493 | 13 |
| 10 | 91 | 417 | 11 | 38 | 1,011 | 1,450 | 64 |
| 20 | 102 | 377 | 10 | 42 | 1,003 | 1,478 | 126 |
| 30 | 103 | 391 | 10 | 52 | 1,021 | 1,493 | 188 |
| 40 | 109 | 381 | 10 | **69** | 1,014 | 1,450 | 252 |

JS heap (sum of all node sizes of a `Heap.snapshot` after `Heap.gc`): 49.0 / 40.0 / 52.1 / 51.0 MB at minutes 10 / 20 / 30 / 40: flat. Heap diff minute 10 → 40: `FunctionCodeBlock` +2.3 MB, `Object` +0.6 MB, everything else < 0.15 MB. The browser-side stores are bounded.

### hostd, isolated (120 × open a terminal, print 30,000 lines, close it; anonymous + swap, MB, hostd only)

| variant | start | after 40 | after 80 | after 120 |
|---|---|---|---|---|
| default (before the fix) | 6.8 | 18.6 | 26.7 | **34.9** |
| `MALLOC_ARENA_MAX=2` | 6.4 | 16.4 | 24.7 | 32.9 |
| `MALLOC_MMAP_THRESHOLD_=65536` | 6.6 | 17.6 | 25.9 | 33.9 |
| `LD_PRELOAD=libjemalloc.so.2` | 10.9 | 24.7 | 39.0 | 45.5 |
| terminal prints nothing | 6.6 | 9.6 | 9.9 | 9.9 |
| created and killed through the host API, no view attached | 6.7 | 11.1 | 11.4 | 11.9 |
| **fix: the connection keeps ids** | 6.9 | 11.7 | 12.7 | **12.6** |

The variants that only change the allocator do not move the line, so the memory was live; a terminal that prints nothing or has no view attached does not grow; one that printed and has a view does, 0.2 MB each (output of 169 KB). That pointed at what the attach path holds, found in `server.rs` `serve` (`let mut attached: Vec<Arc<Session>>`, pushed on every `pty_attach`, released when the connection ends). A wire test with the real `serve_with` (`a_closed_terminal_is_freed_while_the_connection_that_attached_it_stays_open`) kills a terminal that printed 300,000 bytes while a second connection that attached it stays open and waits for the session to be dropped (a test-only recorder in `Session`'s `Drop`): red before ("the connection still holds the killed terminal"), green after, with the 82 other `server::` and `pty::` tests.

### Language servers (`lsp_exp.py`: `typescript-language-server` started as `atlas-lsp` does, `apps/desktop`, one file open, 30 s)

| initializationOptions | processes | PSS + swap |
|---|---|---|
| Atlas today | language server 32, syntax server 174, tsserver 1,085, typingsInstaller 57 | **1,348 MB** |
| `tsserver.useSyntaxServer: "never"` | 37, 1,091, 60 | 1,189 MB |
| + top-level `disableAutomaticTypingAcquisition: true` | 41, 1,094 | 1,136 MB (-212 MB, -16 %) |

Not shipped: both are a behaviour change (no fast first answer while the project loads; no `@types` fetched for untyped JavaScript packages) that needs a user-visible decision, and the cap above removes the multiplication, which is the larger factor. `maxTsServerMemory: 512` made the server die on this project. Note: an option under `tsserver` (`extraArgs`) is silently ignored by `typescript-language-server` 6.0.2; `disableAutomaticTypingAcquisition` and `maxTsServerMemory` are top-level.

## Ranked by MB

| # | What | MB | Status |
|---|---|---|---|
| 1 | one TypeScript server per worktree root, kept 5 min after the last file closes | 1,100-1,300 per project; ~1,000 held through the soak | **fixed** (`fix/lsp-idle-cap`): one idle server at most |
| 2 | web process: WebKit floor ~117 + page ~250 after the first minutes (JIT'd code of CodeMirror/xterm/ECharts, `WKFastMalloc` 25 MB private at welcome) | 380 | not reduced: flat, no leak. Lazy loading of startup code is the lever; the welcome page loads 131 modules |
| 3 | per-language-server helper processes: syntax server, `typingsInstaller` | 230 per TypeScript instance | measured (above), needs a decision |
| 4 | UI process (GTK, tao/wry, tokio, the debug executable 11 MB) | 106 | plateau after 10 min, not examined further |
| 5 | hostd growth per closed terminal | +0.23 each, +48 MB in 40 min | **fixed** (this PR) |
| 6 | `ws.ptys` / `ws.titles` / `ws.exited` never pruned on `closeTab` | 252 entries after 40 min, ~100 bytes each | not fixed: not measurable in MB, listed so it is not rediscovered |

## CPU leftovers on this build (GPU display, % of one core over 60-120 s; load at start in brackets)

| state | total | split | round 1 | note |
|---|---|---|---|---|
| `stack-running`, first 120 s after Start | 4.4 (1.84) | web 2.6, hostd 0.5, `atlas` child 0.7, app 0.4 | 4.6-4.8 | steady state 2.5-3.0 after the first 15 s (33 → 12 → 5 %: JIT of the panel) |
| `chat-stream`, 60 s | 22.0 (0.73) | web 10.7, app 9.2, hostd 1.8 | 18-20 | same level (the number moves with the machine); app main thread 5-8 %, web main thread 5-8 % |
| `terminal-output`, 60 s | 195.6 (0.83) | web 105, app 62.8, hostd 27.8 | ~170 (release build) | debug build; web ≈ 1 core = xterm.js parsing |

**Stack-running cause**: with the stack running the Services panel shows five ECharts canvases fed one sample per 3 s (`metrics.ts` `STEP_MS`); `chartMotion.ts` (#731) already turned animation off, what remains is one redraw of five canvases per sample. `ScriptProfiler` over 30 s: 159 of 177 samples under `flush` (Svelte scheduler), 147 under zrender's `update`; `getPropertyValue`, `fillText`, `Date` at 3-4 samples each. No `invoke`, no `setTimeout` above 0 ms, 0 running CSS animations. A slower chart cadence would remove most of it; that changes what the user sees every 3 seconds, so it is a decision, not a fix made here.

## Not done (and why)

* **Bench memory scenario / gate in `scripts/linux-idle-bench.ts`**: not added. The numbers above need a quiet machine (a PSS gate on this one would read the other agents' swap), and the soak scripts are in `~/.cache/WebkitRam2/`, not in the repo. Hypothesis: a gate on `Anonymous + SwapPss` of hostd after N open/print/close cycles (the new Rust test already holds the cause).
* **terminal-output** (xterm parse ≈ 1 core): I did not look at throttling xterm's render loop while the window rests, nor at per-frame work left; no cause was established.
* **Chat-stream**: the 22 % is split evenly between the web process and the app's main thread; not profiled further.
* Heap snapshot / `smaps` of the **UI process** beyond the table above; macOS and Windows (these fixes are shared Rust, but only Linux was measured).
* The 40-minute soak ran on the build before the fixes; the fixed behaviour was measured by the isolated churn only (hostd +6 MB per 120 cycles), not by a second soak.
