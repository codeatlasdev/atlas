//! Time to "ready" for a new worktree, cold vs warm (#130): the same steps Atlas Desktop runs
//! for an issue:start-like flow, against a real git repository and the real `atlas` binary.
//!
//! A worktree is ready after: `git worktree add`, the seed of `node_modules` (copy-on-write
//! clone, `atlas_core::seed`), and the setup, an install step plus a build declared as a cached
//! action (`atlas run --cache`). The first worktree is cold: nothing to clone from the cache, the
//! build runs. The second is warm: seeded from the main checkout, and its build is a cache hit
//! (same input bytes), so it restores `dist/` and runs nothing.
//!
//! The build and the install are stand-ins (real file writes, each build run counted in a file), so
//! the numbers are not a benchmark of bun or cargo. What is asserted is the mechanism, by counting
//! the work and not by timing it (a stopwatch fails on a loaded runner with nothing regressed): the
//! warm path runs the build zero times and the install zero times (when the volume can clone).
//! The measured line is printed (`cargo test -p atlas-cli --test worktree_fast_start -- --nocapture`)
//! and quoted in docs/configuration.md. On a file system that cannot clone (Linux CI on ext4) the
//! seed is reported as skipped and only the cache saving is asserted.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use atlas_core::scaling::HANG;
use atlas_core::seed;

const BIN: &str = env!("CARGO_BIN_EXE_atlas");
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

/// How many times the build stand-in really ran (one line per run, written by the command itself).
fn builds(f: &Fixture) -> usize {
    fs::read_to_string(f.store.parent().unwrap().join("builds")).map_or(0, |s| s.lines().count())
}

/// `atlas run --cache` for the build, as `worktree.setup` expands `{{action: Build}}`.
fn cached_build(f: &Fixture, wt: &Path) -> (String, Duration) {
    let started = Instant::now();
    let out = Command::new(BIN)
        .current_dir(wt)
        .env("ATLAS_CACHE_DIR", &f.store)
        .env("HOME", f.store.parent().unwrap())
        .stdin(Stdio::null())
        .args(["run", "--cache", "--label", "Build", "--input", "src/**"])
        .args(["--output", "dist", "--"])
        .args(["sh", "-c"])
        .arg(format!(
            "echo ran >> {}; mkdir -p dist; cp src/in.txt dist/out.txt",
            f.store.parent().unwrap().join("builds").display()
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

/// What getting a worktree ready did, and how long the whole thing took (printed, never asserted).
struct Ready {
    total: Duration,
    report: Option<seed::Report>,
    build_log: String,
    build_time: Duration,
    /// Whether the install stand-in had to write `node_modules` (a seed that cloned it spares that).
    installed: bool,
}

/// Everything between `git worktree add` and "ready": seed (when asked) then setup.
fn ready(f: &Fixture, name: &str, with_seed: bool) -> Ready {
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
    let installed = !wt.join("node_modules").exists();
    if installed {
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
    Ready {
        total: started.elapsed(),
        report,
        build_log,
        build_time,
        installed,
    }
}

// Guards #130: the second worktree of a project does less work than the first, because its build
// is restored from the task cache (the build runs once in all) and its node_modules is cloned
// (no install), and an issue:start-like flow finishes. Work is counted, not timed: the old guards
// (`warm_build < 1 s` against a 1.5 s sleep, `warm < cold`, `warm < 30 s`) failed on a loaded
// runner with nothing regressed. The 60 s ceiling only detects a hang.
#[test]
fn a_warm_worktree_runs_neither_the_build_nor_the_install_again() {
    let f = fixture();
    let cold = ready(&f, "cold", false);
    assert!(
        cold.build_log.contains("atlas cache: stored"),
        "the cold build runs and fills the cache: {}",
        cold.build_log
    );
    assert!(cold.installed, "the cold worktree installs node_modules");
    assert_eq!(builds(&f), 1, "the cold build ran once");

    let warm = ready(&f, "warm", true);
    assert!(
        warm.build_log.contains("atlas cache: hit"),
        "the warm build is a cache hit: {}",
        warm.build_log
    );
    assert_eq!(builds(&f), 1, "a hit must not run the build again");
    let report = warm.report.as_ref().unwrap();
    let seeded = report.seeded.contains(&"node_modules".to_string());
    if seeded {
        assert!(
            !warm.installed,
            "a cloned node_modules is not installed again"
        );
    } else {
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
        "worktree fast start: cold ready {:.2?} (install+build {:.2?}), warm ready {:.2?} \
         (seed {} ms{}, build hit {:.2?})",
        cold.total,
        cold.build_time,
        warm.total,
        report.ms,
        if seeded {
            ""
        } else {
            ", clone unsupported on this volume"
        },
        warm.build_time,
    );
    assert!(warm.total < HANG, "hang detector, got {:?}", warm.total);
}
