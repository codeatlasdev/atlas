use super::*;
use crate::runtime::logs::LogStream;

fn key() -> ViewKey {
    ViewKey {
        service: None,
        hidden: vec![],
        min_level: None,
        filter: None,
        find: None,
        json: false,
        window: Window::default(),
    }
}

fn buf(lines: &[(&str, &str)]) -> LogBuffer {
    let mut b = LogBuffer::new(10_000);
    for (s, l) in lines {
        b.push((*s).into(), (*l).into(), LogStream::Stdout);
    }
    b
}

#[test]
fn chatty_service_eviction_leaves_no_dead_rows_on_the_all_tab() {
    // `quiet` keeps its old lines while `chatty` (ring of 3) evicts: the
    // evicted seqs are *greater* than quiet's oldest, so trimming by the
    // global oldest seq would keep them.
    let mut b = LogBuffer::with_budget(3, usize::MAX);
    let mut k = key();
    k.find = Some(("x".into(), false));
    let mut ix = ViewIndex::new();
    b.push("quiet".into(), "x ERROR old".into(), LogStream::Stdout); // 0
    for i in 0..10 {
        b.push("chatty".into(), format!("x chatty {i}"), LogStream::Stdout);
        ix.sync(&b, &k);
        let live: Vec<u64> = b.all().iter().map(|e| e.seq).collect();
        assert_eq!(rows(&ix), live, "after chatty line {i}");
        assert!(rows(&ix).iter().all(|&s| b.get_any(s).is_some()));
    }
    // quiet's line (seq 0) is still there, chatty holds only its last 3.
    assert_eq!(rows(&ix), [0, 8, 9, 10]);
    assert_eq!(ix.total_hits(), 4, "hits of evicted rows are gone too");
    assert_eq!(ix.nth_hit(1), Some((8, 0)));
    assert_eq!((ix.error_count(), ix.warn_count()), (1, 0));
    // An evicted error leaves the jump list: only quiet's remains.
    b.push("quiet".into(), "x ERROR newer".into(), LogStream::Stdout); // 11
    b.push("quiet".into(), "x ERROR newest".into(), LogStream::Stdout); // 12
    for _ in 0..3 {
        b.push("quiet".into(), "x fine".into(), LogStream::Stdout);
        ix.sync(&b, &k);
    }
    let live: Vec<u64> = b.all().iter().map(|e| e.seq).collect();
    assert_eq!(rows(&ix), live);
    let errors = b
        .all()
        .iter()
        .filter(|e| e.level == LogLevel::Error)
        .count();
    assert_eq!(ix.error_count(), errors);
    assert_eq!(
        ix.jump_level(None, true, false),
        b.all()
            .iter()
            .find(|e| e.level == LogLevel::Error)
            .map(|e| e.seq)
    );
}

#[test]
fn index_that_fell_behind_the_eviction_log_rebuilds() {
    let mut b = LogBuffer::with_budget(2, usize::MAX);
    let mut ix = ViewIndex::new();
    b.push("quiet".into(), "old".into(), LogStream::Stdout);
    ix.sync(&b, &key());
    // Far more evictions than the log remembers between two syncs.
    for i in 0..(super::super::logs::EVICTION_LOG_FOR_TESTS + 10) {
        b.push("chatty".into(), format!("l{i}"), LogStream::Stdout);
    }
    ix.sync(&b, &key());
    let live: Vec<u64> = b.all().iter().map(|e| e.seq).collect();
    assert_eq!(rows(&ix), live);
}

#[test]
fn matcher_smartcase_literal_and_regex() {
    let m = Matcher::new("error", false).unwrap();
    assert!(m.is_match("An ERROR happened"));
    let m = Matcher::new("Error", false).unwrap();
    assert!(!m.is_match("an error"));
    // Literal: regex metacharacters are text.
    assert!(Matcher::new("a.b", false).unwrap().is_match("xa.by"));
    assert!(!Matcher::new("a.b", false).unwrap().is_match("axb"));
    assert!(Matcher::new("a.b", true).unwrap().is_match("axb"));
    assert!(Matcher::new("(", true).is_none());
    assert!(
        Matcher::compile("(", true)
            .unwrap_err()
            .contains("invalid regex")
    );
    assert!(Matcher::new("", false).is_none());
    // Empty matches never count as hits.
    assert_eq!(Matcher::new("x*", true).unwrap().count("abc"), 0);
    assert_eq!(Matcher::new("ab", false).unwrap().count("ab ab ab"), 3);
}

#[test]
fn index_follows_appends_without_rescanning() {
    let mut b = buf(&[("a", "one needle"), ("a", "two"), ("a", "needle needle")]);
    let mut k = key();
    k.find = Some(("needle".into(), false));
    let mut ix = ViewIndex::new();
    ix.sync(&b, &k);
    assert_eq!((ix.len(), ix.total_hits()), (3, 3));
    b.push("a".into(), "needle again".into(), LogStream::Stdout);
    ix.sync(&b, &k);
    assert_eq!((ix.len(), ix.total_hits()), (4, 4));
    assert_eq!(ix.nth_hit(0), Some((0, 0)));
    assert_eq!(ix.nth_hit(1), Some((2, 0)));
    assert_eq!(ix.nth_hit(2), Some((2, 1)));
    assert_eq!(ix.nth_hit(3), Some((3, 0)));
    assert_eq!(ix.nth_hit(4), None);
    assert_eq!(ix.hit_ordinal(2, 1), Some(2));
    assert_eq!(ix.hit_ordinal(1, 0), None);
}

#[test]
fn next_prev_wrap_and_start_from_position() {
    let b = buf(&[("a", "x"), ("a", "x"), ("a", "x")]);
    let mut k = key();
    k.find = Some(("x".into(), false));
    let mut ix = ViewIndex::new();
    ix.sync(&b, &k);
    assert_eq!(ix.next_hit(2), Some(0), "next wraps");
    assert_eq!(ix.prev_hit(0), Some(2), "prev wraps");
    assert_eq!(ix.next_hit(0), Some(1));
    assert_eq!(ix.prev_hit(2), Some(1));
    assert_eq!(ix.hit_ordinal_from_seq(1), 1);
    assert_eq!(ix.hit_ordinal_from_seq(99), 3);
    // No hits: no movement.
    let mut k2 = key();
    k2.find = Some(("zzz".into(), false));
    ix.sync(&b, &k2);
    assert_eq!((ix.next_hit(0), ix.prev_hit(0)), (None, None));
}

#[test]
fn filter_view_keeps_only_matching_rows_and_marks_are_dropped() {
    let mut b = buf(&[("a", "alpha"), ("a", "beta"), ("b", "alphabet")]);
    b.push_mark("a".into());
    let mut k = key();
    k.filter = Some(("alpha".into(), false));
    let mut ix = ViewIndex::new();
    ix.sync(&b, &k);
    assert_eq!(ix.seq_at(0), Some(0));
    assert_eq!(ix.len(), 2);
    // Single-service view.
    let mut k = key();
    k.service = Some("a".into());
    ix.sync(&b, &k);
    assert_eq!(ix.len(), 3, "alpha, beta, mark");
    // Hidden services are excluded from All.
    let mut k = key();
    k.hidden = vec!["b".into()];
    ix.sync(&b, &k);
    assert_eq!(ix.len(), 3);
}

#[test]
fn level_filter_and_jumps_wrap() {
    let b = buf(&[
        ("a", "ok"),
        ("a", "ERROR one"),
        ("a", "ok"),
        ("a", "WARN two"),
        ("a", "ok"),
        ("a", "ERROR three"),
    ]);
    let mut ix = ViewIndex::new();
    ix.sync(&b, &key());
    assert_eq!((ix.error_count(), ix.warn_count()), (2, 1));
    // Errors only.
    assert_eq!(ix.jump_level(None, true, false), Some(1));
    assert_eq!(ix.jump_level(Some(1), true, false), Some(5));
    assert_eq!(ix.jump_level(Some(5), true, false), Some(1), "wraps");
    assert_eq!(ix.jump_level(Some(5), false, false), Some(1));
    assert_eq!(ix.jump_level(Some(1), false, false), Some(5), "wraps back");
    assert_eq!(ix.jump_level(None, false, false), Some(5));
    // Warn + error.
    assert_eq!(ix.jump_level(Some(1), true, true), Some(3));
    assert_eq!(ix.jump_level(Some(3), true, true), Some(5));
    assert_eq!(ix.jump_level(Some(5), true, true), Some(1));
    assert_eq!(ix.jump_level(Some(5), false, true), Some(3));
    assert_eq!(ix.jump_level(Some(1), false, true), Some(5));
    // min_level view only holds those rows.
    let mut k = key();
    k.min_level = Some(LogLevel::Error);
    ix.sync(&b, &k);
    assert_eq!(ix.len(), 2);
    // No problems anywhere → None.
    let quiet = buf(&[("a", "fine")]);
    ix.sync(&quiet, &key());
    assert_eq!(ix.jump_level(None, true, true), None);
}

#[test]
fn eviction_trims_rows_and_hits_stay_consistent() {
    let mut b = LogBuffer::new(3);
    let mut k = key();
    k.find = Some(("n".into(), false));
    let mut ix = ViewIndex::new();
    for i in 0..10 {
        b.push("a".into(), format!("n{i}"), LogStream::Stdout);
        ix.sync(&b, &k);
    }
    assert_eq!((ix.len(), ix.total_hits()), (3, 3));
    assert_eq!(ix.seq_at(0), Some(7));
    assert_eq!(ix.nth_hit(0), Some((7, 0)));
    assert_eq!(ix.nth_hit(2), Some((9, 0)));
    assert_eq!(ix.hit_ordinal(9, 0), Some(2));
}

#[test]
fn clear_rebuilds_from_empty() {
    let mut b = buf(&[("a", "x"), ("a", "x")]);
    let mut ix = ViewIndex::new();
    ix.sync(&b, &key());
    assert_eq!(ix.len(), 2);
    b.clear();
    ix.sync(&b, &key());
    assert!(ix.is_empty());
    b.push("a".into(), "y".into(), LogStream::Stdout);
    ix.sync(&b, &key());
    assert_eq!(ix.len(), 1);
}

#[test]
fn json_flag_changes_the_searched_text() {
    let b = buf(&[("a", r#"{"level":30,"msg":"hi","user":"ana"}"#)]);
    let mut k = key();
    k.filter = Some(("user=ana".into(), false));
    let mut ix = ViewIndex::new();
    ix.sync(&b, &k);
    assert!(ix.is_empty());
    k.json = true;
    ix.sync(&b, &k);
    assert_eq!(ix.len(), 1);
}

#[test]
fn invalid_filter_shows_nothing() {
    let b = buf(&[("a", "x")]);
    let mut k = key();
    k.filter = Some(("(".into(), true));
    let mut ix = ViewIndex::new();
    ix.sync(&b, &k);
    assert!(ix.is_empty());
}

#[test]
fn index_survives_restart_markers_and_counts_them_as_rows() {
    let mut b = buf(&[("a", "boot")]);
    b.push_restart("a".into());
    b.push("a".into(), "ERROR after".into(), LogStream::Stdout);
    let mut ix = ViewIndex::new();
    ix.sync(&b, &key());
    assert_eq!(ix.len(), 3);
    assert_eq!(ix.error_count(), 1);
}

// ---- time windows ---------------------------------------------------

use crate::runtime::timerange::{TimeRange, resolve};

fn put(b: &mut LogBuffer, svc: &str, text: &str, ts: u64) {
    b.push_at(svc.into(), text.into(), LogStream::Stdout, ts);
}

fn rows(ix: &ViewIndex) -> Vec<u64> {
    ix.seqs(0..ix.len()).collect()
}

#[test]
fn sliding_window_drops_aged_lines_from_the_front_and_keeps_counts() {
    let mut b = LogBuffer::new(1000);
    put(&mut b, "a", "old ERROR", 1_000); // 0
    put(&mut b, "a", "WARN mid", 61_000); // 1
    put(&mut b, "a", "fresh", 100_000); // 2
    let mut k = key();
    k.window = Window {
        last_ms: Some(60_000),
        ..Window::default()
    };
    let mut ix = ViewIndex::new();
    ix.sync_at(&b, &k, 100_000); // cutoff 40_000 → seq 0 out
    assert_eq!(rows(&ix), [1, 2]);
    assert_eq!((ix.error_count(), ix.warn_count()), (0, 1));
    // Less than a step later: not re-evaluated.
    ix.sync_at(&b, &k, 100_000 + CLOCK_STEP_MS - 1);
    assert_eq!(rows(&ix), [1, 2]);
    // The WARN line ages out (cutoff 61_001 > 61_000).
    ix.sync_at(&b, &k, 121_001);
    assert_eq!(rows(&ix), [2]);
    assert_eq!((ix.error_count(), ix.warn_count()), (0, 0));
    assert_eq!(ix.jump_level(None, true, true), None);
    // Everything ages out; a new line still shows up.
    ix.sync_at(&b, &k, 500_000);
    assert!(ix.is_empty());
    put(&mut b, "a", "newer", 500_000);
    ix.sync_at(&b, &k, 500_000);
    assert_eq!(rows(&ix), [3]);
}

#[test]
fn window_composes_with_level_filter_and_find_hits_follow_dropped_rows() {
    let mut b = LogBuffer::new(1000);
    put(&mut b, "a", "needle ERROR", 10_000); // 0
    put(&mut b, "a", "needle needle ERROR", 70_000); // 1
    put(&mut b, "a", "needle info", 71_000); // 2
    let mut k = key();
    k.min_level = Some(LogLevel::Error);
    k.find = Some(("needle".into(), false));
    k.window = Window {
        last_ms: Some(120_000),
        ..Window::default()
    };
    let mut ix = ViewIndex::new();
    ix.sync_at(&b, &k, 80_000);
    assert_eq!((ix.len(), ix.total_hits()), (2, 3));
    ix.sync_at(&b, &k, 135_000); // cutoff 15_000: row 0 gone
    assert_eq!((ix.len(), ix.total_hits()), (1, 2));
    assert_eq!(ix.nth_hit(0), Some((1, 0)));
    assert_eq!(ix.hit_ordinal(1, 1), Some(1));
}

#[test]
fn since_restart_cuts_at_the_marker_and_follows_new_restarts() {
    let mut b = LogBuffer::new(1000);
    put(&mut b, "a", "one", 1_000);
    put(&mut b, "b", "other", 2_000);
    b.push_restart("a".into());
    put(&mut b, "a", "two", 3_000);
    let mut ix = ViewIndex::new();
    let mut k = key();
    k.window = resolve(TimeRange::SinceRestart, &b, None, 4_000).unwrap();
    ix.sync_at(&b, &k, 4_000);
    // The marker itself opens the window.
    assert_eq!(rows(&ix), [2, 3]);
    // A service tab is bounded by its own marker only.
    let mut kb = key();
    kb.service = Some("b".into());
    kb.window = resolve(TimeRange::SinceRestart, &b, Some("b"), 4_000).unwrap();
    ix.sync_at(&b, &kb, 4_000);
    assert_eq!(rows(&ix), [1]);
    // A later restart changes the resolved window → one rebuild.
    b.push_restart("b".into());
    put(&mut b, "b", "again", 5_000);
    kb.window = resolve(TimeRange::SinceRestart, &b, Some("b"), 5_000).unwrap();
    ix.sync_at(&b, &kb, 5_000);
    assert_eq!(rows(&ix), [4, 5]);
}

#[test]
fn custom_range_bounds_both_ends() {
    let mut b = LogBuffer::new(1000);
    for (i, ts) in [100u64, 200, 300, 400].into_iter().enumerate() {
        put(&mut b, "a", &format!("l{i}"), ts);
    }
    let mut k = key();
    k.window = Window {
        from_ms: Some(200),
        to_ms: Some(300),
        ..Window::default()
    };
    let mut ix = ViewIndex::new();
    ix.sync_at(&b, &k, 1_000);
    assert_eq!(rows(&ix), [1, 2], "both ends inclusive");
}

/// Tiny deterministic generator (no rand dependency).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// What the view must show, computed from scratch.
fn brute(b: &LogBuffer, k: &ViewKey, now: u64) -> Vec<u64> {
    let cutoff = k.window.cutoff(now);
    let filter = k.filter.as_ref().and_then(|(q, re)| Matcher::new(q, *re));
    b.all()
        .into_iter()
        .filter(|e| k.service.as_deref().is_none_or(|s| s == e.service))
        .filter(|e| k.service.is_some() || !k.hidden.contains(&e.service))
        .filter(|e| k.window.allows(e.seq, e.timestamp, cutoff))
        .filter(|e| e.is_separator() || k.min_level.is_none_or(|m| e.level.rank() >= m.rank()))
        .filter(|e| {
            filter
                .as_ref()
                .is_none_or(|m| !e.is_separator() && m.is_match(&e.content))
        })
        .map(|e| e.seq)
        .collect()
}

#[test]
fn incremental_window_matches_brute_force_on_random_streams() {
    let words = ["alpha", "beta ERROR", "gamma WARN", "delta", "needle x"];
    let mut total_compactions = 0;
    for seed in 1..=12u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        // Small per-service rings and skewed volumes: the chatty service
        // evicts constantly while the quiet ones keep their old lines, so
        // dead rows sit in the middle of the All view.
        let mut b = LogBuffer::new(4 + rng.below(30) as usize);
        let mut ix = ViewIndex::new();
        let mut now = 1_000_000u64;
        let mut k = key();
        let mut range = TimeRange::All;
        let mut seen_compaction = false;
        let svcs = ["a", "a", "a", "a", "a", "a", "b", "b", "b", "c"];
        for step in 0..200 {
            now += rng.below(4_000); // time never goes back
            match rng.below(12) {
                0 => {
                    b.push_restart(svcs[rng.below(10) as usize].into());
                }
                1 => {
                    b.push_mark(svcs[rng.below(10) as usize].into());
                }
                _ => put(
                    &mut b,
                    svcs[rng.below(10) as usize],
                    words[rng.below(5) as usize],
                    now,
                ),
            }
            if step % 25 == 0 {
                // Redefine the view now and then (the one-time rebuild).
                k.service = [None, Some("a".to_string())][rng.below(2) as usize].clone();
                k.hidden = if rng.below(3) == 0 {
                    vec!["c".into()]
                } else {
                    vec![]
                };
                k.min_level =
                    [None, Some(LogLevel::Warn), Some(LogLevel::Error)][rng.below(3) as usize];
                k.filter = (rng.below(4) == 0).then(|| ("needle".to_string(), false));
                k.find =
                    (k.filter.is_none() && rng.below(2) == 0).then(|| ("a".to_string(), false));
                range = match rng.below(5) {
                    0 => TimeRange::All,
                    1 => TimeRange::LastMinutes(1),
                    2 => TimeRange::SinceRestart,
                    3 => TimeRange::SinceStackStart,
                    _ => TimeRange::LastMinutes(5),
                };
            }
            // Like the app: the window is re-resolved on every sync.
            k.window = resolve(range, &b, k.service.as_deref(), now).unwrap();
            ix.sync_at(&b, &k, now);
            // Throttled clock: compare at the clock the index last used.
            let want = brute(&b, &k, ix.clock_ms);
            assert_eq!(rows(&ix), want, "seed {seed} step {step}");
            let want_err = want
                .iter()
                .filter(|&&s| {
                    b.get_any(s)
                        .is_some_and(|e| !e.is_separator() && e.level == LogLevel::Error)
                })
                .count();
            assert_eq!(ix.error_count(), want_err, "seed {seed} step {step}");
            if let Some((q, re)) = &k.find {
                let m = Matcher::new(q, *re).unwrap();
                let hits: usize = want
                    .iter()
                    .filter_map(|&s| b.get_any(s))
                    .filter(|e| !e.is_separator())
                    .map(|e| m.count(&e.content))
                    .sum();
                assert_eq!(ix.total_hits(), hits, "seed {seed} step {step}");
            }
            // Every row the view hands the renderer still exists.
            assert!(
                rows(&ix).iter().all(|&s| b.get_any(s).is_some()),
                "seed {seed} step {step}: a row points at an evicted entry"
            );
            seen_compaction |= ix.compactions > 0;
        }
        total_compactions += usize::from(seen_compaction);
    }
    // Marks/restarts carry the real clock, so straggler rows do occur.
    assert!(total_compactions > 0, "the compaction path was never taken");
}

// ── cost guards (they replaced the wall-clock budgets of the 1M-line bench) ──────────────────────────
//
// "Ingest 1M lines under 15 s" and "an incremental find under 250 ms" measured the machine as much as the
// code. What a regression changes is how the cost GROWS: `sync` rescanning the whole buffer after an append,
// or the ring evicting in time proportional to what it holds. Each test compares the cost at two sizes in
// one process (`atlas_core::scaling::growth`), so the speed of the machine cancels out.

/// A buffer of `lines` lines, one in ten holding the needle.
fn filled(lines: usize) -> LogBuffer {
    let mut b = LogBuffer::with_budget(usize::MAX, usize::MAX);
    for i in 0..lines {
        let text = if i % 10 == 0 { "needle" } else { "hay" };
        b.push("a".into(), format!("{text} #{i}"), LogStream::Stdout);
    }
    b
}

/// An append only costs the new lines: syncing 200 appended lines is as cheap on a buffer 8 times larger
/// (flat, 1x). A `sync` that rescanned the buffer would cost 8x.
#[test]
fn a_sync_after_appends_costs_the_new_lines_not_the_history() {
    let mut k = key();
    k.find = Some(("needle".into(), false));
    let (mut small, mut big) = (filled(10_000), filled(80_000));
    let (mut small_ix, mut big_ix) = (ViewIndex::new(), ViewIndex::new());
    small_ix.sync(&small, &k);
    big_ix.sync(&big, &k);
    let append = |b: &mut LogBuffer, ix: &mut ViewIndex| {
        for i in 0..200 {
            b.push("a".into(), format!("needle new #{i}"), LogStream::Stdout);
        }
        ix.sync(b, &k);
    };
    const RUNS: usize = 15;
    let ratio = atlas_core::scaling::growth(
        RUNS,
        || append(&mut small, &mut small_ix),
        || append(&mut big, &mut big_ix),
    );
    assert!(
        ratio < 4.0,
        "an append to a buffer 8x larger cost {ratio:.1}x (flat is 1x, a rescan 8x)"
    );
    assert_eq!(
        big_ix.len(),
        80_000 + 200 * (RUNS + 1),
        "the index did follow the appends"
    );
}

/// Ingesting a line is constant work, eviction included: 8 times the lines into a ring 8 times bigger
/// costs about 8x. An eviction that walked the ring (or a push that recounted what it holds) costs 64x.
#[test]
fn ingest_with_eviction_costs_a_constant_per_line() {
    let ingest = |ring: usize, lines: usize| {
        let mut b = LogBuffer::with_budget(ring, usize::MAX);
        for i in 0..lines {
            b.push(
                "a".into(),
                format!("GET /api/users/42 200 #{i}"),
                LogStream::Stdout,
            );
        }
        assert_eq!(b.len(), ring, "the ring evicted down to its capacity");
    };
    let ratio = atlas_core::scaling::growth(5, || ingest(1_000, 4_000), || ingest(8_000, 32_000));
    let bound = atlas_core::scaling::linear_bound(8);
    assert!(
        ratio < bound,
        "8x the lines cost {ratio:.1}x (linear is 8x, quadratic 64x, bound {bound})"
    );
}
