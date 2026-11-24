// The counter run of docs/perf/terminal-linux.md ("Where the grid write goes"): feeds each corpus to `atlas_term::Vt` once and
// prints the counters that `alacritty-grid-counters.patch` adds to alacritty_terminal 0.26.0 (cell writes, writes that dropped
// a `Some(extra)`, cell resets, rows reset). Scratch crate, not part of the workspace:
//
//   cp -r ~/.cargo/registry/src/*/alacritty_terminal-0.26.0 /tmp/at && (cd /tmp/at && patch -p2 < <repo>/docs/perf/terminal-linux-tools/alacritty-grid-counters.patch)
//   # Cargo.toml: [dependencies] atlas-term = { path = "<repo>/crates/atlas-term" }, alacritty_terminal = "0.26";
//   #             [patch.crates-io] alacritty_terminal = { path = "/tmp/at" }; [workspace]; copy <repo>/Cargo.lock and rust-toolchain.toml
//   cargo run --release --offline -- <corpus dir>
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: <corpus dir>");
    for name in ["seq", "ascii", "sgr", "repaint256", "truecolor", "ink-2026", "ratatui-diff", "omp-boxes"] {
        let data = std::fs::read(format!("{dir}/{name}.bin")).unwrap();
        for c in alacritty_terminal::CNT.iter() {
            c.store(0, Relaxed);
        }
        let vt = atlas_term::Vt::new(120, 40, |_| {});
        let t = Instant::now();
        for chunk in data.chunks(65536) {
            vt.feed(chunk);
        }
        vt.flush_sync();
        let secs = t.elapsed().as_secs_f64();
        let c: Vec<u64> = alacritty_terminal::CNT.iter().map(|c| c.load(Relaxed)).collect();
        println!(
            "{name:<13} {:6.1} MB/s | writes {} over-Some {} template-Some {} | cell-reset {} over-Some {} | row-resets {} cells-in-row-resets {}",
            data.len() as f64 / 1e6 / secs,
            c[0],
            c[1],
            c[2],
            c[3],
            c[4],
            c[5],
            c[6]
        );
    }
}
