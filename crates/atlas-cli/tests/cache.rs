//! `atlas run --cache` and `atlas cache` against the real binary: a repeat of the same work in
//! another folder is a hit that restores outputs, replays the log and runs nothing; edits, env and
//! failures behave as the key and the policy say. Each test has its own store (`ATLAS_CACHE_DIR`),
//! so nothing touches `~/.atlas`. Guards issue #126.

// The fixtures are `sh` one-liners.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

struct Env {
    _tmp: tempfile::TempDir,
    store: PathBuf,
    root: PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    Env {
        store: root.join("store"),
        root,
        _tmp: tmp,
    }
}

/// A "worktree" with one source file, and a counter file proving how many times the job ran.
fn worktree(e: &Env, name: &str, src: &str) -> PathBuf {
    let w = e.root.join(name);
    fs::create_dir_all(w.join("src")).unwrap();
    fs::write(w.join("src/in.txt"), src).unwrap();
    w
}

/// `atlas run --cache --input 'src/**' --output out -- sh -c <script>` in `cwd`.
fn cached(e: &Env, cwd: &Path, script: &str, extra: &[&str]) -> Output {
    Command::new(BIN)
        .current_dir(cwd)
        .env("ATLAS_CACHE_DIR", &e.store)
        .env("HOME", &e.root)
        .stdin(Stdio::null())
        .args(["run", "--cache", "--label", "build", "--input", "src/**"])
        .args(["--output", "out"])
        .args(extra)
        .args(["--", "sh", "-c", script])
        .output()
        .unwrap()
}

fn atlas_cache(e: &Env, args: &[&str]) -> Output {
    Command::new(BIN)
        .env("ATLAS_CACHE_DIR", &e.store)
        .env("HOME", &e.root)
        .args(["cache"])
        .args(args)
        .output()
        .unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// The job: counts its runs in `$RUNS` (outside the project), writes an output, prints on both streams.
fn job(e: &Env) -> String {
    format!(
        "echo x >> {runs}; mkdir -p out; cp src/in.txt out/result.txt; echo to-stdout; echo to-stderr >&2",
        runs = e.root.join("runs").display()
    )
}

fn runs(e: &Env) -> usize {
    fs::read_to_string(e.root.join("runs")).map_or(0, |s| s.lines().count())
}

#[test]
fn the_same_work_in_another_folder_is_a_hit_that_runs_nothing() {
    let e = env();
    let (a, b) = (worktree(&e, "a", "same"), worktree(&e, "b", "same"));
    let first = cached(&e, &a, &job(&e), &[]);
    assert!(first.status.success(), "{}", text(&first.stderr));
    assert_eq!(runs(&e), 1);
    assert!(text(&first.stdout).contains("to-stdout"));

    let second = cached(&e, &b, &job(&e), &[]);
    assert!(second.status.success(), "{}", text(&second.stderr));
    assert_eq!(runs(&e), 1, "a hit must not run the command again");
    // Output restored, log replayed to the right streams, and the hit says so on stderr.
    assert_eq!(
        fs::read_to_string(b.join("out/result.txt")).unwrap(),
        "same"
    );
    assert!(text(&second.stdout).contains("to-stdout"));
    assert!(!text(&second.stdout).contains("to-stderr"));
    assert!(text(&second.stderr).contains("to-stderr"));
    assert!(
        text(&second.stderr).contains("atlas cache: hit"),
        "{}",
        text(&second.stderr)
    );
}

#[test]
fn an_uncommitted_edit_misses_and_runs_again() {
    let e = env();
    let (a, b) = (worktree(&e, "a", "one"), worktree(&e, "b", "two"));
    assert!(cached(&e, &a, &job(&e), &[]).status.success());
    assert!(cached(&e, &b, &job(&e), &[]).status.success());
    assert_eq!(runs(&e), 2, "different bytes never share a result");
    assert_eq!(fs::read_to_string(b.join("out/result.txt")).unwrap(), "two");
}

#[test]
fn a_failing_command_passes_its_exit_code_and_is_never_a_hit() {
    let e = env();
    let a = worktree(&e, "a", "x");
    let script = format!(
        "echo x >> {}; echo boom >&2; exit 3",
        e.root.join("runs").display()
    );
    let first = cached(&e, &a, &script, &[]);
    assert_eq!(first.status.code(), Some(3));
    let second = cached(&e, &a, &script, &[]);
    assert_eq!(second.status.code(), Some(3));
    assert_eq!(runs(&e), 2, "a failure is not cached by default");
    // With --cache-failures it is, and the hit replays the exit code.
    let kept = cached(&e, &a, &script, &["--cache-failures"]);
    assert_eq!(kept.status.code(), Some(3));
    let again = cached(&e, &a, &script, &["--cache-failures"]);
    assert_eq!(again.status.code(), Some(3));
    assert_eq!(runs(&e), 3);
}

#[test]
fn a_declared_env_value_is_part_of_the_key() {
    let e = env();
    let a = worktree(&e, "a", "x");
    let run_with = |v: &str| {
        Command::new(BIN)
            .current_dir(&a)
            .env("ATLAS_CACHE_DIR", &e.store)
            .env("HOME", &e.root)
            .env("MODE", v)
            .stdin(Stdio::null())
            .args(["run", "--cache", "--input", "src/**", "--env", "MODE", "--"])
            .args([
                "sh",
                "-c",
                &format!("echo x >> {}", e.root.join("runs").display()),
            ])
            .output()
            .unwrap()
    };
    assert!(run_with("a").status.success());
    assert!(run_with("a").status.success());
    assert!(run_with("b").status.success());
    assert_eq!(runs(&e), 2, "same value hits, another value misses");
}

#[test]
fn status_gc_and_clear_report_and_empty_the_store() {
    let e = env();
    let a = worktree(&e, "a", "x");
    assert!(cached(&e, &a, &job(&e), &[]).status.success());
    assert!(cached(&e, &a, &job(&e), &[]).status.success());

    let st = atlas_cache(&e, &["status", "--json"]);
    assert!(st.status.success(), "{}", text(&st.stderr));
    let v: Value = serde_json::from_slice(&st.stdout).unwrap();
    assert_eq!(v["entries"], 1);
    assert_eq!(v["stats"]["total"]["hits"], 1);
    assert_eq!(v["stats"]["total"]["misses"], 1);
    assert!(v["bytes"].as_u64().unwrap() > 0);

    let human = atlas_cache(&e, &["status"]);
    assert!(
        text(&human.stdout).contains("1 entr"),
        "{}",
        text(&human.stdout)
    );

    assert!(atlas_cache(&e, &["gc"]).status.success());
    assert!(atlas_cache(&e, &["clear"]).status.success());
    let after: Value =
        serde_json::from_slice(&atlas_cache(&e, &["status", "--json"]).stdout).unwrap();
    assert_eq!(after["entries"], 0);
    assert_eq!(after["bytes"], 0);
}

// Two agents running the same cacheable command at the same instant: both succeed, one entry.
#[test]
fn two_processes_caching_the_same_key_leave_one_valid_entry() {
    let e = env();
    let (a, b) = (worktree(&e, "a", "same"), worktree(&e, "b", "same"));
    let script = job(&e);
    let spawn = |cwd: &Path| {
        Command::new(BIN)
            .current_dir(cwd)
            .env("ATLAS_CACHE_DIR", &e.store)
            .env("HOME", &e.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .args([
                "run", "--cache", "--input", "src/**", "--output", "out", "--", "sh", "-c", &script,
            ])
            .spawn()
            .unwrap()
    };
    let (mut p, mut q) = (spawn(&a), spawn(&b));
    assert!(p.wait().unwrap().success() && q.wait().unwrap().success());
    let v: Value = serde_json::from_slice(&atlas_cache(&e, &["status", "--json"]).stdout).unwrap();
    assert_eq!(v["entries"], 1);
    let hit = cached(&e, &a, &job(&e), &[]);
    assert!(text(&hit.stderr).contains("atlas cache: hit"));
}

#[test]
fn run_without_heavy_or_cache_is_refused() {
    let out = Command::new(BIN)
        .args(["run", "--", "true"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

// A bad output path must stop the run before it executes anything.
#[test]
fn an_output_outside_the_project_is_refused_before_the_command_runs() {
    let e = env();
    let a = worktree(&e, "a", "x");
    let out = cached(&e, &a, &job(&e), &["--output", "../escape"]);
    assert!(!out.status.success());
    assert_eq!(runs(&e), 0);
}
