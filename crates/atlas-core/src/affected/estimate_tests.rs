//! Duration statistics and the would-it-hit check, on a real cache store in a temp dir.
//! Guards issue #128: `atlas_estimate` answers from recorded data, never by running.

use std::fs;
use std::path::Path;

use super::estimate::{Durations, Estimate, estimate};
use crate::heavy::Dir;
use crate::taskcache::{Cache, Chunk, Run, Spec, Stream};

fn spec(cwd: &Path) -> Spec {
    Spec {
        cwd: cwd.to_path_buf(),
        argv: vec!["cargo test".into()],
        inputs: vec!["src/**".into()],
        outputs: vec![],
        env: vec![],
        tools: vec![],
    }
}

fn run(label: &str, ms: u64) -> Run {
    Run {
        project: "demo".into(),
        label: label.into(),
        command: "cargo test".into(),
        exit: 0,
        duration_ms: ms,
        log: vec![Chunk {
            stream: Stream::Stdout,
            bytes: b"ok\n".to_vec(),
        }],
    }
}

/// A heavy queue dir that exists, as it does once any job has run on the machine.
fn heavy_dir(root: &Path) -> Dir {
    fs::create_dir_all(root).unwrap();
    Dir::new(root)
}

fn project(root: &Path, src: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), src).unwrap();
}

#[test]
fn median_and_p90_of_the_recorded_runs() {
    let d = Durations::from_ms(vec![5000, 1000, 3000, 2000, 4000]);
    assert_eq!(d.runs(), 5);
    assert_eq!(d.median_ms(), Some(3000));
    // nearest-rank p90 of 5 samples is the 5th (ceil(0.9 * 5) = 5)
    assert_eq!(d.p90_ms(), Some(5000));
}

#[test]
fn median_of_an_even_count_is_the_mean_of_the_middle_two() {
    let d = Durations::from_ms(vec![1000, 2000, 3000, 4000]);
    assert_eq!(d.median_ms(), Some(2500));
}

#[test]
fn one_sample_is_its_own_median_and_p90_and_none_is_none() {
    let one = Durations::from_ms(vec![700]);
    assert_eq!((one.median_ms(), one.p90_ms()), (Some(700), Some(700)));
    let none = Durations::from_ms(vec![]);
    assert_eq!(
        (none.runs(), none.median_ms(), none.p90_ms()),
        (0, None, None)
    );
}

#[test]
fn p90_ignores_the_single_outlier_in_a_big_sample() {
    // 20 runs, one 100 s outlier: nearest-rank p90 is the 18th value.
    let mut v: Vec<u64> = (1..=19).map(|i| i * 1000).collect();
    v.push(100_000);
    let d = Durations::from_ms(v);
    assert_eq!(d.p90_ms(), Some(18_000));
}

#[test]
fn durations_come_from_the_cache_runs_that_really_ran_not_from_hits() {
    let t = tempfile::tempdir().unwrap();
    let cache = Cache::at(t.path().join("store"), 64 << 20);
    let work = t.path().join("w");
    project(&work, "1");
    let key = spec(&work).key().unwrap();
    for ms in [1000, 3000] {
        cache.record(&key, &spec(&work), &run("test", ms)).unwrap();
    }
    // a hit replays the ORIGINAL duration: it is not a new sample
    let entry = cache.lookup(&key).unwrap();
    cache.note_hit(&entry, &key);
    cache.note_hit(&entry, &key);

    let d = cache.durations_ms("test", Some("demo"));
    assert_eq!(d, [1000, 3000]);
    // found by the command line as well as the label; other project / name: nothing
    assert_eq!(cache.durations_ms("cargo test", None), [1000, 3000]);
    assert!(cache.durations_ms("test", Some("other")).is_empty());
    assert!(cache.durations_ms("nope", None).is_empty());
}

#[test]
fn estimate_prefers_the_cache_runs_and_falls_back_to_the_heavy_history() {
    let t = tempfile::tempdir().unwrap();
    let cache = Cache::at(t.path().join("store"), 64 << 20);
    let heavy = heavy_dir(&t.path().join("heavy"));
    let work = t.path().join("w");
    project(&work, "1");

    // Only the queue's history knows "gate": it is the source (whole seconds, as ms).
    heavy.record_run("gate", 20);
    heavy.record_run("gate", 30);
    let e = estimate(
        "gate",
        "cargo test",
        Some("demo"),
        Some(&spec(&work)),
        &cache,
        &heavy,
    );
    assert_eq!(
        (e.runs, e.median_ms, e.p90_ms),
        (2, Some(25_000), Some(30_000))
    );
    assert_eq!(e.sources, ["heavy history"]);

    // Once the cache recorded the label, it wins: a cached `--heavy` action is written to BOTH
    // stores by the same real run, so pooling them would count that run twice.
    cache
        .record(
            &spec(&work).key().unwrap(),
            &spec(&work),
            &run("gate", 10_000),
        )
        .unwrap();
    let e = estimate(
        "gate",
        "cargo test",
        Some("demo"),
        Some(&spec(&work)),
        &cache,
        &heavy,
    );
    assert_eq!(
        (e.runs, e.median_ms, e.p90_ms),
        (1, Some(10_000), Some(10_000))
    );
    assert_eq!(e.sources, ["task cache"]);
}

#[test]
fn estimate_with_no_history_says_so_instead_of_guessing() {
    let t = tempfile::tempdir().unwrap();
    let work = t.path().join("w");
    project(&work, "1");
    let e = estimate(
        "unknown",
        "cargo test",
        None,
        Some(&spec(&work)),
        &Cache::at(t.path().join("store"), 1 << 20),
        &heavy_dir(&t.path().join("heavy")),
    );
    assert_eq!((e.runs, e.median_ms, e.p90_ms), (0, None, None));
    assert!(e.sources.is_empty());
    assert!(!e.would_hit);
}

#[test]
fn would_hit_is_true_only_for_the_same_bytes_and_flips_on_an_edit() {
    let t = tempfile::tempdir().unwrap();
    let cache = Cache::at(t.path().join("store"), 64 << 20);
    let heavy = heavy_dir(&t.path().join("heavy"));
    let work = t.path().join("w");
    project(&work, "1");
    let ask = |work: &Path| -> Estimate {
        estimate(
            "test",
            "cargo test",
            Some("demo"),
            Some(&spec(work)),
            &cache,
            &heavy,
        )
    };

    assert!(!ask(&work).would_hit, "nothing stored yet");
    cache
        .record(
            &spec(&work).key().unwrap(),
            &spec(&work),
            &run("test", 5000),
        )
        .unwrap();
    let hit = ask(&work);
    assert!(hit.would_hit);
    assert_eq!(hit.saved_ms, Some(5000));
    assert_eq!(hit.key.as_deref().map(str::len), Some(64));

    // an uncommitted edit moves the key: a miss now
    project(&work, "2");
    let miss = ask(&work);
    assert!(!miss.would_hit);
    assert_eq!(miss.saved_ms, None);

    // another folder with the first bytes shares the entry (worktrees)
    let other = t.path().join("other");
    project(&other, "1");
    assert!(ask(&other).would_hit);
}

#[test]
fn would_hit_does_not_touch_the_counters_or_the_recent_runs() {
    let t = tempfile::tempdir().unwrap();
    let cache = Cache::at(t.path().join("store"), 64 << 20);
    let heavy = heavy_dir(&t.path().join("heavy"));
    let work = t.path().join("w");
    project(&work, "1");
    cache
        .record(
            &spec(&work).key().unwrap(),
            &spec(&work),
            &run("test", 5000),
        )
        .unwrap();
    let before = cache.stats();
    for _ in 0..3 {
        assert!(
            estimate(
                "test",
                "cargo test",
                None,
                Some(&spec(&work)),
                &cache,
                &heavy
            )
            .would_hit
        );
    }
    assert_eq!(cache.stats(), before);
    assert_eq!(cache.durations_ms("test", None), [5000]);
}

#[test]
fn a_spec_that_cannot_be_keyed_reports_why_and_still_gives_the_durations() {
    let t = tempfile::tempdir().unwrap();
    let cache = Cache::at(t.path().join("store"), 64 << 20);
    let heavy = heavy_dir(&t.path().join("heavy"));
    heavy.record_run("gate", 40);
    let mut s = spec(&t.path().join("w"));
    s.tools = vec!["definitely-not-a-program-atlas".into()];
    let e = estimate("gate", "x", None, Some(&s), &cache, &heavy);
    assert!(!e.would_hit);
    assert!(
        e.key_error
            .as_deref()
            .is_some_and(|m| m.contains("definitely-not")),
        "{e:?}"
    );
    assert_eq!(e.median_ms, Some(40_000));
}

#[test]
fn a_task_without_a_spec_has_a_duration_but_no_key_and_never_a_hit() {
    let t = tempfile::tempdir().unwrap();
    let heavy = heavy_dir(&t.path().join("heavy"));
    heavy.record_run("plain", 3);
    let e = estimate(
        "plain",
        "echo hi",
        None,
        None,
        &Cache::at(t.path().join("store"), 1 << 20),
        &heavy,
    );
    assert_eq!(e.median_ms, Some(3000));
    assert!(!e.would_hit && e.key.is_none() && e.key_error.is_none());
}
