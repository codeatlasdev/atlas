# Native terminal on Linux: what runs when nothing changes

[#774](https://github.com/codeatlasdev/atlas/pull/774) measured an idle native terminal at 1.6-1.7 % of a core against 0.3-0.7 % for xterm.js (the design target is 0.5 %). This is what ran, what was removed and what is left. Same machine, release build and private headless sway as [terminal-linux-gpu.md](terminal-linux-gpu.md); every window started at 1-minute load under 2.

## What ran

Method: per-thread CPU and context switches over the idle window (`scripts/linux-idle-bench.ts`), `WAYLAND_DEBUG=1` to count the toplevel's commits, one change at a time against the live app.

| source | evidence | what it cost |
|---|---|---|
| The focused input sink: a 1 px `<textarea>` with a blinking caret (opacity 0 does not stop the caret) | the toplevel `wl_surface` committed 16 times in 10 s with the terminal idle and 1 time with the sink blurred or sized 0x0; hiding the page's loading strip or pausing its animations changed nothing | WebKit repainted the page and the compositor blended and committed the whole window twice a second: the web process (0.6 %) and the app thread (0.5 %) |
| The cursor blink of the render thread | `render_loop` woke every 530 ms for as long as the window was focused and drew a whole frame each time | one wgpu frame per half cycle, forever |
| The 1 s tick of `term/linux/wl.rs` (scale and toplevel check) | 1 wakeup a second, a few microseconds | not measurable (kept: it is the only way to notice a scale change) |
| Frame callbacks, damage | none: the terminal commits only when it has drawn, and an idle terminal draws nothing but the blink | - |

## What changed

- **The sink has no area** (`width`/`height` 0 in `GpuTerminal.svelte`): there is no caret to blink, so an idle page stops repainting. xterm.js's own helper textarea is the same.
- **The blink rests** (`term/blink.rs`, pure): it runs for 10 s after the last output or input (the GTK/VTE rule, `gtk-cursor-blink-timeout`), then the cursor stays visible and the thread sleeps until woken. A hidden terminal never wakes for it. Output, input or a resize start it again. The loop also stopped waking for timers that change nothing (a blink tick while resting, a watchdog check with no frame owed).

## Numbers (3 windows of 30 s, load before 0.43 / 1.07 / 0.76)

| idle terminal | total CPU | app | web process | hostd |
|---|---|---|---|---|
| native, #774 (clean runs) | 1.6 / 1.6 / 1.6 % | 0.7 | 0.6 | 0.3 |
| native, this change | 1.1 / 1.2 / 1.2 % | 0.4 | 0.4-0.5 | 0.3 |
| xterm.js (clean runs) | 0.6 / 0.7 / 0.6 % | 0.1 | 0.2 | 0.3-0.4 |

Wakeups per second: app 83 -> 64, web process 47 -> 30. Raw windows: [terminal-linux-idle-fixed.jsonl](terminal-linux-idle-fixed.jsonl).

**The 0.5 % target was not met by that change** (1.1-1.2 %); the next section found what was left and met it.

## Every wakeup of an idle terminal, by thread ([#289](https://github.com/codeatlasdev/atlas/issues/289))

Method: `terminal-linux-idle-tools/prof.py` reads `/proc/<pid>/task/*/schedstat` (column 3 = times the thread ran = wakeups, column 1 = ns on CPU) and `stat` for the app, the web process, the network process and hostd over a 30 s window, 25 s after one idle bash terminal is open; release build (`debug-assertions` on `atlas-desktop` for the debug bridge), private headless sway on the Radeon 680M, fresh instance and HOME per window, a window starts only when the 1-minute load is under 2 (`run2.sh`). "Base" is `7d5aba71` (main).

| thread (base, native terminal idle) | wakeups/s | what woke it | now |
|---|---|---|---|
| app `VBlankMonitor` | 39 | WebKit's display link, armed by the focused input sink's caret animator | 0 |
| web process main + compositor threads, `ReceiveQueue` of both | 30 + 10 | the same animator repainting, both processes' IPC | 7 + 1.5 |
| app main thread | 7 + 3 | the repaint, plus the 1 s tick of `wl.rs` | 2 |
| `atlas` (hostd) `tokio-rt-worker` | 10 | the pty reader's 100 ms `poll` (a `std::thread` started from a tokio worker inherits its name), per open terminal | 0 (`pty-<id>`) |
| hostd CPU | 0.65-0.74 % | `Registry::track` every 2 s reading `/proc/<pid>/stat` of all 433 processes of the machine (a scan is ~16 ms here) | 0.14 % |
| `atlas-term-render` | 0 | nothing: the render loop has no timer when idle, no frame callbacks, no damage | 0 |

Eliminations (each against the live app, one change at a time): pausing the page's running animations (the only one is the loading strip) and removing the strip changed nothing; blurring the sink dropped the app to 17 wakeups/s, so the cause is a **focused collapsed caret**, not the page. That also explains why xterm.js, once its helper textarea has focus, costs 1.4-1.5 % (96-99 wakeups/s, 4 windows; its unfocused run, 0.5 %, is the number the earlier tables called xterm.js). A zero-area sink, `opacity: 0`, `caret-color: transparent`, `contain: strict`, `font-size: 0`, `content-visibility: hidden`, `transform: scale(0)`, `-webkit-user-select: none` and `gtk-cursor-blink=false` in `settings.ini` all left the 105-114 wakeups/s; a `tabindex` div (no caret) and a textarea with a **range selection** gave 27-31; `contenteditable` gave 113.

What changed:

- **The sink rests on a selected placeholder** (`terminal/sink.ts`: U+200B, selected; typed text replaces it and is read back without it; `restSink` runs on focus, on every input and after every key release, writes nothing when already at rest). A programmatic selection does not own PRIMARY (a `wl-copy --primary` sentinel survived `select()` and typing, so middle-click paste is intact). Checked live with fcitx5 pinyin (preedit, commit) and without an input method (typing, arrows, Backspace, UTF-8).
- **hostd's pty reader sleeps** until the program prints or `freeze` wakes it: `pty/stop.rs` gives the stop flag a socket pair the reader `poll`s next to the master, with no timeout. `freeze` returns at once instead of within one 100 ms tick; `thaw` withdraws a request and drains its byte. The thread is named `pty-<id>`.
- **Session tracking reads the processes below its roots** (`procs::subtree`: the kernel's per-thread `children` lists on Linux, the table scan elsewhere and on a kernel without them), not every process of the machine.
- **The scale and toplevel check has no timer** (`wl.rs`): the display fd is watched on the GTK main loop, which GDK's source wakes for the same traffic, and the check runs in an idle callback after GDK has dispatched (it would race GDK for the events otherwise). Checked live: `swaymsg output HEADLESS-1 scale 2` and `1.5` re-render the terminal at the new scale (crisp glyphs, the page's logical size) with no tick.

### Numbers (release, 3 runs each, native and xterm.js alternated, every window started at load under 2)

| idle terminal | total CPU | wakeups/s | load before -> after |
|---|---|---|---|
| native, base | 2.04 / 2.10 / 2.10 % | 107 | 1.43 / 0.99 / 1.03 (all < 2) |
| native, this change | 0.48 / 0.46 / 0.49 % | 18 | 1.41 / 0.50 / 1.76 |
| xterm.js, this change (unfocused page, as the earlier tables) | 0.56 / 0.53 / 0.57 % | 20 | 0.73 / 0.94 / 1.30 |
| xterm.js, base | 1.12 / 1.09 / 1.01 % | 30 | 1.10 / 1.99 / 0.66 |
| xterm.js, helper textarea focused (this change; the sink fix does not apply to it) | 1.54 / 1.52 / 1.47 % | 97 | 0.63 / 2.31 / 1.07 |

**The 0.5 % target is met: 0.46-0.49 % against 0.53-0.57 % for xterm.js.** Base measured higher than the 1.1-1.2 % above (2.0-2.1 %): this machine has 433 processes (hostd's scan alone: 0.7 %) and the build has `debug-assertions`; read the comparison within this table only. Windows that started at load 2 or more are marked: the focused-xterm window at 2.31 is in the row above (marked, 1.52 %), one at 13.6 (1.50 %) is `discarded` by `summ.py` and in no cell; its repeat started at 1.07 (1.47 %). Raw windows with the per-thread split: [terminal-linux-idle-wakeups.jsonl](terminal-linux-idle-wakeups.jsonl); tools: [terminal-linux-idle-tools/](terminal-linux-idle-tools/). What is left (18 wakeups/s, none of them traced further): the web process (7), the app's tokio worker (3.7), hostd's sweeper (0.5/s) and tokio workers. [INFERENCE: the web process's remaining wakeups are WebKit's own timers; no stack was taken.] The 16 ms process-table scan was timed with a Python loop over `/proc/*/stat` on this machine.

Not changed: `xterm.js` with its helper textarea focused costs 1.5 % for the same reason (a collapsed caret in a textarea); the fix would be the same selection trick on xterm's own textarea and is not part of this change.

## Tests

Tests (each written first and seen red): `terminal/sink.test.ts` (the sink rests on a ranged selection over the placeholder, typed text is read back without it and the sink rests again, a collapsed caret is repaired, a lost placeholder comes back; it fails with no `sink.ts`), `pty::tests::a_quiet_terminals_reader_sleeps_instead_of_polling` (fails with 14 wakeups in 1.5 s, passes with at most 1; output still arrives) and `freeze_stops_a_blocked_reader_at_once_and_thaw_reads_again`, `procs::tests::a_subtree_is_the_root_and_everything_below_it_and_nothing_else` and the fake-`/proc` pair `a_subtree_follows_the_children_files_of_every_thread...` / `a_kernel_without_children_files_says_so...` (the kernel without the lists must fall back to the scan, never return an empty subtree). The connection watch has no unit test (it needs a GTK main loop and a compositor): it is covered by the live scale 2 / 1.5 check above and the idle windows.

`blink::tests` (the policy: flips each half cycle, rests after 10 s with the cursor visible and no wakeup, comes back once when it rests in the off phase, restarts on output, never wakes when hidden or steady, ignores a tick before the deadline). Incident-named test `the_blink_rests_with_the_cursor_visible_and_the_thread_sleeps` was run against the old behavior (rest disabled) and fails with 113 frames in 60 s against 19 + 0.

## Hidden terminals give their GPU back ([#254](https://github.com/codeatlasdev/atlas/issues/254))

A view the page parked (a background tab or worktree, a dialog over it) kept its swapchain, atlases and buffers. The render thread now drops the `Renderer` once the view has been hidden for 30 s (`term/release.rs`, pure policy; Linux only until the same numbers exist on macOS) and builds a new one on the same view when it is shown again; the view stays off screen until that renderer's first frame, so the rebuild is never seen. The screen and scrollback live in the `Vt`. Geometry, font and ligature requests wait in `State` while released and apply to the new renderer.

Measured with `terminal-linux-gpu-tools/rel.sh` (release builds with and without the change, private headless sway on the Radeon 680M, VRAM from amdgpu `fdinfo` of app + web process + hostd, N terminal tabs of one worktree with the last in front, a sample waits for 1-minute load < 2; raw: [terminal-linux-hidden-release.jsonl](terminal-linux-hidden-release.jsonl)):

| terminals (1 visible) | VRAM without release, 45 s after opening | with release, default 128-256 MB blocks | with release, 8-64 MB blocks (this change) | after showing one hidden terminal (8-64 MB blocks) |
|---|---|---|---|---|
| 1 | 368.1 MB | 369.1 MB | 210.0 MB | 210.0 MB |
| 4 | 409.8 MB | 373.6 MB (-36.2) | 232.1 MB (-177.7) | 244.9 MB (+12.8) |
| 20 | 863.1 MB | 633.9 MB (-229.2) | 270.2 MB (-592.9) | 285.7 MB (+15.5) |

Base (no terminal) is 347.4 MB with the default blocks and 171.5-175.6 MB with the small ones (the lowest column's rows are one run each, load 0.1-0.8 at the 45 s sample; raw: [terminal-linux-hidden-release-blocks.jsonl](terminal-linux-hidden-release-blocks.jsonl)). **Rebuild on show: 6.1 ms (N = 4) and 7.1 ms (N = 20)** with the small blocks, 6.6 ms with the default ones (measured in the app around renderer creation; one sample each).

**Why only about half was freed, and what stays.** An instrumented build (not committed) printed wgpu's allocator report and the process's amdgpu `drm-total-vram` around each `drop(Renderer)` (N = 4 and N = 20; the N = 20 small-block log is [terminal-linux-hidden-release-allocator-mu20.txt](terminal-linux-hidden-release-allocator-mu20.txt)). With the default hints, each drop gave back 8.4 MB of allocator bytes (the glyph atlas 4 MB, the colour atlas 4 MB, the vertex buffers) and 12.35 MB of process VRAM (the extra 3.9 MB is not in the allocator report: the swapchain images); calling `Device::poll` after the drop changed nothing, so wgpu's deferred destruction was **not** the cause (the first suspect, now ruled out). What stayed was the allocator's own memory: 19 released terminals left `total_allocated` at 9 MB (the one live terminal) but `total_reserved` at 448 MB, because wgpu's Vulkan allocator (`gpu-allocator`) takes memory from the driver in blocks of 128-256 MB by default (`MemoryHints::Performance`), returns a block only when it is empty and another one exists (`active_general_blocks > 1` in its `free`), so the atlases were freed inside blocks that were never returned to the driver.

**What this change frees more.** The device is now asked for `MemoryHints::MemoryUsage` (blocks of 8-64 MB): the same drop pattern now returns whole blocks (reserved 192 MB to 77 MB with 9 MB live, N = 20). It also lowers the no-terminal base by 172-176 MB (the first block no longer takes 128 MB when the device is made) and the VRAM per opened terminal (all processes) from about 25 to 19.6 MB (N = 20, before any release). Cost: more `vkAllocateMemory` calls, paid when a terminal opens or is shown, never per frame; the rebuild stays at 6-7 ms. **What still stays** with the small blocks: 78 MB of the app's client at N = 20 over its no-terminal 43 MB, for one live terminal; that is the blocks that still hold a live allocation and the one block the allocator never returns, [HYPOTHESIS: not broken down per block]. I did not go further (smaller or `Manual` block sizes would trade more `vkAllocateMemory` calls for the remainder and were not measured). The flicker sweep (Linux variant) was not run; the first frame after a show is drawn before the view moves over the pane, as for every first frame (`Placement`).
