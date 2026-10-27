//! Capture timings: how long each stage of a still took, so "capture to card under 150 ms" is measured, not felt.
//!
//! `mark(stage, since)` records `since.elapsed()` under `stage` (the last 64 per stage) and prints it when
//! `ATLAS_SHOTS_PERF=1`. `shots_perf` (debug builds) returns the numbers to the agent bridge.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Instant;

use parking_lot::Mutex;
use serde::Serialize;

const KEEP: usize = 64;

static TABLE: LazyLock<Mutex<HashMap<&'static str, Vec<f64>>>> = LazyLock::new(Default::default);

fn table() -> &'static Mutex<HashMap<&'static str, Vec<f64>>> {
    &TABLE
}

/// Record how long `stage` took since `since`.
pub fn mark(stage: &'static str, since: Instant) {
    record(stage, since.elapsed().as_secs_f64() * 1000.0);
}

/// Record `stage` as the time since the selection was released (`Shots::released`, set when the commit reaches
/// `feed`): "release to overlay gone", "release to file", "release to card" are what the person waits for.
pub fn since_release(stage: &'static str, released: Option<Instant>) {
    if let Some(t) = released {
        mark(stage, t);
    }
}

pub fn record(stage: &'static str, ms: f64) {
    if std::env::var_os("ATLAS_SHOTS_PERF").is_some() {
        eprintln!("shots-perf: {stage} {ms:.1} ms");
    }
    let mut t = table().lock();
    let v = t.entry(stage).or_default();
    if v.len() == KEEP {
        v.remove(0);
    }
    v.push(ms);
}

/// Summary of one stage.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub stage: String,
    pub count: usize,
    pub last_ms: f64,
    pub p50_ms: f64,
    pub max_ms: f64,
}

/// `p`-th percentile (nearest rank) of `sorted`, which must be ascending and not empty.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = (p * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn stat(stage: &str, v: &[f64]) -> Option<Stat> {
    let last = *v.last()?;
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    Some(Stat {
        stage: stage.to_string(),
        count: v.len(),
        last_ms: last,
        p50_ms: percentile(&s, 0.5),
        max_ms: *s.last()?,
    })
}

/// Every recorded stage, by name.
pub fn stats() -> Vec<Stat> {
    let t = table().lock();
    let mut out: Vec<Stat> = t.iter().filter_map(|(k, v)| stat(k, v)).collect();
    out.sort_by(|a, b| a.stage.cmp(&b.stage));
    out
}

/// Debug builds: the stage timings recorded so far (`atlas desktop debug eval 'return await
/// window.__TAURI_INTERNALS__.invoke("shots_perf")'`).
#[cfg(debug_assertions)]
#[tauri::command]
pub fn shots_perf() -> Vec<Stat> {
    stats()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let v = [10.0, 20.0, 30.0, 40.0];
        assert_eq!(percentile(&v, 0.5), 20.0);
        assert_eq!(percentile(&v, 1.0), 40.0);
        assert_eq!(percentile(&[7.0], 0.5), 7.0);
    }

    #[test]
    fn a_stage_keeps_the_last_64_and_reports_last_median_and_max() {
        for i in 0..100 {
            record("unit-test-stage", f64::from(i));
        }
        let s = stats()
            .into_iter()
            .find(|s| s.stage == "unit-test-stage")
            .expect("recorded");
        assert_eq!(s.count, KEEP);
        assert_eq!(s.last_ms, 99.0);
        assert_eq!(s.max_ms, 99.0);
        // the oldest 36 fell out: 36..=99, median is the 32nd
        assert_eq!(s.p50_ms, 67.0);
    }

    #[test]
    fn the_stats_serialize_camel_case_for_the_bridge() {
        record("unit-test-json", 1.0);
        let v = serde_json::to_value(stats()).unwrap();
        let s = v
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["stage"] == "unit-test-json")
            .unwrap();
        assert!(s.get("lastMs").is_some() && s.get("p50Ms").is_some());
    }
}
