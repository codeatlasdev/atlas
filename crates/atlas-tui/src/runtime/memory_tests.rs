//! The pure rules of `memory.rs`: trend, budget state machine, leak detector, reload lines.

use super::*;

const T0: u64 = 1_700_000_000;

fn series(points: &[(u64, u64)]) -> Series {
    let mut s = Series::default();
    for (dt, mb) in points {
        s.push(T0 + dt, mb * MB);
    }
    s
}

/// One sample every 5 s starting at `start_mb`, changing by `mb_per_min` per minute.
fn ramp(start_mb: f64, mb_per_min: f64, samples: u64) -> Series {
    let mut s = Series::default();
    for i in 0..samples {
        let mb = start_mb + mb_per_min * (i * 5) as f64 / 60.0;
        s.push(T0 + i * 5, (mb * MB as f64) as u64);
    }
    s
}

// ── series ──────────────────────────────────────────────────────────────

#[test]
fn the_series_is_a_bounded_ring_that_drops_the_oldest() {
    let mut s = Series::default();
    for i in 0..(SERIES_CAP as u64 + 50) {
        s.push(T0 + i, i);
    }
    assert_eq!(s.len(), SERIES_CAP);
    assert_eq!(s.tail(None).first().unwrap().rss, 50, "oldest 50 evicted");
    assert_eq!(s.last().unwrap().rss, SERIES_CAP as u64 + 49);
}

#[test]
fn tail_returns_the_newest_n_oldest_first_and_more_than_stored_is_all() {
    let s = series(&[(0, 1), (5, 2), (10, 3), (15, 4)]);
    let t = s.tail(Some(2));
    assert_eq!(
        t.iter().map(|x| x.rss / MB).collect::<Vec<_>>(),
        vec![3, 4],
        "newest two, in time order"
    );
    assert_eq!(s.tail(Some(99)).len(), 4);
    assert_eq!(s.tail(None).len(), 4);
    assert!(Series::default().tail(Some(3)).is_empty());
}

// ── trend ───────────────────────────────────────────────────────────────

#[test]
fn a_steady_climb_is_rising_with_its_real_speed() {
    let t = trend(&ramp(500.0, 30.0, 30)).unwrap();
    assert_eq!(t.direction, Direction::Rising);
    assert!((t.mb_per_min - 30.0).abs() < 0.5, "{t:?}");
}

#[test]
fn a_steady_drop_is_falling_and_negative() {
    let t = trend(&ramp(2000.0, -45.0, 30)).unwrap();
    assert_eq!(t.direction, Direction::Falling);
    assert!((t.mb_per_min + 45.0).abs() < 0.5, "{t:?}");
}

#[test]
fn noise_around_a_level_is_flat() {
    let mut s = Series::default();
    for i in 0..40u64 {
        // +-3 MB saw-tooth
        let mb = 800 + if i % 2 == 0 { 3 } else { 0 };
        s.push(T0 + i * 5, mb * MB);
    }
    let t = trend(&s).unwrap();
    assert_eq!(t.direction, Direction::Flat, "{t:?}");
}

/// Boundary: exactly the flat threshold is still flat; a hair above is rising.
#[test]
fn the_flat_band_edge_is_exclusive() {
    assert_eq!(
        trend(&ramp(800.0, FLAT_MB_PER_MIN, 30)).unwrap().direction,
        Direction::Flat
    );
    assert_eq!(
        trend(&ramp(800.0, FLAT_MB_PER_MIN + 0.6, 30))
            .unwrap()
            .direction,
        Direction::Rising
    );
}

#[test]
fn too_little_data_says_nothing_instead_of_guessing() {
    assert!(trend(&Series::default()).is_none());
    assert!(trend(&ramp(500.0, 100.0, 3)).is_none(), "3 samples");
    // Enough samples but squeezed into a few seconds: no slope.
    let mut s = Series::default();
    for i in 0..10u64 {
        s.push(T0 + i, 500 * MB + i * 50 * MB);
    }
    assert!(trend(&s).is_none(), "span under the minimum");
}

/// An old spike must not keep the trend `rising` after the service settled: only the last
/// window counts.
#[test]
fn only_the_recent_window_decides_the_trend() {
    let mut s = Series::default();
    for i in 0..60u64 {
        s.push(T0 + i * 5, (500 + i * 20) * MB); // climbs for 5 min
    }
    let base = T0 + 60 * 5;
    for i in 0..80u64 {
        s.push(base + i * 5, 1700 * MB); // then holds for 6+ min
    }
    assert_eq!(trend(&s).unwrap().direction, Direction::Flat);
}

// ── budget ──────────────────────────────────────────────────────────────

fn lim(warn: Option<u64>, max: Option<u64>) -> Limits {
    Limits {
        warn: warn.map(|m| m * MB),
        max: max.map(|m| m * MB),
    }
}

#[test]
fn budget_is_ok_warn_over_by_the_thresholds_and_the_edges_are_exclusive() {
    let l = lim(Some(1500), Some(4000));
    assert_eq!(budget_of(1500 * MB, l), Budget::Ok, "== warn is still ok");
    assert_eq!(budget_of(1500 * MB + 1, l), Budget::Warn);
    assert_eq!(
        budget_of(4000 * MB, l),
        Budget::Warn,
        "== max is still warn"
    );
    assert_eq!(budget_of(4000 * MB + 1, l), Budget::Over);
}

#[test]
fn a_service_with_no_budget_is_always_ok() {
    assert_eq!(budget_of(u64::MAX, Limits::default()), Budget::Ok);
}

#[test]
fn only_one_threshold_configured_still_works() {
    assert_eq!(budget_of(2000 * MB, lim(Some(1500), None)), Budget::Warn);
    assert_eq!(budget_of(2000 * MB, lim(None, Some(1500))), Budget::Over);
}

/// A nonsensical `warn > max` (validation rejects it) must still answer `over`, never `ok`.
#[test]
fn max_wins_when_warn_is_misconfigured_above_it() {
    assert_eq!(
        budget_of(2000 * MB, lim(Some(3000), Some(1500))),
        Budget::Over
    );
}

#[test]
fn one_spike_does_not_change_the_state_two_in_a_row_do() {
    let l = lim(Some(1000), Some(3000));
    let mut m = BudgetMachine::default();
    assert_eq!(m.observe(1200 * MB, l), None, "first reading over warn");
    assert_eq!(m.state(), Budget::Ok);
    assert_eq!(m.observe(1300 * MB, l), Some(Budget::Warn));
    assert_eq!(m.state(), Budget::Warn);
}

#[test]
fn a_spike_between_normal_readings_never_enters() {
    let l = lim(Some(1000), None);
    let mut m = BudgetMachine::default();
    for mb in [800, 1500, 800, 1500, 800] {
        assert_eq!(m.observe(mb * MB, l), None);
    }
    assert_eq!(m.state(), Budget::Ok);
}

#[test]
fn it_clears_after_consecutive_readings_under_not_on_the_first() {
    let l = lim(Some(1000), None);
    let mut m = BudgetMachine::default();
    m.observe(1500 * MB, l);
    m.observe(1500 * MB, l);
    assert_eq!(m.state(), Budget::Warn);
    assert_eq!(m.observe(900 * MB, l), None, "one dip is not recovery");
    assert_eq!(m.observe(1500 * MB, l), None, "back over: still warn");
    assert_eq!(m.observe(900 * MB, l), None);
    assert_eq!(m.observe(900 * MB, l), Some(Budget::Ok));
}

#[test]
fn warn_to_over_to_warn_walks_the_states_one_confirmation_at_a_time() {
    let l = lim(Some(1000), Some(2000));
    let mut m = BudgetMachine::default();
    let mut seen = vec![];
    for mb in [1500, 1500, 2500, 2500, 1500, 1500, 500, 500] {
        if let Some(s) = m.observe(mb * MB, l) {
            seen.push(s);
        }
    }
    assert_eq!(
        seen,
        vec![Budget::Warn, Budget::Over, Budget::Warn, Budget::Ok]
    );
}

#[test]
fn a_jump_straight_over_max_skips_warn() {
    let l = lim(Some(1000), Some(2000));
    let mut m = BudgetMachine::default();
    m.observe(5000 * MB, l);
    assert_eq!(m.observe(5000 * MB, l), Some(Budget::Over));
}

#[test]
fn reset_forgets_the_previous_process() {
    let l = lim(Some(1000), None);
    let mut m = BudgetMachine::default();
    m.observe(1500 * MB, l);
    m.observe(1500 * MB, l);
    m.reset();
    assert_eq!(m.state(), Budget::Ok);
    assert_eq!(
        m.observe(1500 * MB, l),
        None,
        "needs two fresh readings again"
    );
}

#[test]
fn the_reason_names_both_numbers_in_the_requested_format() {
    assert_eq!(
        warn_reason(2254857830, 3 << 29),
        "using 2.1 GB, over memory_warn 1.5 GB"
    );
    assert_eq!(
        warn_reason(700 * MB, 512 * MB),
        "using 700 MB, over memory_warn 512 MB"
    );
}

// ── leak detector ───────────────────────────────────────────────────────

fn reloads(d: &mut LeakDetector, mbs: &[u64]) {
    for (i, mb) in mbs.iter().enumerate() {
        assert!(d.reload(T0 + i as u64 * 10, mb * MB), "reload {i} counted");
    }
}

/// The incident (#131): a `bun --hot` service grew ~50 MB per reload for a dozen reloads.
#[test]
fn growth_every_reload_is_a_suspected_leak_with_the_total_and_the_count() {
    let mut d = LeakDetector::default();
    let mbs: Vec<u64> = (0..13).map(|i| 1200 + i * 50).collect();
    reloads(&mut d, &mbs);
    let leak = d.suspect().expect("leak");
    assert_eq!(leak.growth_bytes, 600 * MB);
    assert_eq!(leak.cycles, 12);
    assert_eq!(leak.text(), "leak suspected: +600 MB over 12 reloads");
}

#[test]
fn a_service_that_returns_to_its_level_after_each_reload_is_not_a_leak() {
    let mut d = LeakDetector::default();
    reloads(&mut d, &[900, 940, 905, 950, 910, 945, 900, 950, 905]);
    assert!(d.suspect().is_none());
}

#[test]
fn too_few_cycles_never_flag() {
    let mut d = LeakDetector::default();
    reloads(&mut d, &[500, 700, 900]); // 3 cycles < LEAK_MIN_CYCLES, even though +400 MB
    assert!(d.suspect().is_none());
}

#[test]
fn growth_below_the_floor_is_noise() {
    let mut d = LeakDetector::default();
    reloads(&mut d, &[500, 510, 520, 530, 540, 550]); // +50 MB < 64 MB
    assert!(d.suspect().is_none());
}

#[test]
fn the_exact_minimum_cycles_and_growth_flag_and_one_less_does_not() {
    let step = LEAK_MIN_GROWTH / MB / (LEAK_MIN_CYCLES as u64 - 1);
    let mut ok = LeakDetector::default();
    let base = 500;
    // LEAK_MIN_CYCLES boundaries = LEAK_MIN_CYCLES - 1 reloads worth of growth
    let mbs: Vec<u64> = (0..LEAK_MIN_CYCLES as u64)
        .map(|i| base + i * (step + 1))
        .collect();
    reloads(&mut ok, &mbs);
    assert!(ok.suspect().is_some(), "{mbs:?}");

    let mut short = LeakDetector::default();
    reloads(&mut short, &mbs[..mbs.len() - 1]);
    assert!(short.suspect().is_none(), "one cycle fewer");
}

/// A real drop (a big GC, a cache flush) ends the run: only growth since then counts.
#[test]
fn a_big_drop_restarts_the_run_and_only_later_growth_counts() {
    let mut d = LeakDetector::default();
    reloads(&mut d, &[500, 600, 700, 800, 900]); // leak...
    reloads_at(&mut d, 50, &[300]); // ...flushed
    assert!(d.suspect().is_none(), "run restarted at 300");
    reloads_at(&mut d, 60, &[400, 500, 600, 700]);
    let leak = d.suspect().unwrap();
    assert_eq!(leak.growth_bytes, 400 * MB);
    assert_eq!(leak.cycles, 4);
}

fn reloads_at(d: &mut LeakDetector, from: u64, mbs: &[u64]) {
    for (i, mb) in mbs.iter().enumerate() {
        assert!(d.reload(T0 + (from + i as u64) * 10, mb * MB));
    }
}

#[test]
fn a_small_dip_is_tolerated_as_gc_not_a_reset() {
    let mut d = LeakDetector::default();
    // 1000 -> 990 is a 1% dip: still the same climbing run.
    reloads(&mut d, &[1000, 1100, 1090, 1200, 1300]);
    let leak = d.suspect().expect("dip tolerated");
    assert_eq!(leak.growth_bytes, 300 * MB);
}

#[test]
fn reload_lines_of_one_change_count_once() {
    let mut d = LeakDetector::default();
    assert!(d.reload(T0, 500 * MB));
    assert!(!d.reload(T0 + 1, 510 * MB), "within the debounce");
    assert!(d.reload(T0 + RELOAD_DEBOUNCE_S, 520 * MB), "edge: counts");
    assert_eq!(d.cycles(), 2);
}

#[test]
fn restarts_are_cycles_too_and_the_text_says_restarts() {
    let mut d = LeakDetector::default();
    for mb in [800, 900, 1000, 1100, 1200] {
        d.restart(mb * MB);
    }
    let leak = d.suspect().unwrap();
    assert_eq!(leak.kind, "restarts");
    assert_eq!(leak.text(), "leak suspected: +400 MB over 4 restarts");
}

#[test]
fn history_is_bounded() {
    let mut d = LeakDetector::default();
    for i in 0..(CYCLES_CAP as u64 * 3) {
        d.restart(i * MB);
    }
    assert_eq!(d.cycles(), CYCLES_CAP);
}

#[test]
fn clear_forgets_everything() {
    let mut d = LeakDetector::default();
    reloads(&mut d, &[500, 600, 700, 800, 900]);
    d.clear();
    assert!(d.suspect().is_none());
    assert_eq!(d.cycles(), 0);
}

// ── reload lines ────────────────────────────────────────────────────────

#[test]
fn the_common_watchers_reload_lines_are_recognised() {
    for l in [
        "[bun] reloaded",
        "\x1b[2m12:00:01\x1b[0m [vite] hmr update /src/App.svelte",
        "[vite] page reload src/main.ts",
        "[vite] server restarted.",
        "[nodemon] restarting due to changes...",
        "✓ Compiled /api/users in 212ms",
        "Compiled successfully in 1.2s",
        "Restarting: /app/src/server.ts has changed",
        "[Running 'cargo run']",
        "building...",
        "12:00:01 [vite] hmr update /src/lib.ts",
    ] {
        assert!(is_reload_line(l), "{l}");
    }
}

#[test]
fn ordinary_output_is_not_a_reload() {
    for l in [
        "GET /reload 200 3ms",
        "user clicked reload button",
        "connected to db",
        "ERROR failed to compile module",
        "",
        "listening on :3000",
        "Restarting worker pool is not needed",
    ] {
        assert!(!is_reload_line(l), "{l}");
    }
}
