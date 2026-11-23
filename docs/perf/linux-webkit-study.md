# What WebKitGTK spends on Linux, and why

Roadmap [#289](https://github.com/codeatlasdev/atlas/issues/289) (Atlas Desktop on Linux must be light, stable and fast). Question: why does Atlas Desktop use so much CPU and RAM on Linux, and what makes it light? Everything below was measured on one machine, in one session (2026-11-22), against `main` at c843488c (`1.5.0-alpha.27` build, debug profile, bundled frontend). Nothing was shown on the user's screen (a private headless sway and D-Bus per instance; `hyprctl clients` listed only the user's own windows before and after). Raw data: `docs/perf/linux-idle-*.jsonl`, `docs/perf/linux-webkit-floor.jsonl`; tools: `docs/perf/linux-webkit-study-tools/`; the experiment hook: `docs/perf/linux-webkit-knobs.patch`.

## Answer in five lines

1. **An idle Atlas is already light**: welcome screen 0.2 % of a core, a project open with the SCM dock 0.5 %, window unfocused 1.1 % (GPU path). WebKitGTK's own floor (a static page in a bare GTK3 window) is 0.0 % CPU and 246 MB PSS; Atlas adds ~180 MB (web process +88, UI process +46, hostd 44).
2. **What makes it heavy is never "idle", it is anything that animates or streams.** One 12-px infinite CSS animation in a bare WebKitGTK window costs **13 % of a core on the GPU path and 44 % on llvmpipe** (the CI path); it keeps the whole web process → UI process → compositor pipeline at 60 fps. A workspace whose stack is running (6 running CSS animations) costs **27.8 % / 58.2 %**; a streaming chat 36.5 % / 50.7 %; scrolling a list 31.6 % / 80 %; a flooded terminal 383 % / 425 %.
3. **hostd burns 2.9 % of a core for every open terminal, doing nothing** (`procs::table()` re-reads `/proc/stat` for every process of the machine every 2 s). Verified by syscall counters, reproduced in isolation, and **fixed to 0.5 % by caching one value** (measured with the patch below; not committed here).
4. **A window on another workspace is not "hidden" for WebKit**: `document.visibilityState` stays `visible`, rAF stays at ~61/s, JS and IPC work continue (hidden chat stream 34.8 % vs visible 36.5 %). Nothing in the page can stop work for an invisible window except focus-based heuristics.
5. **Of the WebKit knobs, only one moves the needle: `hardware-acceleration-policy = NEVER`** (stack-running 25.2 → 15.5 %, chat stream 32.8 → 18.0 %) and it needs a terminal/scroll/visual check before it can ship. Forcing the "safe" env vars (`WEBKIT_DISABLE_COMPOSITING_MODE`, `WEBKIT_DISABLE_DMABUF_RENDERER`) makes things **worse** (45.4 and 56.2 % on the same scenario) which confirms the decision in `linux_gfx.rs` to set none.

## Method

Machine: Ryzen 7 7735HS, 16 cores, 5.5 GB RAM shared with other agents' builds (load1 is recorded per row; swapped-out PSS is reported separately and a row with swap > 0 under-reports PSS), AMD Radeon 680M (radeonsi/RADV, `/dev/dri/renderD128`), Mesa 26.2.5, WebKitGTK 2.54.1, GTK 3.24.52, sway 1.12, Arch/Omarchy, Linux 7.2.15.

**Two renderers, both private and invisible.** `scripts/linux-headless.sh`, `scripts/desktop-agent.sh` and the CI job run sway with `WLR_BACKENDS=headless WLR_RENDERER=pixman`. That is what **CI uses** ("Desktop opens (Linux)": a GitHub runner has no GPU, so everything is llvmpipe) and also what `bun run desktop:agent` gives by default. I added a second display with `WLR_RENDERER=gles2 WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128` (`docs/perf/linux-webkit-study-tools/up.sh`). Verified with `webkit://gpu` loaded in a WebKitGTK window on each display:

| | sway `pixman` (CI path) | sway `gles2` + renderD128 (real GPU) |
|---|---|---|
| UI process `GL_RENDERER` | llvmpipe (LLVM 23.1.3) | AMD Radeon 680M (radeonsi) |
| Renderer / buffers | DMABuf, supported buffers: **Shared Memory** only | DMABuf, Hardware **and** Shared Memory, GBM platform |
| Render (web) process GL | AMD Radeon 680M, DMA-BUF export *Unavailable* | AMD Radeon 680M, DMA-BUF mmap `Yes`, 2 GPU raster threads |
| The app's web process opens | - | `/dev/dri/renderD128` ×4, maps `libgallium-26.2.5` |

So on the pixman display the web process still rasterises on the GPU but every frame crosses to the UI process as shared memory and is composited by llvmpipe; on a runner with no GPU both are llvmpipe. Numbers for both are given; the GPU path is the one a user has, the llvmpipe path is what CI can regress-test.

**Measurement.** `scripts/linux-idle-bench.ts --scenario S` (new, tested in `scripts/linux-idle-bench.test.ts`) puts a running agent instance in state S, waits 10 s (70 s for the unfocused ones, past the 60 s `blurGraceMs` of `clock.svelte.ts`), then reads `/proc/<pid>/stat` utime+stime of every process of the instance (app `atlas-desktop`, `WebKitWebProcess`, `WebKitNetworkProcess`, hostd and its children; no GPU process exists in 2.54.1 here) at both ends of **30 s**, per thread too (name, CPU, context switches = wakeups), plus RSS/PSS/SwapPss from `smaps_rollup`, then `atlas desktop debug perf --seconds 5` (rAF/s, long tasks) *after* the window (its own rAF loop would force frames) and a page-state probe (`visibilityState`, `hasFocus`, number of running CSS animations from `document.getAnimations()`). 100 = one full core. A window is given a virtual keyboard on its private seat only (`wtype`, otherwise `document.hasFocus()` is false forever on a headless seat).

```bash
# display + instance (study tools; same as bun run desktop:agent but with the renderer you choose)
docs/perf/linux-webkit-study-tools/cfg.sh gles2 gpu-default        # or: pixman pixman-default
bun scripts/linux-idle-bench.ts --sock /tmp/aws/debug.sock --scenario chat-stream --label gpu-default --out run.jsonl
bun scripts/linux-idle-bench.ts --judge run.jsonl gpu-default/chat-stream=25   # exit 1 over 25 %
```

Scenarios (all against a throwaway project: git repo with an `atlas.yaml` of two real `python3 -m http.server` services, ~2000 files, two linked worktrees, an uncommitted change): `welcome` (no project), `workspace` (project selected, SCM dock open: sidebar, worktrees, status polling), `stack-running` (Start clicked, both services listening), `terminal-idle` (interactive bash in an xterm.js tab, focused, cursor blinking), `terminal-output` (`yes … | head -c 20M` in a loop, unthrottled), `chat-idle`/`chat-stream` (the native chat tab fed by `scripts/fixtures/linux-idle/fake-omp.py`, which speaks omp's `rpc-ui` protocol: 30 cumulative `message_update` lines/s plus a tool call), `list-scroll` (the explorer, 40 folders, scrolled by script at 60 fps), `unfocused` (a `foot` window takes the keyboard on the instance's sway), `hidden-workspace` (sway `workspace 2`).

Not available and what replaced it: **`perf` is not installed** (and `kernel.perf_event_paranoid=2`, `ptrace_scope=1` also block attaching gdb to the detached hostd), so there are no `perf record -g` call graphs. Attribution is by thread name and wakeups from `/proc/<pid>/task/*`, syscall counters from `/proc/<pid>/io`, and a Python replica of the suspect loop. Where a cause is named without a stack, the text says how it was established.

## Per-scenario results

CPU is % of one core over 30 s (web = `WebKitWebProcess`). PSS in MB with the split web/app/hostd/network. "running CSS anims" is `document.getAnimations()` in `running` state. Memory is cumulative within a run (scenarios run in the order shown on one instance, so a later row carries what earlier rows left behind; the first two rows are the clean ones). `load1` is the 1-minute load average (other agents' builds): CPU *time* of a process barely depends on it, PSS does through swapping.

### Real GPU (sway gles2 + renderD128): what a user has

| scenario | CPU % total | app | web | hostd | network | PSS MB (web/app/hostd/net) | RSS MB | page rAF/s | long tasks | running CSS anims | swapped MB | load1 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| scenario | CPU % total | app | web | hostd | network | PSS MB (web/app/hostd/net) | RSS MB | page rAF/s | long tasks | running CSS anims | swapped MB | load1 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| welcome | **0.2** | 0.1 | 0.1 | 0 | 0 | 426.6 (205.5/140.6/43.7/36.8) | 599.3 | 62 | 0 | 0 | 0 | 0.93 |
| workspace | **0.5** | 0.2 | 0.2 | 0.1 | 0 | 464.8 (240.9/141.9/45.2/36.8) | 637.8 | 61.9 | 0 | 0 | 0 | 0.59 |
| stack-running | **27.8** | 10.1 | 12.6 | 2.5 | 0 | 568.2 (303.1/143.8/33.6/36.6) | 784.3 | 61.9 | 0 | 6 | 0 | 0.23 |
| terminal-idle | **4** | 0.9 | 0.2 | 2.9 | 0 | 547.3 (314.9/147.7/46.7/36.7) | 724.9 | 61.9 | 0 | 0 | 0 | 0.1 |
| terminal-output | **382.9** | 151.6 | 106 | 125.2 | 0 | 1153.5 (922.3/134.3/68.1/27.6) | 1335 | 61.6 | 0 | 0 | 17.1 | 2.75 |
| chat-idle | **5.1** | 1.4 | 0.8 | 2.9 | 0 | 1367.9 (1134.5/129.6/68.4/27.6) | 1546.2 | 61.7 | 0 | 0 | 16.7 | 1.5 |
| chat-stream | **36.5** | 12.2 | 19.9 | 4.1 | 0 | 1370.7 (1134.1/132.1/68.9/27.6) | 1549.1 | 61.7 | 0 | 0 | 16.7 | 0.94 |
| list-scroll | **31.6** | 11.4 | 20.1 | 0.1 | 0 | 1368.2 (1138.3/133/69.3/27.6) | 1542.8 | 61.5 | 0 | 0 | 16.7 | 0.88 |
| hidden-workspace | **1.5** | 0.9 | 0.3 | 0.3 | 0 | 1275.2 (1045/133.3/69.3/27.6) | 1449.7 | 61.5 | 0 | 0 | 16.5 | 0.5 |
| hidden-chat-stream | **34.8** | 11.9 | 18.3 | 4.3 | 0 | 1096 (854.1/136.1/70.2/27.6) | 1274.3 | 61.6 | 0 | 0 | 16.5 | 0.34 |
| unfocused | **1.1** | 0.7 | 0.1 | 0.3 | 0 | 1082.2 (850.9/133.5/70.3/27.5) | 1257.8 | 61.6 | 0 | 0 | 16.5 | 0.12 |
| unfocused-chat-stream | **37.9** | 13.6 | 19.9 | 4.1 | 0 | 965.2 (724.7/135.3/73/25.9) | 1149.5 | 61.7 | 0 | 0 | 16.4 | 8.95 |

### llvmpipe (sway pixman): what CI and `bun run desktop:agent` use

(Rows after `list-scroll` were measured with load1 4-8 and 400-500 MB swapped out: trust their CPU, not their PSS.)

| scenario | CPU % total | app | web | hostd | network | PSS MB (web/app/hostd/net) | RSS MB | page rAF/s | long tasks | running CSS anims | swapped MB | load1 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| scenario | CPU % total | app | web | hostd | network | PSS MB (web/app/hostd/net) | RSS MB | page rAF/s | long tasks | running CSS anims | swapped MB | load1 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| welcome | **0.2** | 0.1 | 0.1 | 0 | 0 | 467.3 (210.5/185.3/43.6/27.9) | 624.9 | 62 | 0 | 0 | 0 | 0.2 |
| workspace | **0.5** | 0.2 | 0.2 | 0.1 | 0 | 511.7 (251.8/187.1/44.9/27.9) | 669.7 | 62 | 0 | 0 | 0 | 0.25 |
| stack-running | **58.2** | 28.6 | 24.4 | 2.7 | 0 | 608.9 (305.3/191.7/33.6/27.7) | 810.4 | 62.1 | 0 | 4 | 0 | 0.61 |
| terminal-idle | **3.8** | 0.9 | 0.1 | 2.8 | 0 | 604.1 (334.1/194.1/46.7/27.9) | 766.9 | 62.1 | 0 | 0 | 0 | 0.5 |
| terminal-output | **425.1** | 169.9 | 128.6 | 126.5 | 0 | 1208.4 (933.8/177.1/68.5/27.8) | 1375.2 | 62.1 | 0 | 0 | 28 | 4.05 |
| chat-idle | **5.9** | 1.9 | 1.1 | 2.9 | 0 | 1437.9 (1162.7/171.3/68.9/27.6) | 1602.7 | 62.1 | 0 | 0 | 28 | 2.58 |
| chat-stream | **50.7** | 21.1 | 24.9 | 4.4 | 0 | 1282.9 (1059.9/142.5/58.2/14.7) | 1446.7 | 62.1 | 0 | 0 | 146.7 | 1.97 |
| list-scroll | **80** | 37.4 | 42.6 | 0 | 0 | 442.1 (307.3/88/38.1/8.7) | 543.6 | 62.1 | 0 | 0 | 512.4 | 4.28 |
| hidden-workspace | **2.3** | 1.6 | 0.2 | 0.5 | 0 | 284.4 (177.6/74.9/24.2/7.7) | 384.1 | 62.1 | 0 | 0 | 539 | 5.28 |
| hidden-chat-stream | **32.9** | 10.1 | 15.9 | 6.4 | 0 | 292.9 (180.9/70.6/30.1/3.7) | 365.6 | 62.1 | 0 | 0 | 421.9 | 5.94 |
| unfocused | **3.1** | 1.1 | 1.5 | 0.5 | 0 | 166.3 (108.1/45.5/10.4/2.3) | 208.9 | 62 | 0 | 0 | 465.2 | 4.5 |
| unfocused-chat-stream | **72.7** | 30 | 36.6 | 5.6 | 0 | 276.5 (177.1/62/25.9/1.8) | 325.1 | 62 | 0 | 0 | 377 | 8.01 |

(hostd rows in both tables are with the *unpatched* `/proc` scan: see cause 4.)

### The floor: WebKitGTK 2.54.1 alone

`docs/perf/linux-webkit-study-tools/floor.py`: a plain GTK3 window with one `WebKitWebView` (no Tauri, no JS) on the same two displays, 10 s settle + 30 s.

| page | display | CPU % (ui / web) | PSS MB (ui / network / web) | total PSS | RSS |
|---|---|---|---|---|---|
| static text | GPU | 0.0 (0.0 / 0.0) | 94.7 / 33.8 / 117.1 | 245.6 | 397 |
| static text | llvmpipe | 0.1 (0.1 / 0.0) | 141.0 / 32.0 / 128.3 | 301.2 | 437 |
| one 12 px dot, `opacity` keyframes, `infinite` | GPU | **13.1** (8.6 / 4.5) | 109.8 / 31.7 / 115.8 | 257.3 | 415 |
| same | llvmpipe | **44.4** (28.9 / 15.5) | 128.6 / 32.0 / 136.9 | 297.5 | 431 |

A minimal Tauri window is the same two processes plus tao/wry (a few MB); I did not build a separate wry example. Atlas at the welcome screen is 426.6 MB PSS (GPU): web 205.5 (+88 over the floor), UI process 140.6 (+46), hostd 43.7, network 36.8.

## Ranked causes, with measured share

Shares are of the busy state's CPU, GPU path / llvmpipe path.

| # | Cause | State | Cost | Evidence |
|---|---|---|---|---|
| 1 | **Running CSS animations keep the compositing pipeline at 60 fps** (web process commit + UI-process main thread composite per frame), however small the animated element | stack running (6 animations) | 27.8 % / 58.2 % vs 0.5 % idle (≈98 % of the state); floor with ONE dot: 13.1 % / 44.4 % | threads: UI process main thread 8.6 % at 233 wakeups/s, web main 6.9 %, `ThreadedCompositor` 3 %, `ReceiveQueue` 1 %; floor reproduces it with no Atlas code |
| 2 | **Streaming chat: whole-transcript re-evaluation per delta** (omp sends the cumulative `partial` in every `message_update`; hostd → Rust → base64 → `emit` → JSON.parse + Svelte for each ≤16 ms batch) | chat streaming | 36.5 % / 50.7 % (web JS 19.9, UI process 12.2, hostd 4.1) | thread split above; unchanged when the window is on another workspace (34.8 %) |
| 3 | **Terminal flood path**: hostd read/coalesce → Rust `PTY_FLUSH` 16 ms → base64 → `emit_to` → xterm.js | `yes` flood | 383 % / 425 % (app 152, web 106, hostd 125); web PSS 205 → 1,153 MB | worker threads of all three evenly loaded; web `HeapHelper` ×3 ≈ 2,000 wakeups/s each |
| 4 | **hostd re-reads `/proc/stat` once per process of the machine, every 2 s, while any terminal exists** (`procs::table()` → `stat(pid)` → `boot_time()`, called from `Registry::track()` by the 2 s sweeper in `server.rs`) | one idle terminal, nothing printed | **2.9 %** of a core per idle terminal (0.1 % with none) | `/proc/<hostd>/io`: +29,000 reads and +7 MB `rchar` per 10 s = 405 processes × 3.2 KB per scan; a Python replica of the loop costs 49.8 ms/scan = 2.49 % of a core, 7.0 ms = 0.35 % without the repeated read. **With `boot_time()` cached (patch): terminal-idle hostd 2.9 → 0.5 %, stack-running hostd 2.5 → 0.6 % and its child `atlas` 2.4 → 0.6 %, chat-stream hostd 4.1 → 2.1 %** |
| 5 | **Scrolling** a 40-row virtual list by script at 60 fps | scrolling | 31.6 % / 80 % | web main 15 %, UI process 8.9 % |
| 6 | **Memory that does not come back**: the web process went 205 → 1,134 MB after the flood and stayed ≥ 725 MB through the next 10 minutes of scenarios although the tab was closed | after a terminal flood | +0.9 GB | default memory-pressure settings are limit 3,072 MB, poll 30 s, conservative 0.33, strict 0.5, kill off: 1.13 GB is just over the conservative line, so WebKit trims slowly; setting 1 GB did not change the number measured right after (see knobs) |
| 7 | Nothing: idle (welcome, workspace, unfocused, hidden, terminal with a blinking cursor) | idle | 0.2-1.1 % (+ cause 4) | table above |

### Always-running work in the frontend (audit, not measured per item)

* `setInterval`: 14 call sites outside the capture helpers and the clock itself. The app clock (`lib/clock.svelte.ts`) is one visibility- and focus-gated timer (asleep after 60 s blurred), `usePoll` rides it; the remaining raw intervals are in `CrewBoard`, `CrewmateView`, `CrewSettings`, `AutomationsSection`, `GithubAppCard`, `githubCrew` (4 s while crew is open), `panel/metrics`, `autoCleanup`, `preview`, `updates`, `routes/dock`: none was running in the idle states measured (`long tasks` 0 in every row; web process 0.2 % with the project open).
* `requestAnimationFrame` loops: every one found is event-driven or stops on `document.hidden` (`artTimeline`, `CrewmateFace`, `Globe`); none runs in `welcome`/`workspace` (rAF is 62/s there only because the probe itself asks for frames).
* **Infinite CSS animations: 60 `animation: … infinite` declarations in 42 files** under `apps/desktop/src` (`app.css` `.dot.warn` pulse and `native-load`, `StatusRing` spin, `ProjectHead` `alive` ring, `SidebarFooter` `glow`, the Crew art/onboarding scenes, `CrewTyping`, `CrewmateListRow`, `ChatHeader` pill, composer mic …). 54 files carry `prefers-reduced-motion` guards; none is gated on window focus. **Which six were running in `stack-running` was not identified** (the bench now reports the count; naming them with `getAnimations()` is step one of fix 2). In an earlier probe with services that never listened, the running ones were `.dot.warn` `pulse` and two spinner `spin` animations.
* `backdrop-filter`: only `CrewArtHow.svelte` (4 rules, onboarding art); `noBackdropBlur.test.ts` keeps the rest out. `will-change`: 17 sites, not measured.
* IPC floods from Rust: the only high-rate producer is the pty flusher (`host.rs` `PTY_FLUSH`, 16 ms, one event per pty per frame). No other event source ran in idle.
* GTK side (title row, tray): the tray and dock webviews are built lazily (#286); nothing GTK-side showed up in the UI process threads at idle (app 0.1-0.2 %).
* xterm.js: WebGL renderer, cursor blink is xterm's own timer: a focused idle terminal costs web 0.2 % + UI 0.9 % (`terminal-idle` 1.6 % with the hostd fix).

## Window state: unfocused, hidden, other workspace

| state | CPU % (GPU) | `visibilityState` | `hasFocus` | rAF/s |
|---|---|---|---|---|
| focused idle workspace | 0.5 | visible | true | 61.9 |
| unfocused (another window has the keyboard), 70 s settle | 1.1 | **visible** | false | 61.6 |
| window on a sway workspace that is not shown | 1.5 | **visible** | false | 61.5 |
| chat streaming, focused | 36.5 | visible | true | 61.7 |
| chat streaming, window on a hidden workspace | **34.8** | **visible** | false | 61.6 |
| chat streaming, unfocused | 37.9 | visible | false | 61.7 |
| (llvmpipe) chat streaming focused / hidden workspace / unfocused | 50.7 / 32.9 / 72.7 (the last two with load1 6-8) | visible | | 62 |

On Wayland, GTK3/WebKitGTK gets no occlusion signal: `visibilityState` is `visible` for an unfocused window and for one on another workspace, and the headless output keeps delivering frame callbacks, so rAF keeps running. (A real compositor may stop frame callbacks to a hidden surface; the page would still not learn it, and the 34.8 vs 36.5 % shows the JS/IPC half does not depend on it.) Consequence: the clock's focus-gating (60 s grace) is the only lever Atlas has; CSS animations and chat streaming ignore it.

## Knobs (same display, same scenario, one change each)

Each row is a fresh instance on the **GPU display**; `base` = unmodified behaviour built with the same patch. WebKitSettings were applied through a throw-away environment-driven hook (`docs/perf/linux-webkit-knobs.patch`, uses `webkit2gtk` 2.0.2 already in `Cargo.lock`: `WebViewExt::settings`, `SettingsExt::set_hardware_acceleration_policy`, `WebContextExt::set_cache_model`, `WebsiteDataManager::set_memory_pressure_settings`; wry itself only sets `enable-webgl`, `enable-webaudio`, `enable-page-cache` and developer extras). Env vars were checked to exist in `libwebkit2gtk-4.1.so` 2.54.1 (`strings`). Noise: two baseline runs of the same scenario differ by < 1 point of CPU (terminal-output 382.9 / 380.2 %, workspace 0.5 / 0.6 %). The hostd fix is in all rows, so compare rows with each other and with `base-patched-procs`, not with the tables above.

| config | workspace CPU% (web/app/hostd) | stack-running CPU% (web/app/hostd) | chat-stream CPU% (web/app/hostd) | terminal-idle CPU% (web/app/hostd) | terminal-output CPU% | web PSS MB after terminal-output | chat-idle web PSS MB |
|---|---|---|---|---|---|---|---|

Reading it:

| knob | effect | risk / verdict |
|---|---|---|
| `hardware-acceleration-policy = NEVER` | stack-running 25.2 → **15.5 %**, chat-stream 32.8 → **18.0 %** (UI process 10.1 → 3.2 %, web 13.7 → 11.0 %) | **Not shippable: the window paints nothing under it** (see "Acceleration policy" below); was: promising, not shippable yet: list-scroll, terminal-output, the xterm WebGL renderer and visual diffs were not run under it. Software compositing is the *safest* path for NVIDIA/X11 stability but may cost scrolling and flood throughput. Needs the scenario set + a screenshot diff (`flicker-sweep`) first |
| `= ON_DEMAND` | 24.5 / 32.4 % (no change) | animations count as "needed": equal to ALWAYS here |
| `WEBKIT_DISPLAY_REFRESH_THROTTLE_FPS=30` | stack-running 22.9 (−9 %), chat-stream 29.8 (−9 %) | small gain, halves nothing (cost is not only per frame); would visibly cap animation/scroll to 30 fps. Not worth it alone |
| `WEBKIT_DISABLE_COMPOSITING_MODE=1` | stack-running **45.4**, chat-stream 36.5 (+80 % / +11 %) | worse; matches Tauri's own warning and the xnmp report cited in `linux_gfx.rs` |
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | stack-running **56.2**, chat-stream 45.2 (+123 % / +38 %) | worse. Keep unset on working setups (it is the fix for some NVIDIA/X11 boxes: user-opt-in, never default) |
| `WEBKIT_SKIA_ENABLE_CPU_RENDERING=1` | 26.8 / 34.6 (+6 %) | no gain on a GPU box; only relevant for llvmpipe (CPU raster 8 threads vs 2 GPU threads in `webkit://gpu`) |
| `enable-smooth-scrolling = false` | 24.4 / 32.5 | no CPU effect (scroll was programmatic): UX only |
| window `transparent` (what macOS does) | 24.6 / 32.6 | **no measurable cost on Wayland** here: the comment in `main.rs` "the Linux cost is inferred, not measured" is now measured as zero on sway; may differ on a blending compositor/real display |
| `enable-webgl = false` (xterm falls back to its DOM renderer) | terminal-idle 1.6 → 2.3 %, terminal-output 380 → 394 %, web PSS after flood 908 → 984 MB | **worse**: keep WebGL on |
| cache model `DOCUMENT_VIEWER` | terminal-output 377 %, web PSS 921 vs 908 MB, chat-idle 1,072 vs 1,059 | no effect (the app has no HTTP cache to speak of) |
| memory pressure 1 GB / poll 10 s / 0.4 / 0.6 | web PSS 923 vs 908 MB after the flood, 1,080 vs 1,059 later | no effect inside the 3-minute window: the pressure poll did not release the xterm/JSC heap in time; not proven useless over longer runs |

**Not measured** (say so rather than guess): `GDK_BACKEND=x11` (agent mode refuses any `DISPLAY` on purpose, so Atlas cannot run on the private Xwayland; only the floor could, and I did not run it), `enable-media=false`, `enable-page-cache=false`, NVIDIA (no NVIDIA machine), `WEBKIT_DISABLE_MEMORY_PRESSURE_MONITOR`, the 2.54 `WEBKIT_SKIA_*` thread/MSAA knobs, `perf` call graphs, macOS numbers (no macOS number exists in `docs/perf`: its numbers are GPU-power ones in `main.rs`' comment on tauri#15471, not comparable).

## Acceleration policy: why fix 3 is not shipped (follow-up, 2026-11-22)

`hardware-acceleration-policy = NEVER` was applied through an experiment build (`ATLAS_WEBKIT_ACCEL=off`, WebKitSettings set right after the window is built) and the whole scenario matrix was run against the default on both displays, 3 runs per side. **Verdict: not shippable, because under `NEVER` the window paints nothing on this stack, and the CPU savings below are the savings of not painting.**

* **Screenshots.** `grim` of the private sway output is a flat black frame on the GPU display (PNG of 3,848 bytes against 132,000 with the default) and a flat off-white frame on the llvmpipe display (5,769 bytes). The page itself is alive: `document.readyState` is `complete`, `body.innerText` has the UI text, `atlas desktop debug eval` answers, the xterm WebGL canvas exists and `isContextLost()` is false. `atlas desktop debug shot` (WebKit's own snapshot) is also blank (3,131 bytes). Reloading the page after setting the policy did not help. Per-pixel diff default vs `NEVER` (ImageMagick `compare -fuzz 3%`): 70 % of the pixels differ on the GPU display, 7.4 % on llvmpipe, for `workspace`, a terminal with ANSI colours and a terminal tab; two default runs of the same state differ by 0 pixels and the same state on both displays by 0.02 %.
* **The sampled-frame flicker check cannot see this.** Frames of a 15 s list scroll and a 15 s `yes` flood were grabbed with `grim` and judged by `apps/desktop/src/lib/debug/flickerSweep.ts` (`analyze`: blips, blanks, flashing): 0 findings in all 8 captures, including the black ones, because a page that is blank in every frame "has nothing to lose" by design (a blank is only reported as a *departure* from content). The luminance of every frame of the `NEVER` captures is constant (0.0000 on the GPU display, 0.7388 on llvmpipe). `scripts/flicker-sweep.ts` itself is macOS only (ScreenCaptureKit), so it was not run; the grim-based capture here gets 5-18 fps, not 60.
* **xterm.** With the default, xterm's WebGL addon creates a `webgl2` canvas and keeps it (no context loss); that is also what it reports under `NEVER` (the canvas exists and is not lost), so WebGL health says nothing about whether the pixels reach the screen. `enable-webgl` stays on (cost of the DOM renderer: see Knobs).

CPU, % of one core over 30 s, three runs each (default / `NEVER`; raw rows without the per-thread detail: `docs/perf/linux-accel-policy.jsonl`, label `<display>-<default|off>-v3r<run>/<scenario>`), with the 1-minute load at each run's start in brackets. Every row proved its state happened (`live` in the report: services accepting connections, flood passes, `message_update` lines the fake engine wrote, scroller frames); rows with load >= 4 are marked `*` and are not to be quoted; the earlier matrix (`r1`-`r3`) had no such proof and is not used for these scenarios.

| scenario | GPU display default | GPU display NEVER | llvmpipe default | llvmpipe NEVER |
|---|---|---|---|---|
| stack-running | 26.7 (3.95) / 31.3 (1.97) / 26.2 (1.03) | 16.6 (2.71) / 15.9 (0.75) / 16.8 (2.05) | 54.3 (2.9) / 56.8 (0.65) / 60.3 (2.73) | 15.7 (1.28) / 15.7 (0.48) / 18.6 (3.92) |
| terminal-output | 381.6 (3.89) / 377.1 (3.52) / 362.9 (1.73) | 363 (4.38*) / 363.3 (1.4) / 363.3 (2.66) | 412.1 (2.28) / 400 (1.32) / 427.4 (3.89) | 360.1 (1.19) / 362.4 (1.01) / 366.4 (3.8) |
| chat-stream | 34.9 (2.64) / 34.7 (3.24) / 33.1 (2.78) | 17.6 (2.89) / 17.8 (2.48) / 20.9 (4.02*) | 48.1 (3.17) / 47.2 (2.9) / 53.4 (3.56) | 20.2 (2.38) / 19.4 (1.82) / 21.5 (3.33) |
| list-scroll | 44.9 (4.04*) / 32.6 (2.42) / 37.3 (2.17) | 13.5 (2.66) / 11.9 (1.3) / 12.6 (2.5) | 62.1 (1.71) / 121.3 (15.29*) / 68.5 (2.82) | 11.3 (2.01) / 19.1 (3.62) / 13.2 (2.83) |
| hidden-chat-stream | 32.8 (2.68) / 32.7 (1.28) / 33.7 (1.86) | 19.3 (2.78) / 17.1 (0.71) / 23.3 (2.07) | 22.9 (1.89) / 19.2 (2.92) / 23.5 (3.09) | 18.4 (1.69) / 17.3 (3.06) / 20.6 (2.61) |
| unfocused-chat-stream | 35.6 (2.12), 2 of 3 runs did not stream | 21.1 (2.45) / 16.9 (0.38), 1 of 3 did not stream | 42 (1.42) / 53.7 (0.98), 1 of 3 did not stream | 17.4 (1.2), 2 of 3 did not stream |

Many cells are at load 2-4 (other agents were building on the same machine), so they are an indication of the size of the effect, not a measurement to the point. Idle rows (`welcome`, `workspace`, `terminal-idle`, `chat-idle`, `hidden-workspace`, `unfocused`) are 0.2-2.4 % in both modes (`NEVER` is not better or worse there: nothing animates). The ratio of the busy rows (about 2-5x less CPU under `NEVER`) is large, and it is what a window that is not painted costs. What is still unknown: whether `NEVER` blanks the window on a real compositor (this was sway headless with `gles2` and `pixman`) or on X11, and whether setting the policy before the first load (the WebKitGTK constructor property, not reachable through wry/Tauri today) behaves differently. Until a measured run shows pixels on screen with `NEVER`, fix 3 stays off and `linux_gfx.rs` sets no policy. The experiment is kept on the local branch history only; it was 25 lines (`WebViewExt::settings` + `set_hardware_acceleration_policy(Never)` through `with_webview`).

Bench changes this follow-up needed, all in this PR: fixture ports are picked per bench (two benches on one machine started each other's services at 21001/21002), every scenario carries a liveness proof and a run whose state did not happen exits 1 with no result (an `unfocused-chat-stream` row read 1-2 % when the chat never streamed), the bench waits for load < 2 before a scenario (`--max-load`, `--load-timeout`), and the report carries the start load and the ports.

## Gate (CI, "Desktop opens (Linux)")

`scripts/linux-idle-gate.sh` runs the bench against the CI build on llvmpipe and `linux-idle-bench.ts --judge --gate` fails over `GATE_BUDGET`: `welcome` 3 %, `workspace` 4 % of a core over 30 s. Measured here on llvmpipe, three runs of the matrix: `welcome` 0.2-0.3 %, `workspace` 0.2-0.5 %; the gate script itself on this machine (15 s windows, the bench's own bun counted in the instance's tree) read 0.3-0.9 and 0.9-1.2 %. The budgets are 4-10x the worst of those because a 2-vCPU runner with a different Mesa and the sandbox off is slower, and a regression the gate must catch is of the order of the animation floor (13-44 %), not 1 point. `stack-running`, `terminal-idle`, `chat-stream` and `terminal-output` are not gated: they are not idle on main (26-60 %, 1.5 % plus hostd's 2.9 %, 33-53 %, 380-420 %); the PR that removes each cost adds its line to `GATE_BUDGET`. The first CI run of the job is what sets the runner's real numbers: the report (`desktop-linux-idle-gate` artifact, per-thread split) is uploaded for that.

## Memory

* Welcome screen (GPU): **427 MB PSS / 599 MB RSS** = web 205 + app 141 + hostd 44 + network 37; floor 246 MB PSS (WebKitGTK alone). 58 % of Atlas's resting memory (246 of 427 MB) is WebKitGTK's own floor; the rest is the page (+88), the UI process (+46) and hostd (44).
* Project open with a dock: 465 MB. A running stack: 568 MB (+103: the services' pages, spinner layers, hostd child).
* A 20 MB-per-pass terminal flood: web 205 → **1,153 MB** and the process was still 855-1,045 MB three scenarios later with the tab closed, 725 MB at the end of the run (GPU). The soak script (`soak.sh`, 10 min of scripted use) stalled after 2 minutes on a busy machine, so **there is no valid 10-minute growth series**; what exists is the ~15-minute cumulative matrix above, whose web-process PSS is 205 → 1,134 → 725 MB: grown by the flood, partly released, never back to the start.

## Fix list (each item is one PR; none is implemented here)

| # | PR | Expected gain | How to prove |
|---|---|---|---|
| 1 | `fix(core): read /proc/stat once`: cache `boot_time()` in `atlas_core::procs` (`OnceLock`) | hostd −2.4 % of a core per open terminal (2.9 → 0.5 % measured with exactly this change, and `atlas` child 2.4 → 0.6 % while services run) | fail-first test: `table()` over N fake `/proc` entries must read `/proc/stat` once (inject the reader); bench gate: `terminal-idle` hostd ≤ 1 % |
| 2 | `fix(desktop): animations only while something changes`: name the 6 running animations (`getAnimations()`), replace infinite pulses/spinners on steady states (a healthy service dot, finished chips) with static styles, pause the rest under a root class when the window is blurred past `blurGraceMs` | stack-running 27.8 → ~1 % on the GPU, 58 → ~1 % on llvmpipe (cost = per-animation-pipeline, not per-element: the floor dot alone is 13 / 44 %) | bench gate `stack-running` ≤ 3 % (fails today at 25-28 %); unit test that no element in a settled `ServiceCard`/`ProjectHead` render has an `infinite` animation |
| 3 | `perf(desktop): software compositing policy on Linux`: `hardware-acceleration-policy = NEVER` via `with_webview` | stack-running 25 → 15 %, chat 33 → 18 % (GPU path) | first extend the matrix to `list-scroll`, `terminal-output`, a flicker sweep and a screenshot diff under the policy; ship only if scroll p95 frame time and flood throughput hold; opt-out env var; NVIDIA/X11 reports as the watch item |
| 4 | `perf(desktop): chat streaming applies one delta per frame`: `rpc.apply` keeps only the newest `message_update` per `messageId` per batch, avoid cumulative re-parse; ask omp for deltas | chat-stream 36.5 → target ≤ 15 % | bench gate `chat-stream` ≤ 15 % with `fake-omp.py`; test: N cumulative updates in one batch produce one item update |
| 5 | `perf(desktop): terminal output on a binary channel`: Tauri `Channel`/raw bytes instead of base64 + `emit_to` string per frame; hostd coalesce already 8 ms | `terminal-output` 383 % → target < 200 %; fewer web `HeapHelper` wakeups | bench gate `terminal-output`; the existing `pty_batch` tests keep ordering |
| 6 | `fix(desktop): release terminal memory on close`: dispose scrollback and the WebGL context when a tab closes, cap `max_scroll_history_lines` under flood | web PSS after the flood back to ≤ 400 MB | `soak.sh`-style bench: PSS after open/flood/close cycles flat within 10 % over 10 minutes (needs a stable 10-minute run) |
| 7 | `test(bench): gate the idle scenarios`: wire `linux-idle-bench.ts --judge` into the CI desktop job for `workspace` ≤ 2 %, `terminal-idle` ≤ 2 % and, once 1-2 land, `stack-running` ≤ 3 %, with llvmpipe numbers (they are the ones CI can reproduce; GPU numbers stay manual) | keeps 1-2 from regressing | the job itself (owned by whoever owns `linux-startup-bench.ts`) |
| 8 | `docs: record window visibility on Wayland`: WebKitGTK never reports hidden for an unfocused window or another workspace; any "pause when hidden" must key on focus + `blurGraceMs` | none by itself | this report's table |

macOS equivalents: fixes 1, 4, 5 (Rust/JS shared) and 2 (shared CSS) apply unchanged; fix 3 and the knobs are WebKitGTK-only (WKWebView has no such policy; its occlusion handling is documented in `docs/desktop.md`).

## Trivially safe items done here

None in product code. The bench (`scripts/linux-idle-bench.ts` + test + `scripts/fixtures/linux-idle/fake-omp.py`) is new; `docs/perf/linux-webkit-knobs.patch` is an experiment, not applied.

## Limits and risks of this study

* One machine, debug build (`opt-level 0` for the Rust side inflates app/hostd CPU; the WebKit side is the system library and is unaffected). A release build must re-run the matrix before any budget is set from it.
* Load from other agents' builds varied (load1 0.1-9); CPU time per process is the metric for that reason, and rows with load > 4 are flagged. The llvmpipe `unfocused-chat-stream` (72.7 %) and `list-scroll` (80 %) rows were measured at load 4-8 and should be re-run quiet before being quoted.
* The six animations of `stack-running` are counted, not named. The hostd cause is established by syscall counters and a replica, and its fix by a measured patch, but not by a stack trace (no `perf`).
* Focus was provided by a virtual keyboard on the private seat; real keyboard/IME behaviour (xterm focus handling) was not exercised.
