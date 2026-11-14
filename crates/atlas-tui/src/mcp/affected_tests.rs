//! The MCP tools `atlas_affected` and `atlas_estimate` on real projects (atlas.yaml + git repo in
//! temp dirs, a temp cache and a temp heavy queue). The contract is the serialized JSON an agent
//! reads. Guards issue #128.

use std::fs;
use std::path::Path;
use std::process::Command;

use atlas_core::heavy::Dir;
use atlas_core::taskcache::{Cache, Chunk, Run, Spec, Stream};
use serde_json::{Value, json};

use super::affected::{affected_in, estimate_in};

fn write(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C"])
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A project: atlas.yaml `extra` on top of two cached actions, committed on `main`, then a
/// `feature` branch checked out.
fn project(root: &Path, extra: &str) {
    write(
        &root.join("atlas.yaml"),
        &format!(
            "name: demo\nservices:\n  api: {{ command: run }}\nactions:\n  - {{ key: c, label: Run checks, command: cargo test, cache: {{ inputs: ['crates/**'] }} }}\n  - {{ key: b, label: Site build, command: bun run build, cache: {{ inputs: ['site/**'] }} }}\n  - {{ key: p, label: Plain, command: echo hi }}\n{extra}"
        ),
    );
    write(&root.join("crates/a/src/lib.rs"), "1");
    write(&root.join("site/index.md"), "1");
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);
    git(root, &["checkout", "-q", "-b", "feature"]);
}

fn call(f: Result<String, String>) -> Value {
    serde_json::from_str(&f.unwrap()).unwrap()
}

fn strs(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect()
}

// ---- atlas_affected ----

#[test]
fn affected_falls_back_to_cache_inputs_using_the_worktrees_uncommitted_and_untracked_files() {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    project(r, "");
    write(&r.join("crates/a/src/lib.rs"), "edited"); // unstaged
    write(&r.join("site/new.md"), "untracked");
    let v = call(affected_in(r, &json!({})));
    assert_eq!(v["source"], "cache_inputs");
    assert_eq!(v["files_from"], "git");
    assert_eq!(strs(&v["files"]), ["crates/a/src/lib.rs", "site/new.md"]);
    assert_eq!(strs(&v["tasks"]), ["Run checks", "Site build"]);
    for k in ["tests", "routes", "builds"] {
        assert!(v[k].as_array().unwrap().is_empty(), "{k}: {v}");
    }
}

#[test]
fn affected_with_explicit_files_does_not_read_git() {
    let t = tempfile::tempdir().unwrap();
    project(t.path(), "");
    write(&t.path().join("site/new.md"), "untracked: must not count");
    let v = call(affected_in(
        t.path(),
        &json!({"files": ["crates/a/src/lib.rs"]}),
    ));
    assert_eq!(v["files_from"], "args");
    assert_eq!(strs(&v["tasks"]), ["Run checks"]);
    // an absolute path inside the project is accepted and made relative
    let abs = t.path().join("site/index.md");
    let v = call(affected_in(t.path(), &json!({"files": [abs]})));
    assert_eq!(strs(&v["files"]), ["site/index.md"]);
    assert_eq!(strs(&v["tasks"]), ["Site build"]);
}

#[test]
fn affected_uses_the_declared_rules_over_the_fallback() {
    let t = tempfile::tempdir().unwrap();
    project(
        t.path(),
        "affected:\n  rules:\n    - { match: ['crates/**'], tasks: [Run checks], tests: ['cargo test -p a'], routes: [/api], builds: [a] }\n",
    );
    let v = call(affected_in(
        t.path(),
        &json!({"files": ["crates/a/src/lib.rs", "site/index.md"]}),
    ));
    assert_eq!(v["source"], "rules");
    assert_eq!(
        strs(&v["tasks"]),
        ["Run checks"],
        "the site action is NOT picked: rules replace the fallback"
    );
    assert_eq!(strs(&v["tests"]), ["cargo test -p a"]);
    assert_eq!(strs(&v["routes"]), ["/api"]);
    assert_eq!(strs(&v["builds"]), ["a"]);
}

#[cfg(unix)]
#[test]
fn affected_runs_the_plugin_command_in_the_project_root_with_the_changed_files() {
    let t = tempfile::tempdir().unwrap();
    // A project script, like ui's `bun scripts/affected.ts`: reads the files on stdin.
    write(
        &t.path().join("scripts/affected.sh"),
        "n=$(wc -l | tr -d ' ')\nprintf '{\"tests\":[\"saw %s\"]}' \"$n\"\n",
    );
    project(t.path(), "affected:\n  command: sh scripts/affected.sh\n");
    let v = call(affected_in(t.path(), &json!({"files": ["a", "b", "c"]})));
    assert_eq!(v["source"], "command");
    assert_eq!(strs(&v["tests"]), ["saw 3"]);
}

#[cfg(unix)]
#[test]
fn affected_reports_a_failing_plugin_as_an_error_with_its_stderr() {
    let t = tempfile::tempdir().unwrap();
    project(
        t.path(),
        "affected:\n  command: \"echo kaboom >&2; exit 2\"\n",
    );
    let e = affected_in(t.path(), &json!({"files": ["a"]})).unwrap_err();
    assert!(e.contains("kaboom"), "{e}");
}

#[test]
fn affected_base_argument_picks_the_merge_base() {
    let t = tempfile::tempdir().unwrap();
    let r = t.path();
    project(r, "");
    write(&r.join("crates/a/src/lib.rs"), "feature");
    git(r, &["commit", "-q", "-am", "feature edit"]);
    // Against HEAD itself nothing is committed-changed and the tree is clean.
    let v = call(affected_in(r, &json!({"base": "HEAD"})));
    assert!(v["files"].as_array().unwrap().is_empty(), "{v}");
    assert!(v["tasks"].as_array().unwrap().is_empty());
    assert_eq!(v["base"], "HEAD");
    // Against main the committed edit counts.
    let v = call(affected_in(r, &json!({"base": "main"})));
    assert_eq!(strs(&v["files"]), ["crates/a/src/lib.rs"]);
    assert_eq!(strs(&v["tasks"]), ["Run checks"]);
}

#[test]
fn affected_rejects_malformed_arguments_and_unknown_bases() {
    let t = tempfile::tempdir().unwrap();
    project(t.path(), "");
    for (args, needle) in [
        (json!({"files": "a"}), "files"),
        (json!({"files": [1]}), "files"),
        (json!({"base": 3}), "base"),
        (json!({"base": "no-such-ref"}), "no-such-ref"),
        (json!({"files": ["../outside"]}), "outside"),
    ] {
        let e = affected_in(t.path(), &args).unwrap_err();
        assert!(e.contains(needle), "{args}: {e}");
    }
}

#[test]
fn affected_without_a_git_repo_and_without_files_says_to_pass_files() {
    let t = tempfile::tempdir().unwrap();
    write(
        &t.path().join("atlas.yaml"),
        "name: demo\nservices:\n  api: { command: run }\n",
    );
    let e = affected_in(t.path(), &json!({})).unwrap_err();
    assert!(e.contains("files"), "{e}");
    // with files it works (no source declared, no actions: nothing affected)
    let v = call(affected_in(t.path(), &json!({"files": ["x"]})));
    assert_eq!(v["source"], "cache_inputs");
    assert!(v["tasks"].as_array().unwrap().is_empty());
}

// ---- atlas_estimate ----

fn spec_of(root: &Path, command: &str, inputs: &[&str]) -> Spec {
    Spec {
        cwd: root.to_path_buf(),
        argv: vec![command.into()],
        inputs: inputs.iter().map(|s| (*s).to_string()).collect(),
        outputs: vec![],
        env: vec![],
        tools: vec![],
    }
}

fn record(cache: &Cache, spec: &Spec, label: &str, command: &str, ms: u64) {
    let run = Run {
        // The folder name of the project root, as `atlas run` records it.
        project: "proj".into(),
        label: label.into(),
        command: command.into(),
        exit: 0,
        duration_ms: ms,
        log: vec![Chunk {
            stream: Stream::Stdout,
            bytes: b"ok\n".to_vec(),
        }],
    };
    cache.record(&spec.key().unwrap(), spec, &run).unwrap();
}

struct Env {
    _t: tempfile::TempDir,
    root: std::path::PathBuf,
    cache: Cache,
    heavy: Dir,
}

fn env() -> Env {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("proj");
    project(&root, "");
    let heavy_dir = t.path().join("heavy");
    fs::create_dir_all(&heavy_dir).unwrap();
    let cache = Cache::at(t.path().join("store"), 64 << 20);
    Env {
        root,
        cache,
        heavy: Dir::new(heavy_dir),
        _t: t,
    }
}

#[test]
fn estimate_of_an_action_reports_duration_hit_and_queue_from_recorded_data() {
    let e = env();
    let spec = spec_of(&e.root, "cargo test", &["crates/**"]);
    record(&e.cache, &spec, "Run checks", "cargo test", 8000);
    e.heavy.record_run("Run checks", 12);

    let v = call(estimate_in(
        &e.root,
        &json!({"action": "run checks"}),
        &e.cache,
        &e.heavy,
    ));
    assert_eq!(v["name"], "Run checks");
    assert_eq!(v["command"], "cargo test");
    // the cache's recorded run wins over the queue's history of the same label
    assert_eq!(v["runs"], 1);
    assert_eq!(v["median_ms"], 8000);
    assert_eq!(v["p90_ms"], 8000);
    assert_eq!(strs(&v["sources"]), ["task cache"]);
    assert_eq!(v["would_hit"], true);
    assert_eq!(v["saved_ms"], 8000);
    assert_eq!(v["key"].as_str().unwrap().len(), 64);
    // queue: the machine's load right now, never started anything
    for k in ["slots", "running", "queued"] {
        assert!(v["queue"].get(k).is_some(), "queue.{k}: {v}");
    }
    assert_eq!(v["queue"]["running"], 0);
    assert!(v["queue"]["slots"].as_u64().unwrap() >= 1);
}

#[test]
fn estimate_flips_to_a_miss_when_the_inputs_change_and_recommends_how_to_run() {
    let e = env();
    let spec = spec_of(&e.root, "cargo test", &["crates/**"]);
    record(&e.cache, &spec, "Run checks", "cargo test", 8000);
    let args = json!({"action": "Run checks"});
    assert_eq!(
        call(estimate_in(&e.root, &args, &e.cache, &e.heavy))["would_hit"],
        true
    );

    write(
        &e.root.join("crates/a/src/lib.rs"),
        "edited after the cached run",
    );
    let v = call(estimate_in(&e.root, &args, &e.cache, &e.heavy));
    assert_eq!(v["would_hit"], false);
    assert!(v["saved_ms"].is_null());
    // history survives the miss: the agent still learns the cost of running it
    assert_eq!(v["median_ms"], 8000);
    let run = v["run_with"].as_str().unwrap();
    // The atlas path and the quoting are the platform's (`atlas.exe` and double quotes for cmd).
    let q = atlas_core::heavy::sh_quote;
    assert!(run.contains(" run --cache"), "{run}");
    assert!(
        run.contains(&format!("--input {}", q("crates/**"))),
        "{run}"
    );
    assert!(
        run.contains(&format!("--label {}", q("Run checks"))),
        "{run}"
    );
    assert!(run.ends_with(&format!("-- {}", q("cargo test"))), "{run}");
}

#[test]
fn estimate_of_an_action_without_cache_block_has_history_but_never_a_hit() {
    let e = env();
    e.heavy.record_run("Plain", 3);
    let v = call(estimate_in(
        &e.root,
        &json!({"action": "p"}),
        &e.cache,
        &e.heavy,
    )); // by key
    assert_eq!(v["name"], "Plain");
    assert_eq!(v["would_hit"], false);
    assert_eq!(v["median_ms"], 3000);
    assert!(v["key"].is_null());
    assert!(
        v["note"].as_str().unwrap().contains("cache:"),
        "tells the agent how to make it cacheable: {v}"
    );
}

#[test]
fn estimate_of_a_raw_command_uses_the_declared_inputs_like_atlas_run_cache() {
    let e = env();
    let spec = spec_of(&e.root, "cargo test -p a", &["crates/a/**"]);
    record(&e.cache, &spec, "cargo", "cargo test -p a", 5000);
    let v = call(estimate_in(
        &e.root,
        &json!({"command": "cargo test -p a", "inputs": ["crates/a/**"], "label": "cargo"}),
        &e.cache,
        &e.heavy,
    ));
    assert_eq!(v["would_hit"], true);
    assert_eq!(v["command"], "cargo test -p a");
    assert_eq!(v["median_ms"], 5000);
    // different inputs = a different run: not a hit
    let v = call(estimate_in(
        &e.root,
        &json!({"command": "cargo test -p a", "inputs": ["crates/**"], "label": "cargo"}),
        &e.cache,
        &e.heavy,
    ));
    assert_eq!(v["would_hit"], false);
}

#[test]
fn estimate_with_no_history_says_runs_zero_not_a_made_up_number() {
    let e = env();
    let v = call(estimate_in(
        &e.root,
        &json!({"command": "never-ran --x"}),
        &e.cache,
        &e.heavy,
    ));
    assert_eq!(v["runs"], 0);
    assert!(v["median_ms"].is_null() && v["p90_ms"].is_null());
    assert!(strs(&v["sources"]).is_empty());
}

#[test]
fn estimate_reports_the_heavy_queue_load_it_sees() {
    let e = env();
    let mut ticket = e
        .heavy
        .enqueue("gate", "bun run check", &e.root, "demo")
        .unwrap();
    assert!(ticket.try_start(2).unwrap());
    e.heavy.record_run("gate", 100);
    let v = call(estimate_in(
        &e.root,
        &json!({"command": "bun run check", "label": "gate"}),
        &e.cache,
        &e.heavy,
    ));
    assert_eq!(v["queue"]["running"], 1);
    assert_eq!(v["queue"]["queued"], 0);
    drop(ticket);
}

#[test]
fn estimate_rejects_bad_arguments_and_unknown_actions() {
    let e = env();
    for (args, needle) in [
        (json!({}), "action"),
        (json!({"action": "a", "command": "b"}), "either"),
        (json!({"action": "No such action"}), "No such action"),
        (json!({"command": 3}), "command"),
        (json!({"command": "x", "inputs": "a"}), "inputs"),
        (json!({"command": "x", "inputs": ["a/[**"]}), "a/[**"),
    ] {
        let err = estimate_in(&e.root, &args, &e.cache, &e.heavy).unwrap_err();
        assert!(err.contains(needle), "{args}: {err}");
    }
}

#[test]
fn estimate_never_runs_the_command_or_touches_the_counters() {
    let e = env();
    let marker = e.root.join("ran.txt");
    let cmd = format!("touch {}", marker.display());
    let before = e.cache.stats();
    let v = call(estimate_in(
        &e.root,
        &json!({"command": cmd}),
        &e.cache,
        &e.heavy,
    ));
    assert_eq!(v["would_hit"], false);
    assert!(!marker.exists(), "estimate must not execute the command");
    assert_eq!(e.cache.stats(), before);
}
