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

## Where the grid write goes (phase 1)

The design (section 5.2) read a stack sample as "35 % of `seq`/ASCII time is dropping `Option<Arc<CellExtra>>`" and proposed removing that drop. It was checked with counters and with eliminations before anything was written, and **the drop is not the cost**. Method and tools: `terminal-linux-tools/alacritty-grid-counters.patch` (counters in a scratch copy of alacritty_terminal 0.26.0), `grid_counters_main.rs`, `ptrace_sample.py` (a ptrace sampler: attach, read RIP, detach, 2,000-3,000 samples at ~1 ms, symbolized with `llvm-symbolizer --inlining`; `perf` is not installed). 16 MiB corpora, 64 KiB chunks, best of 4-6, load 1-2.7.

- **Counter.** Every cell write and every cell reset dropped `None`: `seq` 12,802,040 writes and 12,731,767 resets, **0** over a `Some`; `ascii` 16,434,912 and 15,471,168, 0; `sgr` 8,952,664 and 8,175,115, 0. The generic drop glue runs for each cell and frees nothing.
- **Removing it gains 4-6 %.** A scratch alacritty that writes `extra` only when either side is `Some` and resets cells field by field: seq 103 to 109, ascii 91 to 95, sgr 85 to 87 MB/s (with a 364-line history, 128 to 135 on ascii).
- **What the sampler attributes to the glue is a memory stall.** Skipping the cell store entirely changed nothing (ascii 94 to 94); `Term::input` returning right after the width lookup runs ASCII at 300 MB/s; with `write_at_cursor` skipped but the scroll kept (so no cell is ever dirty and `Row::reset` has nothing to do) 218. The cost is `Row::reset` of the row a scroll recycles: **37 % of `ascii` and 10-13 % of `seq`**. A full scrollback is a ring (10,000 lines x 120 cells x 24 bytes = 28.8 MB), the recycled row was last touched 10,000 lines ago and is in RAM, and the reset reads each cell's old `extra` (the "drop" the sampler saw: the load that misses) and writes 24 bytes. With the history cut to 364 lines (1 MiB, in cache) ascii runs 91 to 128 MB/s.
- **Fix (no fork of alacritty).** `Vt::advance` feeds a flood of text one line at a time (`vt/warm.rs`) and, before each line, prefetches the next oldest history rows, which are exactly the rows the next scrolls recycle (`warm.rs` pins that against the real alacritty after `Vt` settles the ring: alacritty grows it in batches of 1,000 spare rows, and with spares the recycled row is not the oldest; `Grid::truncate` once, when the history first reaches its cap). Findings that fixed its shape: a **burst** (all the rows of a 64 KiB chunk at once, 3,000 cache lines) gained nothing (ascii 95.1 against 95.6 with the fetch off), it overflows the CPU's miss queue; **paced** per line it gained 25 %; **one row ahead** 35 %; slices of 3 lines (200 bytes) lose most of it (104). Rows under 1 KiB (`seq`: 192 bytes) gain nothing and the fetch cost 10 % of the time there, so they take the plain path. A prefetch inside alacritty's `scroll_up` (a scratch fork, two rows ahead) reaches 122-134 on ascii: the same order, 5-10 % more.
- **The three scanners** found ESC with `iter().position` (2-3 % each on text, not vectorized); they use `memchr` (already in the lockfile through vte). ESC-heavy corpora gained 16-21 % (repaint256 139.5 to 162, ink-2026 153.6 to 185, omp-boxes 168.5 to 209), plain text 2-5 %.

### Phase 1 numbers (Ryzen 7 7735HS, load 1.1, `terminal-linux-parser-phase1.jsonl`, 3 runs; run 3 is the baseline)

| corpus | before (best) | after (best of 3 runs) | change | floor now (85 %) | design target |
|---|---|---|---|---|---|
| seq | 102.8 | 108.7 (101.8-108.7) | +6 % | 92 | >= 140 |
| ascii | 90.9 | 125.8 (115.7-125.8) | **+38 %** | 106 | >= 150 |
| sgr | 84.7 | 103.8 (103.4-104.3) | +23 % | 88 | hold |
| repaint256 | 139.5 | 160.8 | +15 % | 136 | hold |
| truecolor | 139.9 | 162.9 | +16 % | 138 | hold |
| ink-2026 | 153.6 | 184.7 | +20 % | 156 | hold |
| ratatui-diff | 177.4 | 204.6 | +15 % | 173 | hold |
| omp-boxes | 168.5 | 209.1 | +24 % | 177 | hold |

**The design's phase 1 targets (ascii >= 150, seq >= 140) are not met**: ascii reaches 126 (84 % of the target), seq stays at 109. What is left is in the part of alacritty that cannot be changed from outside: `Term::input` is 44-60 % of the time on both (`write_at_cursor`, `Row::index_mut`, `Storage::compute_index`, `Handler::input` per character, ranked by the sampler), and per scroll a `usize %` of a non-constant in `Storage::rotate`, `mark_fully_damaged` and the selection and vi-cursor bookkeeping (seq scrolls 1.99 M times) [INFERENCE: read from the source, not eliminated one by one]. Getting `seq` to 140 needs those changed in a fork of alacritty_terminal, or the engine of phase 6; the one piece of the fork that was measured (no drop, 4-6 %) is not enough. The floors above are what this PR holds. The calibration ratio of run 1 read 32 % high because its calibration was timed under a load spike (5,320 MB/s against 7,048-7,156): the 30 % tolerance is the margin for that, and a gate failure on a noisy runner is to be re-run once before it is believed.

## Pty read sizes

`terminal-linux-tools/pty_probe.py <corpus> [--opost] [--bufsize N]` cats a corpus into a pty and reports how the master hands it out (`terminal-linux-pty.jsonl`, load 0.6):

| corpus | output processing | reads | mean read | reads of exactly 4,095 B | MB/s |
|---|---|---|---|---|---|
| ascii | off | 2,698 | 6,218 B | 77 % | 254 |
| truecolor | off | 2,400 | 7,021 B | 59 % | 209 |
| seq, 1 MiB read buffer | off | 3,802 | 4,413 B | 92 % | 228 |
| ascii | **on** | 18,512 | 916 B | 0 % | 140 |
| seq | **on** | **194,406** | **97 B** | 0 % | 31 |

A real session has output processing on (`\n` to `\r\n`): a `seq` flood then arrives in ~100-byte pieces. The design (5.4) read that as per-read cost deciding throughput and made phase 2 (#253: coalesce reads, parse off the reader thread) its fix; the next section tests that.

## Read sizes, coalescing and a parse thread (phase 2)

**The premise does not hold, and the acceptance numbers (`seq` >= 100 MB/s, ASCII >= 120) are not reachable by this phase.**

- **`Vt::feed` per call is cheap.** `cargo bench -p atlas-term --bench vt_parse -- --chunk N` feeds N bytes per call (`terminal-linux-chunks.jsonl`, after phase 1, load 0.8-1.0, best of 5, MB/s):

| chunk | seq | ascii | sgr | repaint256 | truecolor | ink-2026 | ratatui-diff | omp-boxes |
|---|---|---|---|---|---|---|---|---|
| 97 B | 106.7 | 123.8 | 93.7 | 141.9 | 140.2 | 162.2 | 169.9 | 183.8 |
| 512 B | 115.6 | 122.0 | 93.8 | 152.0 | 151.2 | 176.0 | 175.6 | 193.1 |
| 4,095 B | 114.3 | 128.5 | 99.9 | 153.8 | 155.0 | 174.7 | 184.3 | 205.3 |
| 64 KiB | 111.8 | 129.7 | 100.3 | 157.0 | 157.3 | 177.2 | 193.2 | 200.3 |

  A 97-byte read costs 4.6 % on `seq` and ascii, 6.6 % on sgr and 8-12 % on the repaint and agent corpora (truecolor 11 %, ratatui-diff 12 %) against 64 KiB: the three locks and scans per call are not what held `seq` at 34 MB/s.
- **`seq` is capped by the pty, not by Atlas.** `pty_probe.py --opost seq`, which reads the master and parses nothing, gets 30.6 MB/s (194,406 reads of 97 B). The same flood into a reader that feeds `Vt` (`pty_flood.rs`: `cat` into a pty slave with output processing on, one 64 KiB `read` per call, load 0.7): `seq` **33.3-34.0 MB/s** on one thread, **33.1-33.4** with a reader thread plus a parse thread that takes everything queued (the coalescing is the backpressure), and **33.3** if the reader sleeps to coalesce (it then stalls the producer: the tty buffer is 4 KiB, ASCII dropped 96 to 13-23 MB/s with a 200-1,000 us sleep). Nothing in the consumer moves `seq` past the kernel's per-character output processing.
- **A parse thread does help where the pty is fast and parsing is the cost**, in that in-process single-reader configuration (3 runs each, MB/s): truecolor 115-117 to 158-161 (+37 %), ink-2026 128-132 to 160-185 (+27 %), sgr 82-85 to 94 (+11 %), ascii 96-99 to 102-112 (+10 %). That is the overlap of the `read` syscall with parsing.
- **Atlas does not read the pty in the app.** hostd reads it (in another process, already overlapping with the app's parsing) and the app reads hostd's socket (`lifecycle.rs::spawn_reader`, a 64 KiB buffer). [INFERENCE: not measured end to end] While the app parses, the socket fills, so its next read returns everything queued, large: the consumer is the bottleneck there and batches by itself, and a parse thread would overlap only the socket read, a fraction of the in-process gain above. It was not built: nothing here runs hostd's framing end to end, and the macOS reader that would carry it (`term/lifecycle.rs`) cannot be run on this machine.

Decision for the maintainer: keep phase 2 as written (a thread per terminal and a bounded ring, for up to +10-37 % on the in-process shape and nothing on `seq`), drop it, or re-scope it to the end-to-end measurement through hostd first. Nothing else in the plan depends on it (design: phases 1, 2, 4 and 6 are independent of 3).

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

`docs/perf/terminal-linux-baseline.json`, `terminal-linux-parser.jsonl` (phase 0 runs), `terminal-linux-parser-phase1.jsonl`, `terminal-linux-chunks.jsonl`, `terminal-linux-pty.jsonl`, `terminal-linux-xterm.jsonl`, `terminal-linux-tools/` (`up.sh`, `inenv.sh`, `down.sh`, `pty_probe.py`, `pty_flood.rs`, `xterm_inpage.py`, `ptrace_sample.py`, `alacritty-grid-counters.patch`, `grid_counters_main.rs`); `crates/atlas-term/benches/vt_parse.rs`, `crates/atlas-term/src/vt/warm.rs`; `scripts/term-corpus-gen.ts`, `term-parser-gate.ts`, `term-parser-judge.ts` (+ tests); the gate's CI step in `.github/workflows/ci.yml` and its row in `scripts/checks/map.ts`.
