//! Behavior of the task cache on real files in temp dirs ("worktrees" are plain folders): what
//! decides the key, what is shared, what is never stored, the size cap, concurrent writers and
//! the speed of a hit. Guards issue #126.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;

fn write(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

/// A "worktree": a folder with a source file and an ignored build dir.
fn tree(root: &Path, src: &str) -> PathBuf {
    write(&root.join("src/main.rs"), src);
    write(&root.join(".gitignore"), "target\n");
    write(
        &root.join("target/junk.bin"),
        "ignored, must not move the key",
    );
    root.to_path_buf()
}

fn spec(cwd: &Path) -> Spec {
    Spec {
        cwd: cwd.to_path_buf(),
        argv: vec!["cargo".into(), "test".into()],
        inputs: vec!["src/**".into()],
        outputs: vec!["out".into()],
        env: vec![],
        tools: vec![],
    }
}

fn run(code: i32, ms: u64) -> Run {
    Run {
        project: "proj".into(),
        label: "test".into(),
        command: "cargo test".into(),
        exit: code,
        duration_ms: ms,
        log: vec![
            Chunk {
                stream: Stream::Stdout,
                bytes: b"out line\n".to_vec(),
            },
            Chunk {
                stream: Stream::Stderr,
                bytes: b"err line\n".to_vec(),
            },
        ],
    }
}

fn cache(t: &Path) -> Cache {
    Cache::at(t.join("store"), 64 << 20)
}

// Two worktrees, same argv, different uncommitted edits: never the same key.
#[test]
fn different_uncommitted_edits_give_different_keys() {
    let t = tempfile::tempdir().unwrap();
    let a = tree(&t.path().join("a"), "fn main() {}");
    let b = tree(&t.path().join("b"), "fn main() { dirty(); }");
    assert_ne!(spec(&a).key().unwrap(), spec(&b).key().unwrap());
}

// Same bytes in two different folders: one key (the folder is not an input).
#[test]
fn identical_inputs_in_two_dirs_share_a_key() {
    let t = tempfile::tempdir().unwrap();
    let a = tree(&t.path().join("a"), "fn main() {}");
    let b = tree(&t.path().join("b"), "fn main() {}");
    assert_eq!(spec(&a).key().unwrap(), spec(&b).key().unwrap());
}

// An edit to a file the glob matches changes the key; a gitignored file in the glob's reach, and a
// file outside the glob, do not.
#[test]
fn only_matched_non_ignored_files_move_the_key() {
    let t = tempfile::tempdir().unwrap();
    let a = tree(t.path(), "fn main() {}");
    let base = spec(&a).key().unwrap();
    write(&a.join("target/junk.bin"), "changed");
    write(&a.join("README.md"), "outside the glob");
    assert_eq!(spec(&a).key().unwrap(), base);
    // The ignored dir is skipped even when the glob would reach it.
    let mut wide = spec(&a);
    wide.inputs = vec!["**".into()];
    let wide_before = wide.key().unwrap();
    write(&a.join("target/junk.bin"), "changed again");
    assert_eq!(wide.key().unwrap(), wide_before);
    write(&a.join("src/main.rs"), "fn main() { x(); }");
    assert_ne!(spec(&a).key().unwrap(), base);
}

// A new file matching the glob, a rename and a deletion all change the key.
#[test]
fn adding_renaming_or_removing_an_input_changes_the_key() {
    let t = tempfile::tempdir().unwrap();
    let a = tree(t.path(), "fn main() {}");
    let k0 = spec(&a).key().unwrap();
    write(&a.join("src/new.rs"), "");
    let k1 = spec(&a).key().unwrap();
    assert_ne!(k0, k1);
    fs::rename(a.join("src/new.rs"), a.join("src/other.rs")).unwrap();
    let k2 = spec(&a).key().unwrap();
    assert_ne!(k1, k2);
    fs::remove_file(a.join("src/other.rs")).unwrap();
    assert_eq!(spec(&a).key().unwrap(), k0);
}

#[test]
fn argv_env_values_and_declared_globs_are_part_of_the_key() {
    let t = tempfile::tempdir().unwrap();
    let a = tree(t.path(), "x");
    let base = spec(&a);
    let k = base.key().unwrap();

    let mut other_argv = spec(&a);
    other_argv.argv = vec!["cargo".into(), "build".into()];
    assert_ne!(other_argv.key().unwrap(), k);

    let mut other_inputs = spec(&a);
    other_inputs.inputs = vec!["src/*.rs".into()];
    assert_ne!(other_inputs.key().unwrap(), k);

    let mut other_outputs = spec(&a);
    other_outputs.outputs = vec!["out2".into()];
    assert_ne!(other_outputs.key().unwrap(), k);

    // Env: the VALUE counts, and unset differs from empty.
    let name = "ATLAS_TASKCACHE_TEST_VAR";
    let mut e = spec(&a);
    e.env = vec![name.into()];
    unsafe { std::env::remove_var(name) };
    let unset = e.key().unwrap();
    unsafe { std::env::set_var(name, "") };
    let empty = e.key().unwrap();
    unsafe { std::env::set_var(name, "1") };
    let one = e.key().unwrap();
    unsafe { std::env::remove_var(name) };
    assert_ne!(unset, k, "declaring an env name changes the key");
    assert_ne!(unset, empty);
    assert_ne!(empty, one);
}

// A tool's `--version` is part of the key: a toolchain bump misses.
#[cfg(unix)]
#[test]
fn a_tool_version_change_misses() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let a = tree(&t.path().join("w"), "x");
    let tool = t.path().join("faketool");
    let set = |v: &str| {
        fs::write(&tool, format!("#!/bin/sh\necho faketool {v}\n")).unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
    };
    let mut s = spec(&a);
    s.tools = vec![tool.to_string_lossy().into_owned()];
    set("1.0");
    let k1 = s.key().unwrap();
    assert_eq!(s.key().unwrap(), k1, "stable while the version is");
    set("2.0");
    assert_ne!(s.key().unwrap(), k1);
    // A tool that is not installed is an error, never a silent "same as before".
    s.tools = vec!["/nonexistent/atlas-no-such-tool".into()];
    assert!(s.key().is_err());
}

#[test]
fn outputs_that_leave_the_project_are_refused() {
    let t = tempfile::tempdir().unwrap();
    let a = tree(t.path(), "x");
    for bad in ["../x", "/etc/passwd", "", ".", "a/../../b"] {
        let mut s = spec(&a);
        s.outputs = vec![bad.into()];
        assert!(s.key().is_err(), "{bad:?} must be refused");
    }
}

// Miss, record, then hit in ANOTHER folder: outputs restored, log replayed, one entry in the store.
#[test]
fn a_second_dir_with_identical_inputs_hits_the_first_dirs_entry() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "fn main() {}");
    let b = tree(&t.path().join("b"), "fn main() {}");
    assert!(c.lookup(&spec(&a).key().unwrap()).is_none());

    write(&a.join("out/bin/app"), "BINARY");
    write(&a.join("out/report.txt"), "ok");
    let sa = spec(&a);
    let ka = sa.key().unwrap();
    c.record(&ka, &sa, &run(0, 4200)).unwrap();

    let sb = spec(&b);
    let kb = sb.key().unwrap();
    let entry = c.lookup(&kb).expect("same bytes, other folder: a hit");
    assert_eq!((entry.exit, entry.duration_ms), (0, 4200));
    let restored = c.restore(&entry, &sb).unwrap();
    assert_eq!(restored.files, 2);
    assert_eq!(fs::read_to_string(b.join("out/bin/app")).unwrap(), "BINARY");
    assert_eq!(fs::read_to_string(b.join("out/report.txt")).unwrap(), "ok");
    let (mut so, mut se) = (Vec::new(), Vec::new());
    c.replay(&entry, &mut so, &mut se).unwrap();
    assert_eq!((so, se), (b"out line\n".to_vec(), b"err line\n".to_vec()));
    assert_eq!(c.status().entries, 1);
}

// Content addressing: two entries with the same output bytes store the blob once.
#[test]
fn identical_output_bytes_are_stored_once_across_entries() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "one");
    let b = tree(&t.path().join("b"), "two");
    let big = "x".repeat(100_000);
    write(&a.join("out/blob"), &big);
    write(&b.join("out/blob"), &big);
    let (sa, sb) = (spec(&a), spec(&b));
    c.record(&sa.key().unwrap(), &sa, &run(0, 1)).unwrap();
    let one = c.status().bytes;
    c.record(&sb.key().unwrap(), &sb, &run(0, 1)).unwrap();
    let two = c.status().bytes;
    assert_eq!(c.status().entries, 2);
    assert!(
        two - one < 10_000,
        "the second entry must add only its metadata, not the 100 kB blob again ({one} -> {two})"
    );
}

// A restore replaces a stale output tree: what `out/` held before is gone, as after the real run.
#[test]
fn restoring_a_directory_output_replaces_stale_files() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "x");
    write(&a.join("out/new.txt"), "new");
    let s = spec(&a);
    c.record(&s.key().unwrap(), &s, &run(0, 1)).unwrap();
    write(&a.join("out/stale.txt"), "left over from an older build");
    fs::remove_file(a.join("out/new.txt")).unwrap();
    let entry = c.lookup(&s.key().unwrap()).unwrap();
    c.restore(&entry, &s).unwrap();
    assert!(a.join("out/new.txt").is_file());
    assert!(!a.join("out/stale.txt").exists());
}

// Only exit 0 is stored by default; a failure is reported as not stored, and a later hit cannot
// replay it. (Failures are stored only when asked for.)
#[test]
fn a_failed_run_is_not_cached_by_default() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(t.path(), "x");
    let s = spec(&a);
    let k = s.key().unwrap();
    let r = c.record(&k, &s, &run(1, 10)).unwrap();
    assert_eq!(r, Recorded::NotStored(stats::Why::Failed));
    assert!(c.lookup(&k).is_none());
    assert_eq!(c.status().entries, 0);
    // ... but it is the last result an agent can ask about, with its log tail.
    let last = c
        .last_result("test", None)
        .expect("the failure is recorded");
    assert_eq!((last.exit, last.hit), (1, false));
    assert!(last.log_tail.contains("err line"));
}

#[test]
fn failures_are_stored_when_asked_and_replay_their_exit_code() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path()).keep_failures();
    let a = tree(t.path(), "x");
    let s = spec(&a);
    let k = s.key().unwrap();
    assert_eq!(c.record(&k, &s, &run(3, 10)).unwrap(), Recorded::Stored);
    assert_eq!(c.lookup(&k).unwrap().exit, 3);
}

// The cap: many inserts never push the store past it, and what goes is the least recently USED.
#[test]
fn the_store_never_exceeds_its_cap_and_evicts_the_least_recently_used() {
    let t = tempfile::tempdir().unwrap();
    let cap = 300_000u64;
    let c = Cache::at(t.path().join("store"), cap);
    let w = tree(&t.path().join("w"), "v0");
    let mut keys = Vec::new();
    for i in 0..40 {
        // A distinct 40 kB output per entry: only ~7 fit.
        write(&w.join("src/main.rs"), &format!("v{i}"));
        write(&w.join("out/blob"), &format!("{i}-{}", "z".repeat(40_000)));
        let s = spec(&w);
        let k = s.key().unwrap();
        c.record(&k, &s, &run(0, 1)).unwrap();
        keys.push(k);
        assert!(
            c.status().bytes <= cap,
            "over the cap after insert {i}: {}",
            c.status().bytes
        );
        // Keep the first entry hot: it must outlive its younger neighbours.
        if i > 0 && i < 39 {
            let _ = c.lookup(&keys[0]);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert!(c.lookup(&keys[0]).is_some(), "the entry in use was evicted");
    assert!(
        c.lookup(&keys[1]).is_none(),
        "the oldest unused entry stays"
    );
    assert!(c.lookup(&keys[39]).is_some(), "the newest entry is missing");
}

// One run bigger than the whole cap is not stored (and the cap still holds).
#[test]
fn an_entry_bigger_than_the_cap_is_not_stored() {
    let t = tempfile::tempdir().unwrap();
    let c = Cache::at(t.path().join("store"), 50_000);
    let a = tree(&t.path().join("a"), "x");
    write(&a.join("out/huge"), &"h".repeat(200_000));
    let s = spec(&a);
    let r = c.record(&s.key().unwrap(), &s, &run(0, 1)).unwrap();
    assert_eq!(r, Recorded::NotStored(stats::Why::TooBig));
    assert!(c.status().bytes <= 50_000);
}

// Two agents caching the same key at once leave one valid entry whose outputs restore intact.
#[test]
fn concurrent_writers_of_one_key_leave_one_valid_entry() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "x");
    write(&a.join("out/data"), &"d".repeat(50_000));
    let s = spec(&a);
    let k = s.key().unwrap();
    std::thread::scope(|sc| {
        for _ in 0..8 {
            sc.spawn(|| {
                let c = cache(t.path());
                c.record(&k, &s, &run(0, 7)).unwrap();
            });
        }
    });
    assert_eq!(c.status().entries, 1);
    let entry = c.lookup(&k).unwrap();
    let b = tree(&t.path().join("b"), "x");
    c.restore(&entry, &spec(&b)).unwrap();
    assert_eq!(fs::read(b.join("out/data")).unwrap().len(), 50_000);
    // No temp files left behind by the losers.
    let stray = walk(&t.path().join("store"))
        .into_iter()
        .filter(|p| p.file_name().unwrap().to_string_lossy().contains(".tmp-"))
        .count();
    assert_eq!(stray, 0);
}

fn walk(d: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for e in fs::read_dir(d).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            v.extend(walk(&p));
        } else {
            v.push(p);
        }
    }
    v
}

// The speed promise: a hit on a small entry (lookup, restore, replay) is well under 100 ms. The best of five
// hits is judged: a slow hit every time is a regression, one slow hit on a loaded CI runner is the scheduler.
// Windows gets 400 ms: on the CI runner (Defender scanning every restored file) one hit measured 129 ms and,
// on another run, the BEST of five 158 ms, so 100 ms there is not a promise Atlas keeps today; 400 ms still
// catches a hit that degrades into a rebuild (the recorded run takes 5 s).
#[test]
fn a_hit_on_a_small_entry_takes_under_100_ms() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "x");
    write(&a.join("out/a.txt"), "aaa");
    write(&a.join("out/b.txt"), "bbb");
    let s = spec(&a);
    c.record(&s.key().unwrap(), &s, &run(0, 5000)).unwrap();
    let b = tree(&t.path().join("b"), "x");
    let sb = spec(&b);
    let hit = || {
        let start = Instant::now();
        let k = sb.key().unwrap();
        let entry = c.lookup(&k).unwrap();
        c.restore(&entry, &sb).unwrap();
        c.replay(&entry, &mut Vec::new(), &mut Vec::new()).unwrap();
        c.note_hit(&entry, &k);
        start.elapsed()
    };
    let best = (0..5).map(|_| hit()).min().unwrap();
    let budget = Duration::from_millis(if cfg!(windows) { 400 } else { 100 });
    assert!(
        best < budget,
        "the best of five hits took {best:?} (budget {budget:?})"
    );
}

#[test]
fn stats_count_hits_misses_and_the_time_a_hit_saved() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "x");
    let s = spec(&a);
    let k = s.key().unwrap();
    c.record(&k, &s, &run(0, 9000)).unwrap();
    let entry = c.lookup(&k).unwrap();
    c.note_hit(&entry, &k);
    c.note_hit(&entry, &k);
    let st = c.stats();
    assert_eq!((st.total.hits, st.total.misses), (2, 1));
    assert_eq!(st.total.saved_ms, 18_000);
    let p = &st.projects["proj"];
    assert_eq!((p.hits, p.misses, p.saved_ms), (2, 1, 18_000));
    let ks = &st.keys[&k.to_string()];
    assert_eq!((ks.hits, ks.saved_ms), (2, 18_000));
    // The most recent run of "test" was a hit.
    let last = c.last_result("test", None).unwrap();
    assert!(last.hit);
    assert_eq!(last.saved_ms, Some(9000));
}

// Guards #129: the drawer shows "time saved, last 7 days", so hits are also counted per UTC day.
#[test]
fn saved_time_is_counted_per_day_and_the_last_week_ignores_older_days() {
    const DAY: u64 = 86_400_000;
    let mut st = Stats::default();
    let now = 100 * DAY + 5_000;
    st.add_hit_at(now - 10 * DAY, 4_000);
    st.add_hit_at(now - 6 * DAY - 1_000, 1_000);
    st.add_hit_at(now - DAY, 2_000);
    st.add_hit_at(now, 3_000);
    // Today plus the six days before it; the hit 10 days ago is out.
    assert_eq!(st.saved_since(now, 7), 1_000 + 2_000 + 3_000);
    assert_eq!(st.saved_since(now, 1), 3_000);
    assert_eq!(st.saved_since(now, 30), 10_000);
    // A clock that went backwards never counts the future as "recent" nor panics.
    assert_eq!(st.saved_since(5, 7), 0);
}

#[test]
fn per_day_counters_are_bounded_and_a_hit_through_the_store_lands_on_today() {
    const DAY: u64 = 86_400_000;
    let mut st = Stats::default();
    for d in 0..(stats::DAYS_KEEP as u64 + 10) {
        st.add_hit_at(d * DAY, 1);
    }
    assert_eq!(st.days.len(), stats::DAYS_KEEP);
    // The oldest days went, the newest stayed.
    assert!(!st.days.contains_key(&0u64.to_string()));
    assert!(
        st.days
            .contains_key(&(stats::DAYS_KEEP as u64 + 9).to_string())
    );

    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "x");
    let s = spec(&a);
    let k = s.key().unwrap();
    c.record(&k, &s, &run(0, 9000)).unwrap();
    let entry = c.lookup(&k).unwrap();
    c.note_hit(&entry, &k);
    let st = c.stats();
    assert_eq!(st.saved_since(store::now_ms(), 7), 9_000);
}

// A `stats.json` written before the daily counters existed still loads (counters kept, no days).
#[test]
fn a_stats_file_without_daily_counters_still_loads() {
    let old = r#"{"total":{"hits":2,"misses":1,"saved_ms":18000},"projects":{},"keys":{}}"#;
    let st: Stats = serde_json::from_str(old).unwrap();
    assert_eq!(st.total.saved_ms, 18_000);
    assert!(st.days.is_empty());
}

#[test]
fn clear_empties_the_store_and_gc_drops_blobs_no_entry_uses() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "x");
    write(&a.join("out/f"), "data");
    let s = spec(&a);
    c.record(&s.key().unwrap(), &s, &run(0, 1)).unwrap();
    // An orphan blob (a crashed writer's leftover) is what gc removes; the entry stays valid.
    let orphan = t.path().join("store/blobs/ff/ffffffff");
    write(&orphan, "orphan");
    let freed = c.gc();
    assert!(!orphan.exists());
    assert!(freed.bytes >= 6);
    assert!(c.lookup(&s.key().unwrap()).is_some());
    c.clear();
    assert_eq!(c.status().entries, 0);
    assert_eq!(c.status().bytes, 0);
    assert!(c.lookup(&s.key().unwrap()).is_none());
}

// An entry whose blob vanished (evicted by a racing writer, or a damaged disk) is a miss, never a
// half-restored tree and never a panic.
#[test]
fn an_entry_with_a_missing_blob_does_not_restore() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "x");
    write(&a.join("out/f"), "data");
    let s = spec(&a);
    let k = s.key().unwrap();
    c.record(&k, &s, &run(0, 1)).unwrap();
    for f in walk(&t.path().join("store/blobs")) {
        fs::remove_file(f).unwrap();
    }
    // Lookup itself notices (the hit would be a lie), so the caller runs the command instead.
    assert!(c.lookup(&k).is_none());
}

#[test]
fn log_blob_round_trips_and_keeps_the_newest_bytes() {
    let chunks = vec![
        Chunk {
            stream: Stream::Stdout,
            bytes: b"a".to_vec(),
        },
        Chunk {
            stream: Stream::Stderr,
            bytes: b"b".to_vec(),
        },
    ];
    assert_eq!(
        super::log::decode(&super::log::encode(&chunks)).unwrap(),
        chunks
    );
    assert!(
        super::log::decode(b"\x01\xff\xff\xff\xffxx").is_none(),
        "truncated blob"
    );
    let mut big = vec![Chunk {
        stream: Stream::Stdout,
        bytes: vec![b'x'; super::log::MAX_LOG_BYTES + 10],
    }];
    big.push(Chunk {
        stream: Stream::Stdout,
        bytes: b"END".to_vec(),
    });
    super::log::clamp(&mut big);
    let total: usize = big.iter().map(|c| c.bytes.len()).sum();
    assert_eq!(total, super::log::MAX_LOG_BYTES);
    assert_eq!(big.last().unwrap().bytes, b"END");
}

// The cap and directory come from the environment first, then ~/.atlas/config.json, then a
// default; garbage and zero count as unset (a cap of 0 would silently disable the cache).
#[test]
fn cap_precedence_is_env_then_config_then_default() {
    let t = tempfile::tempdir().unwrap();
    let cfg = t.path().join("config.json");
    fs::write(&cfg, r#"{"cache":{"max_bytes":12345}}"#).unwrap();
    assert_eq!(super::store::config_max_bytes(&cfg), Some(12345));
    fs::write(&cfg, r#"{"cache":{"max_bytes":"big"}}"#).unwrap();
    assert_eq!(super::store::config_max_bytes(&cfg), None);
    assert_eq!(
        super::store::config_max_bytes(&t.path().join("none.json")),
        None
    );
    assert_eq!(
        super::store::resolve_max(Some("777"), Some(5)),
        777,
        "env wins"
    );
    assert_eq!(super::store::resolve_max(None, Some(5)), 5);
    assert_eq!(
        super::store::resolve_max(Some("0"), None),
        super::store::DEFAULT_MAX_BYTES
    );
    assert_eq!(
        super::store::resolve_max(Some("lots"), Some(0)),
        super::store::DEFAULT_MAX_BYTES
    );
}

// Contract with the desktop Machine drawer (#129): `apps/desktop/src/__fixtures__/task_cache_stats.json`
// is the REAL serialization of `stats.json`. `UPDATE_FIXTURES=1` regenerates it.
#[test]
fn stats_fixture_is_the_real_serialization() {
    let t = tempfile::tempdir().unwrap();
    let c = cache(t.path());
    let a = tree(&t.path().join("a"), "x");
    let s = spec(&a);
    let k = s.key().unwrap();
    // Fixed key and times: a fixture that changes on every run is no fixture.
    let key = Key::parse(&"ab".repeat(32)).unwrap();
    let mut r = run(0, 9000);
    r.project = "atlas".into();
    r.label = "Run checks".into();
    c.record(&key, &s, &r).unwrap();
    let entry = c.lookup(&key).unwrap();
    c.note_hit(&entry, &key);
    c.note_hit(&entry, &key);
    let _ = k;
    let mut real = c.stats();
    for ks in real.keys.values_mut() {
        ks.last_at = 1_763_040_000_000;
    }
    // The day the hits landed on is the wall clock's: move it to the fixed time above.
    let today = real.days.values().next().cloned().unwrap();
    real.days = [((1_763_040_000_000u64 / 86_400_000).to_string(), today)].into();
    let fresh = serde_json::to_value(&real).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/desktop/src/__fixtures__/task_cache_stats.json");
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&fresh, &mut ser).unwrap();
        fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
    }
    let committed: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it"),
    )
    .unwrap();
    assert_eq!(
        committed, fresh,
        "stale fixture: re-run with UPDATE_FIXTURES=1"
    );
    // And the file reads back into the type the app's reader mirrors.
    let back: Stats = serde_json::from_value(committed).unwrap();
    assert_eq!(back, real);
}
