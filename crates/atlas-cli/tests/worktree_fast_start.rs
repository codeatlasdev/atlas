//! Time to "ready" for a new worktree, cold vs warm (#130): the same steps Atlas Desktop runs
//! for an issue:start-like flow, against a real git repository and the real `atlas` binary.
//!
//! A worktree is ready after: `git worktree add`, the seed of `node_modules` (copy-on-write
//! clone, `atlas_core::seed`), and the setup, an install step plus a build declared as a cached
//! action (`atlas run --cache`). The first worktree is cold: nothing to clone from the cache, the
//! build runs. The second is warm: seeded from the main checkout, and its build is a cache hit
//! (same input bytes), so it restores `dist/` and runs nothing.
//!
//! The build and the install are fixed-cost stand-ins (a sleep plus real file writes), so the
//! numbers are not a benchmark of bun or cargo; they prove the mechanism (the warm path skips the
//! build, the seed is milliseconds) and hold the acceptance bound of the issue: well under 30 s.
//! The measured line is printed (`cargo test -p atlas-cli --test worktree_fast_start -- --nocapture`)
//! and quoted in docs/configuration.md. On a file system that cannot clone (Linux CI on ext4) the
//! seed is reported as skipped and only the cache saving is asserted.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use atlas_core::seed;

const BIN: &str = env!("CARGO_BIN_EXE_atlas");
/// Cost of the stand-in build, large enough that skipping it is unmistakable.
const BUILD_SECS: &str = "1.5";
const FILES: usize = 1500;

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    store: PathBuf,
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?}");
}

/// A repo with sources, a lockfile and a populated, gitignored `node_modules`.
fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("main");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/in.txt"), "source").unwrap();
    fs::write(root.join("package.json"), "{}").unwrap();
    fs::write(root.join("bun.lock"), "").unwrap();
    fs::write(root.join(".gitignore"), "node_modules\ndist\n").unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "init"]);
    for i in 0..FILES {
        let p = root.join(format!("node_modules/pkg{}/f{i}.js", i % 50));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, format!("module.exports = {i};")).unwrap();
    }
    Fixture {
        store: tmp.path().join("cache"),
        root,
        _tmp: tmp,
    }
}

/// `atlas run --cache` for the build, as `worktree.setup` expands `{{action: Build}}`.
fn cached_build(f: &Fixture, wt: &Path) -> (String, Duration) {
    let started = Instant::now();
    let out = Command::new(BIN)
        .env("ATLAS_LANG", "en")
        .current_dir(wt)
        .env("ATLAS_CACHE_DIR", &f.store)
        .env("HOME", f.store.parent().unwrap())
        .stdin(Stdio::null())
        .args(["run", "--cache", "--label", "Build", "--input", "src/**"])
        .args(["--output", "dist", "--"])
        .args(["sh", "-c"])
        .arg(format!(
            "sleep {BUILD_SECS}; mkdir -p dist; cp src/in.txt dist/out.txt"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        String::from_utf8_lossy(&out.stderr).into_owned(),
        started.elapsed(),
    )
}

/// Everything between `git worktree add` and "ready": seed (when asked) then setup. Returns
/// (total, seed report, build stderr, build time).
fn ready(
    f: &Fixture,
    name: &str,
    with_seed: bool,
) -> (Duration, Option<seed::Report>, String, Duration) {
    let started = Instant::now();
    let wt = f.root.parent().unwrap().join(name);
    git(
        &f.root,
        &["worktree", "add", "-q", "-b", name, wt.to_str().unwrap()],
    );
    let report = with_seed.then(|| {
        let plan = seed::plan(&f.root, &wt, None);
        seed::run(&f.root, &wt, plan)
    });
    // The install stand-in: without a seed the worktree has no node_modules and "installs" it.
    if !wt.join("node_modules").exists() {
        for i in 0..FILES {
            let p = wt.join(format!("node_modules/pkg{}/f{i}.js", i % 50));
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, format!("module.exports = {i};")).unwrap();
        }
    }
    let (build_log, build_time) = cached_build(f, &wt);
    assert!(
        wt.join("dist/out.txt").is_file(),
        "the build output is in the worktree"
    );
    (started.elapsed(), report, build_log, build_time)
}

// Guards #130: the second worktree of a project is ready faster than the first because its
// build is restored from the task cache and its node_modules is cloned, and an issue:start-like
// flow stays far under the 30 s acceptance bound.
#[test]
fn a_warm_worktree_is_ready_faster_than_a_cold_one_and_under_thirty_seconds() {
    let f = fixture();
    let (cold, _, cold_log, cold_build) = ready(&f, "cold", false);
    assert!(
        cold_log.contains("atlas cache: stored"),
        "the cold build runs and fills the cache: {cold_log}"
    );
    assert!(cold_build >= Duration::from_secs_f64(1.4), "{cold_build:?}");

    let (warm, report, warm_log, warm_build) = ready(&f, "warm", true);
    assert!(
        warm_log.contains("atlas cache: hit"),
        "the warm build is a cache hit: {warm_log}"
    );
    assert!(
        warm_build < Duration::from_secs_f64(1.0),
        "a hit must not pay the build: {warm_build:?}"
    );
    let report = report.unwrap();
    let seeded = report.seeded.contains(&"node_modules".to_string());
    if !seeded {
        // Not an APFS/reflink volume: the clone is skipped, never copied (documented).
        assert!(
            report
                .skipped
                .iter()
                .any(|(rel, why)| rel == "node_modules" && *why == seed::Skip::Unsupported),
            "{report:?}"
        );
    }
    println!(
        "worktree fast start: cold ready {cold:.2?} (install+build {cold_build:.2?}), warm ready {warm:.2?} \
         (seed {} ms{}, build hit {warm_build:.2?})",
        report.ms,
        if seeded {
            ""
        } else {
            ", clone unsupported on this volume"
        },
    );
    assert!(warm < cold, "warm {warm:?} must beat cold {cold:?}");
    assert!(
        warm < Duration::from_secs(30),
        "acceptance: under 30 s, got {warm:?}"
    );
}
