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

**The 0.5 % target is not met**: an idle native terminal costs 1.1-1.2 %, about 0.5 points above xterm.js. What is left, by thread: the web process's compositor and main threads (0.4-0.5 %: the transparent window the compositor keeps blending, [HYPOTHESIS]), WebKit's `VBlankMonitor` thread in the app (39 wakeups a second at 0.1 %, present with the terminal idle; its trigger was not found), and the app's main thread (0.2-0.4 %). None of them is the terminal's render loop, which now sleeps.

## Tests

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
