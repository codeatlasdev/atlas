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
