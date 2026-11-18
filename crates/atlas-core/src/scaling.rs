//! Cost checks that hold on a loaded machine (the Rust twin of `apps/desktop/test/scaling.ts`).
//!
//! A wall-clock budget ("a hit takes under 100 ms") measures the machine as much as the code: it fails on a loaded
//! CI runner with nothing regressed. What a regression changes is how the cost GROWS with the input, so these
//! helpers compare the cost at two sizes in the same process, interleaved so the load hits both alike:
//!
//! - a sample is the minimum of several runs (noise only ever adds time);
//! - the result is a ratio, so the speed of the machine cancels out.
//!
//! Growing the input `factor` times costs about `factor` times as much for a linear pass and `factor ** 2` for a
//! quadratic one (a rescan per item, a store walked per hit); a cost that must not depend on the size at all
//! stays near 1. Other guards count the expensive operations (processes spawned, files read) instead of timing them.

use std::time::{Duration, Instant};

/// The ceiling for waiting on a real event (a process exiting, a socket answering): it only detects a hang, so it
/// is generous. A bound tighter than this is a wall-clock budget, and fails on a loaded runner.
pub const HANG: Duration = Duration::from_secs(60);

/// How many times more `big` costs than `small`: the best of `runs` samples of each, taken alternately.
pub fn growth(runs: usize, mut small: impl FnMut(), mut big: impl FnMut()) -> f64 {
    small();
    big(); // warm up: page cache, allocator, lazy statics
    let (mut s, mut b) = (Duration::MAX, Duration::MAX);
    for _ in 0..runs.max(1) {
        let t = Instant::now();
        small();
        s = s.min(t.elapsed());
        let t = Instant::now();
        big();
        b = b.min(t.elapsed());
    }
    b.as_secs_f64() / s.as_secs_f64().max(1e-9)
}

/// The bound for "linear" when the input grew `factor` times: three times the linear cost is far from both a flat
/// cost (1) and a quadratic one (`factor ** 2`) at the usual factor of 8.
pub fn linear_bound(factor: usize) -> f64 {
    (factor * 3) as f64
}
