//! The progress the window shows: one number in `0.0..=1.0` that never goes back and reaches
//! 1.0 only when the whole install is verified.
//!
//! The NSIS installer runs silently, so there is no progress channel to read. The install folder
//! grows while it unpacks, so the bytes written so far over the bytes expected (the payload's own
//! unpacked size) give a real, if coarse, measure. The steps around it take fixed slices:
//!
//! ```text
//! 0 ─ prepare (close Atlas, unpack the installer) ─ 0.08 ─ NSIS unpacks ─ 0.92 ─ verify ─ 1.0
//! ```

/// Slice of the bar where the NSIS installer's own work is shown.
const RUN_START: f32 = 0.08;
const RUN_END: f32 = 0.92;
/// Time alone moves the bar at most this far through the slice (see [`run_fraction`]).
const CREEP_SHARE: f32 = 0.5;

/// Monotonic, clamped progress.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    value: f32,
    done: bool,
}

impl Progress {
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Raise the bar to `v` (clamped to `0..=0.99`); a lower value or NaN changes nothing. Only
    /// [`Progress::finish`] gets to 1.0.
    pub fn raise(&mut self, v: f32) {
        if v.is_nan() || self.done {
            return;
        }
        self.value = self.value.max(v.clamp(0.0, 0.99));
    }

    /// The install is verified: the bar is full.
    pub fn finish(&mut self) {
        self.value = 1.0;
        self.done = true;
    }

    /// While the installer runs: bytes `written` of `expected` in the install folder, plus a
    /// slow creep with elapsed `secs`, so the bar still moves while NSIS registers or stalls.
    pub fn running(&mut self, written: u64, expected: u64, secs: f32) {
        self.raise(run_fraction(written, expected, secs));
    }
}

/// Where in `0..=1` the bar stands while the installer runs.
pub fn run_fraction(written: u64, expected: u64, secs: f32) -> f32 {
    let measured = if expected == 0 {
        0.0
    } else {
        (written as f64 / expected as f64).clamp(0.0, 1.0) as f32
    };
    // Creep: 0 → 0.5 of the slice over ~40 s, asymptotically (an installer that stalls is not
    // frozen on screen, and one that is slow never looks done).
    let creep = CREEP_SHARE * (1.0 - (-secs.max(0.0) / 20.0).exp());
    let f = measured.max(creep);
    RUN_START + f * (RUN_END - RUN_START)
}

/// Ease the displayed value toward the real one (`dt` seconds since the last frame), so the bar
/// glides instead of jumping when the folder size is sampled. Never overshoots, never goes back.
pub fn ease(shown: f32, target: f32, dt: f32) -> f32 {
    if target <= shown {
        return shown;
    }
    let k = 1.0 - (-dt.max(0.0) * 9.0).exp();
    (shown + (target - shown) * k).min(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_goes_back_and_only_finish_reaches_one() {
        let mut p = Progress::default();
        p.raise(0.4);
        p.raise(0.2);
        assert_eq!(p.value(), 0.4);
        p.raise(5.0);
        assert!(p.value() < 1.0, "raise must stop short of 1.0");
        p.finish();
        assert_eq!(p.value(), 1.0);
        p.raise(0.1);
        assert_eq!(p.value(), 1.0);
    }

    #[test]
    fn nan_and_negative_values_are_ignored() {
        let mut p = Progress::default();
        p.raise(f32::NAN);
        p.raise(-3.0);
        assert_eq!(p.value(), 0.0);
    }

    #[test]
    fn a_fully_written_folder_stays_below_the_verify_slice() {
        // Incident guard: 100 % must mean "verified", never "NSIS wrote the last byte".
        let f = run_fraction(10_000, 10_000, 0.0);
        assert!(f <= RUN_END + 1e-6, "{f}");
        let over = run_fraction(99_999, 10_000, 100.0);
        assert!(over <= RUN_END + 1e-6, "{over}");
    }

    #[test]
    fn the_bar_moves_with_bytes_and_with_time() {
        let a = run_fraction(0, 1_000, 0.0);
        let b = run_fraction(500, 1_000, 0.0);
        let c = run_fraction(1_000, 1_000, 0.0);
        assert!(RUN_START <= a && a < b && b < c, "{a} {b} {c}");
        // No bytes yet but time passes: still moving, still short of the end.
        let t0 = run_fraction(0, 1_000, 1.0);
        let t1 = run_fraction(0, 1_000, 30.0);
        assert!(
            t0 < t1 && t1 < (RUN_START + CREEP_SHARE * (RUN_END - RUN_START)),
            "{t0} {t1}"
        );
    }

    #[test]
    fn unknown_expected_size_still_creeps_without_dividing_by_zero() {
        let f = run_fraction(123, 0, 10.0);
        assert!(
            f > RUN_START && f < (RUN_START + CREEP_SHARE * (RUN_END - RUN_START)),
            "{f}"
        );
    }

    #[test]
    fn running_samples_in_any_order_stay_monotonic() {
        let mut p = Progress::default();
        let mut last = 0.0;
        for (w, s) in [(0, 0.0), (900, 1.0), (300, 2.0), (1_000, 3.0), (0, 4.0)] {
            p.running(w, 1_000, s);
            assert!(p.value() >= last, "went back at {w}");
            last = p.value();
        }
    }

    #[test]
    fn easing_glides_up_and_never_overshoots_or_returns() {
        let mut shown = 0.0;
        for _ in 0..200 {
            let next = ease(shown, 0.6, 1.0 / 60.0);
            assert!(next >= shown && next <= 0.6);
            shown = next;
        }
        assert!(shown > 0.59);
        assert_eq!(ease(0.8, 0.3, 0.016), 0.8);
    }
}
