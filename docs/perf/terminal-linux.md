# Native terminal on Linux: bench setup, baseline and the key-to-photon method

Phase 0 of [terminal-linux-design.md](../terminal-linux-design.md) (section 6), bench setup [#239](https://github.com/codeatlasdev/atlas/issues/239), Linux phase [#247](https://github.com/codeatlasdev/atlas/issues/247). It commits what the design's spike measured with scratch code, so the later phases (grid write, read coalescing, the subsurface terminal) are measured by the same tools and a parser regression fails CI.

## What runs where

| Number | Where | Why |
|---|---|---|
| `Vt::feed` MB/s per corpus (CPU only) | **CI** (`rust-linux`: `bun scripts/term-parser-gate.ts`) and locally | needs no GPU, no window, no pty |
| Absolute MB/s floors | local, on the reference machine (`--absolute`) | a CI runner is another CPU |
| GPU render time, key-to-present, key-to-photon, flood CPU, memory | **local only**, machine and driver recorded here | a runner has no GPU; llvmpipe numbers say nothing about a real driver |
| pty read sizes, xterm.js in-page rows | local (`terminal-linux-tools/`) | need a pty / a WebKitGTK window |

The maintainer decision behind it: parser throughput is gated in CI relative to a committed baseline; GPU and latency numbers live here with the machine and driver beside them (design section 7.6).

## The parser bench

```sh
cargo bench -p atlas-term --bench vt_parse -- --mib 16 --passes 5 [--only seq,ascii] [--json out.json]
bun scripts/term-parser-gate.ts            # bench + verdict against docs/perf/terminal-linux-baseline.json
bun scripts/term-parser-gate.ts --absolute # also the per-corpus MB/s floors (reference machine, load < 2)
bun scripts/term-parser-gate.ts --report out.json   # verdict on a saved run, no cargo
```

- **Corpora** (`scripts/term-corpus-gen.ts`, deterministic, written to `$CARGO_TARGET_DIR/term-corpus-<mib>` on first use): `seq`, `ascii`, `sgr`, `repaint256`, `truecolor`, and the agent patterns `ink-2026`, `ratatui-diff`, `omp-boxes` through `term-agent-gen.ts`'s own `frameBytes`. Shapes and what each stresses are in the script's header; `scripts/term-corpus-gen.test.ts` pins them.
- **Method**: a fresh 120x40 `Vt`, 64 KiB chunks, one warm-up pass, then `--passes` timed passes; best and median MB/s, plus `/proc/loadavg` before and after each corpus.
- **Calibration**: every run also times a fixed scalar workload (dependent stores into a 1 MiB table) in the same process. The gate compares `MB/s / calibration MB/s`, so a runner that is 2x slower than the baseline machine passes and a parser that got 2x slower does not. It is a ratio of two different workloads: a CPU whose store-buffer or cache proportions differ moves the ratios a little, which the 30 % tolerance absorbs; a regression under 30 % on one corpus is invisible in CI and is caught locally with `--absolute`. A noisy runner scales both numbers; the loads are printed (`noisy` at 2 or more).
- **Baseline**: `terminal-linux-baseline.json` (ratio, best and median MB/s, floor per corpus; written with `--write-baseline`). Raw runs: `terminal-linux-parser.jsonl` (three runs, load 1.6-1.9).

### Parser baseline (Ryzen 7 7735HS, Linux 7.2.15, rustc 1.99.0, release + line tables, 16 MiB corpora, 3 runs)

| corpus | best MB/s (min-max of 3) | design section 5.1 | floor (85 % of best) |
|---|---|---|---|
| seq | 102.8 (102.8-104.0) | 106 | 87 |
| ascii | 90.9 (90.5-90.9) | 88 | 77 |
| sgr | 84.7 (84.3-84.7) | 94 | 71 |
| repaint256 | 139.5 (138.3-139.5) | 150 | 118 |
| truecolor | 139.9 (139.7-140.2) | 143 | 118 |
| ink-2026 | 153.6 (153.6-154.6) | 168 | 130 |
| ratatui-diff | 177.4 (168.1-177.4) | 174 | 150 |
| omp-boxes | 168.5 (167.0-168.5) | 169 | 143 |

Reproduces the design's table within 10 % on every corpus (largest gaps: sgr -10 %, ink-2026 -9 %, with 16 MiB corpora instead of 64 MiB and a quieter machine).

### Floors replace "500 MB/s"

The roadmap's "parse >= 500 MB/s" is not reachable with `alacritty_terminal`'s grid (design 5.1, 5.2). The target is now **per corpus**: the floor column above on the reference machine is the number a change must not fall under today, and phase 1 raises the `seq` and `ascii` rows (design: >= 140 and >= 150 MB/s) in the same baseline file. Floors are raised by editing the baseline (`--write-baseline`) in the PR that earns them, never lowered to make a PR pass.

## Pty read sizes

`terminal-linux-tools/pty_probe.py <corpus> [--opost] [--bufsize N]` cats a corpus into a pty and reports how the master hands it out (`terminal-linux-pty.jsonl`, load 0.6):

| corpus | output processing | reads | mean read | reads of exactly 4,095 B | MB/s |
|---|---|---|---|---|---|
| ascii | off | 2,698 | 6,218 B | 77 % | 254 |
| truecolor | off | 2,400 | 7,021 B | 59 % | 209 |
| seq, 1 MiB read buffer | off | 3,802 | 4,413 B | 92 % | 228 |
| ascii | **on** | 18,512 | 916 B | 0 % | 140 |
| seq | **on** | **194,406** | **97 B** | 0 % | 31 |

A real session has output processing on (`\n` to `\r\n`): a `seq` flood then arrives in ~100-byte pieces, so per-read cost, not parsing, decides throughput. That is phase 2's (#253) target; the probe is its before/after.

## xterm.js in-page rows

`terminal-linux-tools/xterm_inpage.py <corpus> acked|bulk --xterm <apps/desktop/node_modules/@xterm>` runs xterm.js 6.0.0 + addon-webgl in a plain GTK3 WebKitGTK window (no Tauri, no IPC) on the private sway (`up.sh`, `inenv.sh`, `down.sh`: private D-Bus with no servicedirs, headless sway on `renderD128`, nothing on the user's display). `acked` = 64 KiB chunks each awaiting `write`'s callback (what `ptyStream.ts` does), `bulk` = 1 MiB chunks, a callback every fourth. 16 MiB corpora, 100x30, load 0.2-0.6 (`terminal-linux-xterm.jsonl`):

| corpus | acked MB/s | acked web CPU per MB | bulk MB/s | bulk web CPU per MB |
|---|---|---|---|---|
| ascii | 8.2 | 58 ms | 81.0 | 25 ms |
| seq | 7.6 | 95 ms | 20.5 | 65 ms |
| sgr | 7.9 | 66 ms | n/m | n/m |
| truecolor | 8.3 | 45 ms | 67.4 | 25 ms |
| ink-2026 | 8.3 | 42 ms | n/m | n/m |

Reproduces design section 3 (acked about 8 MB/s whatever the content; bulk ASCII 75 there, 81 here; `seq` the outlier: 17 there, 20.5 here). The web process's PSS was 227-444 MB.

## Key-to-photon: the method

The design's 0.8 ms p50 / 2.2 ms p95 is **key to the end of `Renderer::render`** (the wgpu present call). It is not photons, and the xterm.js 10 / 17 ms ends on a `requestAnimationFrame`. Neither is key-to-photon, and no phase may go default-on on them. Three levels, each says what it measures:

1. **Key-to-present (in process, automated, any machine).** Stamp the key at GTK's `key-press-event` (or the page's keydown), stop when the next `Renderer::render` returns with that key's echo in the grid. A lower bound; it is the number a CI-less local gate can assert (phase 3: p95 <= 4 ms). Drive keys with `wtype` on the private sway's seat.
2. **Key-to-presented (compositor timestamp, automated, needs a compositor that reports it).** Bind `wp_presentation` (`wp_presentation_feedback` on the terminal's `wl_surface` commit). Its `presented` event carries the time the frame reached the display (`tv_sec`/`tv_nsec` on the clock from the `clock_id` event; the `hw_clock`, `hw_completion` and `vsync` flags say how much of it is hardware). Start the clock at the key event's own timestamp (`wl_keyboard.key`'s `time`) rather than at our handler, so our own scheduling is inside the number. This is key to **scanout start**; the remaining latency is the panel's (a fixed property of the monitor). Checked on this machine: the private sway 1.12 (headless) advertises `wp_presentation` v2 in its global list (`WAYLAND_DEBUG=1`). A headless output has no real vblank, so its timestamps would check the plumbing, not the latency [INFERENCE: not run]; **real numbers need a real output**, and Hyprland's `wp_presentation` was not probed (no client may map on the user's display).
3. **Photons (external, manual, the ground truth).** A high-speed camera (240 fps or more) filming the screen and a visible key press (or a USB key-event logger with an LED), or a photodiode on the screen against the key switch: count frames from the key to the first changed pixel row. Done once per phase 3 default-on decision on the maintainer's machine and written here with the monitor model and refresh rate; no agent run can produce it.

What each phase records: level 1 in every run (cheap, comparable across commits), level 2 on the maintainer's real session once phase 3 lands (the `Presented` flags in the output so a run with `vsync` unset is discarded), level 3 once before default-on. A level-1 number is never quoted as "latency" without its level.

## Files

`docs/perf/terminal-linux-baseline.json`, `terminal-linux-parser.jsonl`, `terminal-linux-pty.jsonl`, `terminal-linux-xterm.jsonl`, `terminal-linux-tools/` (`up.sh`, `inenv.sh`, `down.sh`, `pty_probe.py`, `xterm_inpage.py`); `crates/atlas-term/benches/vt_parse.rs`; `scripts/term-corpus-gen.ts`, `term-parser-gate.ts`, `term-parser-judge.ts` (+ tests); the gate's CI step in `.github/workflows/ci.yml` and its row in `scripts/checks/map.ts`.
