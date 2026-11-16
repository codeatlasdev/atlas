//! The UI gate with a fake capturer that counts its calls: what a plan picks, what a rerun
//! captures, when the cache answers instead, and what the regression registry refuses.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use atlas_core::taskcache::Cache;
use parking_lot::Mutex;
use serde_json::json;

use super::*;
use crate::crew::capture::Captured;

/// Counts calls per URL; fails the URLs in `fail`; writes `findings` (a JSON list) next to every
/// PNG of the URLs in it, the way the runner does when the spec asks for `checks`.
struct Fake {
    calls: Mutex<Vec<String>>,
    fail: Mutex<Vec<String>>,
    findings: Mutex<Vec<(String, String)>>,
    asked_checks: Mutex<Vec<bool>>,
    total: AtomicUsize,
}

impl Fake {
    fn new() -> Self {
        Self {
            calls: Mutex::new(vec![]),
            fail: Mutex::new(vec![]),
            findings: Mutex::new(vec![]),
            asked_checks: Mutex::new(vec![]),
            total: AtomicUsize::new(0),
        }
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().clone()
    }
    fn fail_on(&self, url: &str) {
        self.fail.lock().push(url.to_string());
    }
    fn heal(&self) {
        self.fail.lock().clear();
        self.findings.lock().clear();
    }
    fn find_on(&self, url: &str, findings: serde_json::Value) {
        self.findings
            .lock()
            .push((url.to_string(), findings.to_string()));
    }
    fn reset(&self) {
        self.calls.lock().clear();
    }
}

impl Capturer for Fake {
    async fn capture(&self, spec: &Spec, out_dir: &Path) -> Result<Captured, String> {
        self.calls.lock().push(spec.url.clone());
        self.total.fetch_add(1, Ordering::SeqCst);
        if self.fail.lock().contains(&spec.url) {
            return Err(format!("capture incomplete. {}: boom", spec.url));
        }
        self.asked_checks.lock().push(spec.checks);
        let planted = self
            .findings
            .lock()
            .iter()
            .find(|(u, _)| *u == spec.url)
            .map(|(_, f)| f.clone());
        let mut files = vec![];
        let mut findings = vec![];
        for c in spec.combos() {
            let p = out_dir.join(format!(
                "{}-{}-{}.png",
                c.engine.as_str(),
                c.width,
                c.theme.as_str()
            ));
            std::fs::write(&p, format!("PNG {} {c}", spec.url)).unwrap();
            // What the runner does when the spec asks for `checks`: a findings file next to the PNG.
            if spec.checks {
                let f = crate::crew::capture::detectors::findings_path(&p);
                std::fs::write(&f, planted.as_deref().unwrap_or("[]")).unwrap();
                findings.push((c, f));
            }
            files.push((c, p));
        }
        // The gate never asks for `dom`: no reports.
        Ok(Captured {
            files,
            doms: Vec::new(),
            findings,
        })
    }
}

/// A project checkout, a cache, a task dir and a scratch dir, all in one temp dir.
struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    cache: Cache,
    task_dir: PathBuf,
    scratch: PathBuf,
    fake: Fake,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/home.svelte"), "home v1").unwrap();
        std::fs::write(root.join("src/docs.svelte"), "docs v1").unwrap();
        // The routes the project declares: files -> pages.
        std::fs::write(
            root.join("atlas.yaml"),
            "name: shop\nservices:\n  web:\n    command: echo web\n    port: 5173\n\
             affected:\n  rules:\n    - match: ['src/home.svelte']\n      routes: ['/']\n    \
             - match: ['src/docs.svelte']\n      routes: ['/docs']\n",
        )
        .unwrap();
        Self {
            cache: Cache::at(tmp.path().join("cache"), 1 << 28),
            task_dir: tmp.path().join("task"),
            scratch: tmp.path().join("scratch"),
            root,
            fake: Fake::new(),
            _tmp: tmp,
        }
    }

    fn ctx(&self) -> Ctx<'_, Fake> {
        self.ctx_in(&Tree::Own)
    }

    fn ctx_in<'a>(&'a self, tree: &'a Tree) -> Ctx<'a, Fake> {
        Ctx {
            capturer: &self.fake,
            cache: &self.cache,
            checks: &[],
            root: &self.root,
            tree,
            task_dir: &self.task_dir,
            scratch: &self.scratch,
            force: false,
        }
    }

    async fn run(&self, files: &[&str]) -> Result<serde_json::Value, String> {
        dispatch(
            &self.ctx(),
            Action::from_args(&json!({
                "action": "run", "base_url": "http://localhost:5173", "files": files,
            }))?,
        )
        .await
    }

    async fn call(&self, args: serde_json::Value) -> Result<serde_json::Value, String> {
        dispatch(&self.ctx(), Action::from_args(&args)?).await
    }
}

fn urls(v: &serde_json::Value) -> Vec<String> {
    let mut u: Vec<String> = v["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["url"].as_str().unwrap().to_string())
        .collect();
    u.sort();
    u
}

#[tokio::test]
async fn the_plan_picks_only_the_routes_the_diff_touches_plus_every_regression() {
    let w = World::new();
    w.call(json!({"action": "add_regression", "url": "http://localhost:5173/pricing", "note": "cut off at 390"}))
        .await
        .unwrap();

    let v = w.run(&["src/docs.svelte"]).await.unwrap();

    assert_eq!(
        urls(&v),
        [
            "http://localhost:5173/docs",
            "http://localhost:5173/pricing"
        ],
        "docs is touched, / is not, the regression is always there: {v}"
    );
    let mut calls = w.fake.calls();
    calls.sort();
    assert_eq!(
        calls,
        [
            "http://localhost:5173/docs",
            "http://localhost:5173/pricing"
        ]
    );
    let why: Vec<_> = v["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["url"].as_str().unwrap(), s["why"].as_str().unwrap()))
        .collect();
    assert!(
        why.contains(&("http://localhost:5173/pricing", "regression")),
        "{why:?}"
    );
    assert!(
        why.contains(&("http://localhost:5173/docs", "route")),
        "{why:?}"
    );
}

#[tokio::test]
async fn a_change_that_touches_no_route_and_no_regressions_captures_nothing() {
    let w = World::new();
    let v = w.run(&["README.md"]).await.unwrap();
    assert_eq!(v["passed"], true);
    assert!(v["scenarios"].as_array().unwrap().is_empty(), "{v}");
    assert!(w.fake.calls().is_empty());
}

#[tokio::test]
async fn a_rerun_after_one_failure_captures_only_that_scenario() {
    let w = World::new();
    w.fake.fail_on("http://localhost:5173/docs");

    let v = w
        .run(&["src/home.svelte", "src/docs.svelte"])
        .await
        .unwrap();
    assert_eq!(v["passed"], false);
    assert_eq!(v["failed"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(w.fake.calls().len(), 2, "both were captured the first time");

    // Still failing: only docs is captured again, home is not touched.
    w.fake.reset();
    let v = w.call(json!({"action": "rerun_failed"})).await.unwrap();
    assert_eq!(w.fake.calls(), ["http://localhost:5173/docs"], "{v}");
    assert_eq!(v["passed"], false);

    // Fixed: the rerun captures docs once more, the gate now passes, and home kept its verdict.
    w.fake.heal();
    w.fake.reset();
    let v = w.call(json!({"action": "rerun_failed"})).await.unwrap();
    assert_eq!(w.fake.calls(), ["http://localhost:5173/docs"]);
    assert_eq!(v["passed"], true, "{v}");
    assert_eq!(
        v["scenarios"].as_array().unwrap().len(),
        2,
        "home is still in the record: {v}"
    );

    // Nothing failed any more: a rerun captures nothing.
    w.fake.reset();
    w.call(json!({"action": "rerun_failed"})).await.unwrap();
    assert!(w.fake.calls().is_empty());
}

// The rerun must not lean on the cache to avoid work: with the cache wiped between the run and the
// rerun, only the failed scenario is captured, the passed one keeps its verdict untouched.
#[tokio::test]
async fn a_rerun_captures_only_the_failed_scenario_even_when_the_cache_is_empty() {
    let w = World::new();
    w.fake.fail_on("http://localhost:5173/docs");
    w.run(&["src/home.svelte", "src/docs.svelte"])
        .await
        .unwrap();
    w.cache.clear();
    w.fake.reset();

    w.call(json!({"action": "rerun_failed"})).await.unwrap();
    assert_eq!(w.fake.calls(), ["http://localhost:5173/docs"]);
}

#[tokio::test]
async fn the_failures_survive_a_restart_because_they_are_on_the_task() {
    let w = World::new();
    w.fake.fail_on("http://localhost:5173/");
    w.run(&["src/home.svelte"]).await.unwrap();

    let saved = TaskResults::load(&w.task_dir).unwrap();
    assert_eq!(saved.failed().len(), 1);
    assert!(
        saved.scenarios.values().next().unwrap().problems[0].contains("boom"),
        "the reason is kept"
    );
}

#[tokio::test]
async fn an_unchanged_scenario_is_a_cache_hit_with_zero_capturer_calls() {
    let w = World::new();
    let first = w.run(&["src/docs.svelte"]).await.unwrap();
    assert_eq!(w.fake.calls().len(), 1);
    assert_eq!(first["scenarios"][0]["source"], "captured");

    w.fake.reset();
    let second = w.run(&["src/docs.svelte"]).await.unwrap();
    assert!(
        w.fake.calls().is_empty(),
        "same bytes, same scenario: {second}"
    );
    assert_eq!(second["scenarios"][0]["source"], "cached");
    assert_eq!(second["passed"], true);
    assert_eq!(
        second["scenarios"][0]["captures"],
        first["scenarios"][0]["captures"]
    );
}

#[tokio::test]
async fn editing_a_project_file_makes_the_scenario_miss_and_force_skips_the_cache() {
    let w = World::new();
    w.run(&["src/docs.svelte"]).await.unwrap();
    w.fake.reset();

    std::fs::write(w.root.join("src/docs.svelte"), "docs v2").unwrap();
    w.run(&["src/docs.svelte"]).await.unwrap();
    assert_eq!(w.fake.calls().len(), 1, "different bytes: captured again");

    w.fake.reset();
    w.call(json!({"action": "run", "base_url": "http://localhost:5173", "files": ["src/docs.svelte"], "force": true}))
        .await
        .unwrap();
    assert_eq!(w.fake.calls().len(), 1, "force captures even a hit");
}

#[tokio::test]
async fn a_hit_whose_screenshots_are_gone_is_a_miss() {
    let w = World::new();
    w.run(&["src/docs.svelte"]).await.unwrap();
    std::fs::remove_dir_all(&w.task_dir).unwrap();
    w.fake.reset();

    let v = w.run(&["src/docs.svelte"]).await.unwrap();
    assert_eq!(w.fake.calls().len(), 1, "{v}");
    assert_eq!(v["scenarios"][0]["source"], "captured");
}

#[tokio::test]
async fn a_failing_scenario_is_never_cached() {
    let w = World::new();
    w.fake.fail_on("http://localhost:5173/docs");
    w.run(&["src/docs.svelte"]).await.unwrap();
    w.fake.reset();
    w.run(&["src/docs.svelte"]).await.unwrap();
    assert_eq!(
        w.fake.calls().len(),
        1,
        "a failure is captured again, not replayed"
    );
}

#[tokio::test]
async fn add_regression_persists_and_the_next_plan_includes_it() {
    let w = World::new();
    let v = w
        .call(json!({"action": "add_regression", "url": "http://localhost:5173/settings", "widths": [390], "themes": ["dark"], "note": "toggle clipped"}))
        .await
        .unwrap();
    assert_eq!(v["regressions"], 1);
    assert_eq!(v["file"], ".atlas/ui-regressions.json");

    let on_disk: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(w.root.join(".atlas/ui-regressions.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        on_disk["regressions"][0]["url"],
        "http://localhost:5173/settings"
    );
    assert_eq!(on_disk["regressions"][0]["note"], "toggle clipped");

    // A change that touches nothing still replays it, with ITS matrix (1 width x 1 theme x 2 engines).
    let v = w.run(&["README.md"]).await.unwrap();
    assert_eq!(urls(&v), ["http://localhost:5173/settings"]);
    assert_eq!(
        v["scenarios"][0]["captures"].as_array().unwrap().len(),
        2,
        "{v}"
    );

    // Adding the same scenario again updates the note, it does not duplicate it.
    w.call(json!({"action": "add_regression", "url": "http://localhost:5173/settings", "widths": [390], "themes": ["dark"], "note": "new note"}))
        .await
        .unwrap();
    let all = regressions::load(&Tree::Own, &w.root).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].note, "new note");
}

#[tokio::test]
async fn a_regression_on_a_touched_route_is_planned_once() {
    let w = World::new();
    w.call(json!({"action": "add_regression", "url": "http://localhost:5173/docs"}))
        .await
        .unwrap();
    let v = w.run(&["src/docs.svelte"]).await.unwrap();
    assert_eq!(v["scenarios"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["scenarios"][0]["why"], "regression");
    assert_eq!(w.fake.calls().len(), 1);
}

#[tokio::test]
async fn a_malformed_regressions_file_is_a_clear_error_not_an_empty_registry() {
    let w = World::new();
    std::fs::create_dir_all(w.root.join(".atlas")).unwrap();
    for bad in [
        "{not json",
        r#"{"regressions": [{"url": 3}]}"#,
        r#"{"regressions": [{"url": "file:///etc/passwd"}]}"#,
        r#"{"regressions": [{"url": "http://x/", "typo_field": 1}]}"#,
    ] {
        std::fs::write(w.root.join(".atlas/ui-regressions.json"), bad).unwrap();
        let e = w.run(&["src/docs.svelte"]).await.unwrap_err();
        assert!(e.contains("ui-regressions.json"), "{bad}: {e}");
        let e = w
            .call(json!({"action": "add_regression", "url": "http://x/y"}))
            .await
            .unwrap_err();
        assert!(
            e.contains("ui-regressions.json"),
            "add must not overwrite a file it cannot read: {e}"
        );
        assert_eq!(
            std::fs::read_to_string(w.root.join(".atlas/ui-regressions.json")).unwrap(),
            bad,
            "the broken file is left for its owner to fix"
        );
    }
    assert!(
        w.fake.calls().is_empty(),
        "nothing is captured against a registry that cannot be read"
    );
}

#[tokio::test]
async fn bad_requests_are_errors_the_agent_reads() {
    let w = World::new();
    assert!(
        w.call(json!({"action": "nope"}))
            .await
            .unwrap_err()
            .contains("run, rerun_failed or add_regression")
    );
    assert!(
        w.call(json!({"action": "run"}))
            .await
            .unwrap_err()
            .contains("base_url")
    );
    assert!(
        w.call(json!({"action": "add_regression", "url": "file:///etc/passwd"}))
            .await
            .unwrap_err()
            .contains("http"),
    );
    assert!(
        w.call(json!({"action": "rerun_failed"}))
            .await
            .unwrap_err()
            .contains("nothing has run")
    );
    let e = w
        .call(json!({"action": "run", "base_url": "http://x", "routes": ["/a/*"]}))
        .await
        .unwrap_err();
    assert!(e.contains("not a page path"), "{e}");
}

/// A check that flags every capture of one engine: what a detector does.
struct Flags;
impl Check for Flags {
    fn name(&self) -> &str {
        "overflow"
    }
    fn run(&self, combo: crate::crew::capture::Combo, _png: &Path) -> Result<Vec<Finding>, String> {
        Ok(if combo.width == 390 {
            vec![Finding {
                check: "overflow".into(),
                message: "horizontal scroll".into(),
            }]
        } else {
            vec![]
        })
    }
}

#[tokio::test]
async fn a_registered_check_fails_the_scenario_and_is_part_of_the_cache_key() {
    let w = World::new();
    // No checks: passes and is cached.
    w.run(&["src/docs.svelte"]).await.unwrap();
    w.fake.reset();

    let checks: Vec<Box<dyn Check>> = vec![Box::new(Flags)];
    let ctx = Ctx {
        checks: &checks,
        ..w.ctx()
    };
    let v = dispatch(
        &ctx,
        Action::from_args(&json!({"action": "run", "base_url": "http://localhost:5173", "files": ["src/docs.svelte"]})).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        w.fake.calls().len(),
        1,
        "the pass without the check is not a pass with it"
    );
    assert_eq!(v["passed"], false);
    assert!(
        v["scenarios"][0]["problems"][0]
            .as_str()
            .unwrap()
            .contains("overflow: horizontal scroll"),
        "{v}"
    );
}

/// The gate with the real registry (the detectors), as `handle` builds it.
fn with_detectors(w: &World) -> (Vec<Box<dyn Check>>, Ctx<'_, Fake>) {
    let checks = check::registered();
    let ctx = Ctx {
        checks: &[],
        ..w.ctx()
    };
    (checks, ctx)
}

async fn run_with_detectors(w: &World, file: &str) -> serde_json::Value {
    let (checks, ctx) = with_detectors(w);
    let ctx = Ctx {
        checks: &checks,
        ..ctx
    };
    dispatch(
        &ctx,
        Action::from_args(
            &json!({"action": "run", "base_url": "http://localhost:5173", "files": [file]}),
        )
        .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn the_ui_detectors_fail_a_scenario_with_an_error_finding_and_name_what_and_where() {
    let w = World::new();
    w.fake.find_on(
        "http://localhost:5173/docs",
        json!([
            {"detector": "empty-band", "severity": "error", "message": "554 px of the page hold no content",
             "rect": {"x": 0, "y": 81, "width": 390, "height": 554}},
            {"detector": "raw-text", "severity": "error", "message": "\"user_profile\" looks like an internal identifier", "selector": "h1"},
            {"detector": "cls", "severity": "warn", "message": "layout shifted 0.04"}
        ]),
    );
    let v = run_with_detectors(&w, "src/docs.svelte").await;
    assert_eq!(v["passed"], false, "{v}");
    assert!(
        w.fake.asked_checks.lock().iter().all(|c| *c),
        "the gate asks the runner for the detectors"
    );
    let problems: Vec<String> = v["scenarios"][0]["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    assert!(
        problems
            .iter()
            .any(|p| p.contains("empty-band: 554 px of the page hold no content")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("raw-text:") && p.contains("(h1)")),
        "the selector is in the message: {problems:?}"
    );
    assert!(
        problems.iter().all(|p| !p.contains("layout shifted 0.04")),
        "a warning does not fail the gate: {problems:?}"
    );
    // 8 cells each: every cell is judged on its own.
    assert!(
        problems.iter().any(|p| p.starts_with("webkit/390px/dark:")),
        "{problems:?}"
    );
}

#[tokio::test]
async fn warnings_alone_pass_and_a_clean_page_passes_and_is_cached() {
    let w = World::new();
    w.fake.find_on(
        "http://localhost:5173/docs",
        json!([{"detector": "cls", "severity": "warn", "message": "layout shifted 0.04"}]),
    );
    let v = run_with_detectors(&w, "src/docs.svelte").await;
    assert_eq!(v["passed"], true, "{v}");
    w.fake.reset();
    let v = run_with_detectors(&w, "src/docs.svelte").await;
    assert_eq!(v["passed"], true, "{v}");
    assert!(
        w.fake.calls().is_empty(),
        "the clean pass is a cache hit: {v}"
    );
}

#[tokio::test]
async fn a_cell_without_findings_or_with_garbage_fails_the_gate_instead_of_passing_it() {
    let w = World::new();
    // The runner wrote nothing for a cell (an old runner): the check cannot run, which fails.
    let (checks, _) = with_detectors(&w);
    let tmp = tempfile::tempdir().unwrap();
    let png = tmp.path().join("chromium-390px-light.png");
    std::fs::write(&png, "PNG").unwrap();
    let combo = crate::crew::capture::Combo {
        engine: crate::crew::capture::spec::Engine::Chromium,
        width: 390,
        theme: crate::crew::capture::spec::Theme::Light,
    };
    let e = checks[0].run(combo, &png).unwrap_err();
    assert!(e.contains("wrote no"), "{e}");
    assert!(
        e.contains("predates the UI detectors") && e.contains("atlas crew browsers install"),
        "the error names the fix: {e}"
    );
    std::fs::write(
        tmp.path().join("chromium-390px-light.findings.json"),
        "{\"nope\": true}",
    )
    .unwrap();
    let e = checks[0].run(combo, &png).unwrap_err();
    assert!(e.contains("not a list of findings"), "{e}");
    std::fs::write(tmp.path().join("chromium-390px-light.findings.json"), "[]").unwrap();
    assert!(checks[0].run(combo, &png).unwrap().is_empty());
}

#[tokio::test]
async fn fixing_the_page_turns_the_failing_scenario_green_on_rerun() {
    let w = World::new();
    w.fake.find_on(
        "http://localhost:5173/docs",
        json!([{"detector": "gutter", "severity": "error", "message": "4.0 px from the left edge", "selector": "p"}]),
    );
    let v = run_with_detectors(&w, "src/docs.svelte").await;
    assert_eq!(v["passed"], false, "{v}");
    w.fake.heal();
    let (checks, ctx) = with_detectors(&w);
    let ctx = Ctx {
        checks: &checks,
        ..ctx
    };
    let v = dispatch(
        &ctx,
        Action::from_args(&json!({"action": "rerun_failed"})).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(v["passed"], true, "{v}");
}

// ── security review 5, N1: an isolated crewmate's tree is the crewmate's ─────────────────────────────────────
//
// hostd (the crew user) planned the gate from `atlas.yaml` in the crewmate's checkout: it read the file through
// links, ran its `affected.command` as itself and wrote the regression registry through links. `Tree::Isolated`
// is what `handle` uses for a crewmate that runs as its own user.

fn isolated() -> Tree {
    Tree::Isolated(crate::crew::gitrun::Git::local())
}

async fn run_in(w: &World, tree: &Tree, files: &[&str]) -> Result<serde_json::Value, String> {
    let action = Action::from_args(&json!({
        "action": "run", "base_url": "http://localhost:5173", "files": files,
    }))?;
    dispatch(&w.ctx_in(tree), action).await
}

/// An `atlas.yaml` whose plugin command touches `marker` and answers one route.
fn with_command(w: &World, marker: &Path) {
    std::fs::write(
        w.root.join("atlas.yaml"),
        format!(
            "name: shop\nservices:\n  web:\n    command: echo web\n    port: 5173\naffected:\n  command: |\n    touch {} && echo '{{\"routes\": [\"/cmd\"]}}'\n",
            marker.display()
        ),
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_affected_command_of_a_crewmates_atlas_yaml_is_never_run_by_hostd() {
    let w = World::new();
    let marker = w.root.join("ran-as-hostd");
    with_command(&w, &marker);
    // Control: in a tree that is nobody else's the command is the project's plugin and runs.
    let v = run_in(&w, &Tree::Own, &["src/home.svelte"]).await.unwrap();
    assert!(marker.exists(), "control: the plugin did not run");
    assert_eq!(urls(&v), ["http://localhost:5173/cmd"]);
    std::fs::remove_file(&marker).unwrap();

    let e = run_in(&w, &isolated(), &["src/home.svelte"])
        .await
        .unwrap_err();
    assert!(e.contains("affected.command"), "{e}");
    assert!(
        !marker.exists(),
        "hostd ran the crewmate's affected.command"
    );
    assert!(
        w.fake.calls().len() == 1,
        "nothing captured for the refused run"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn declared_rules_still_plan_an_isolated_crewmates_routes() {
    let w = World::new();
    let v = run_in(&w, &isolated(), &["src/docs.svelte"]).await.unwrap();
    assert_eq!(urls(&v), ["http://localhost:5173/docs"], "{v}");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_link_where_the_gate_reads_atlas_yaml_or_the_registry_is_refused_without_reading_through_it()
 {
    use std::os::unix::fs::symlink;
    let w = World::new();
    let outside = w._tmp.path().join("b-work");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join(".env"), "SECRET=hunter2").unwrap();

    // atlas.yaml is a link to another crewmate's file: its content would come back in the YAML error.
    std::fs::remove_file(w.root.join("atlas.yaml")).unwrap();
    symlink(outside.join(".env"), w.root.join("atlas.yaml")).unwrap();
    let e = run_in(&w, &isolated(), &["src/home.svelte"])
        .await
        .unwrap_err();
    assert!(!e.contains("hunter2"), "read through the link: {e}");
    assert!(e.contains("atlas.yaml"), "{e}");

    // The registry file is a link: neither read nor written.
    std::fs::remove_file(w.root.join("atlas.yaml")).unwrap();
    std::fs::create_dir_all(w.root.join(".atlas")).unwrap();
    symlink(
        outside.join(".env"),
        w.root.join(".atlas/ui-regressions.json"),
    )
    .unwrap();
    let e = regressions::load(&isolated(), &w.root).unwrap_err();
    assert!(
        !e.contains("hunter2") && e.contains("ui-regressions.json"),
        "{e}"
    );
    let add = |t: &Tree| {
        regressions::add(
            t,
            &w.root,
            Scenario::for_url("http://localhost:5173/x"),
            "n",
        )
    };
    assert!(add(&isolated()).is_err());
    assert_eq!(
        std::fs::read_to_string(outside.join(".env")).unwrap(),
        "SECRET=hunter2",
        "the link's target was written"
    );

    // `.atlas` itself is a link into another crewmate's tree: nothing is created there.
    std::fs::remove_dir_all(w.root.join(".atlas")).unwrap();
    symlink(&outside, w.root.join(".atlas")).unwrap();
    assert!(add(&isolated()).is_err());
    assert!(
        !outside.join("ui-regressions.json").exists(),
        "hostd wrote the registry through the `.atlas` link"
    );
    assert!(regressions::load(&isolated(), &w.root).is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_registry_round_trips_in_the_crewmates_tree() {
    let w = World::new();
    let t = isolated();
    let all = regressions::add(
        &t,
        &w.root,
        Scenario::for_url("http://localhost:5173/x"),
        "cut off",
    )
    .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(regressions::load(&t, &w.root).unwrap()[0].note, "cut off");
    // The same file an own tree reads.
    assert_eq!(regressions::load(&Tree::Own, &w.root).unwrap().len(), 1);
}

// The cache key of an isolated tree lists its files with git as the crewmate and opens each beneath the root.
#[cfg(unix)]
#[test]
fn the_cache_key_of_an_isolated_tree_never_reads_through_a_link() {
    use std::os::unix::fs::symlink;
    let w = World::new();
    let git = |args: &[&str]| {
        let o = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .current_dir(&w.root)
            .env_remove("GIT_DIR")
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    };
    let outside = w._tmp.path().join("b-work");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret"), "one").unwrap();
    symlink(&outside, w.root.join("dir")).unwrap();
    symlink(outside.join("secret"), w.root.join("leaf")).unwrap();
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    let spec = cache_spec(&w.root, "id", &[]);
    let t = isolated();
    let key = t.cache_key(&spec).unwrap();
    std::fs::write(outside.join("secret"), "two").unwrap();
    assert_eq!(
        t.cache_key(&spec).unwrap(),
        key,
        "the key hashed what a link points at"
    );
    // A file of the tree is hashed (and the tracked one that is gone is just absent).
    std::fs::write(w.root.join("src/home.svelte"), "home v2").unwrap();
    assert_ne!(t.cache_key(&spec).unwrap(), key);
    // Not a git checkout: no key, so the gate captures again instead of trusting a cache it cannot key.
    let plain = tempfile::tempdir().unwrap();
    assert!(t.cache_key(&cache_spec(plain.path(), "id", &[])).is_err());
}

// The changed files of an isolated crewmate's tree come from git run by the spawn helper as that crewmate:
// its `.git/config` names a `core.fsmonitor` that records `$USER`, and only the crewmate may ever be there.
#[cfg(unix)]
#[test]
fn the_changed_files_of_an_isolated_tree_are_found_by_git_run_as_the_crewmate() {
    use crate::crew::spawn::testing::{Rig, USER, rig};
    let rig = rig();
    let marker = rig.booby_trapped_repo();
    let hostd = tempfile::tempdir().unwrap();
    let store = crate::crew::store::CrewStore::new(hostd.path()).with_isolation(Some(rig.setup()));
    let work = rig.work();
    let git = store.git_in(&work);
    assert!(git.is_crewmate());
    let tree = Tree::Isolated(git);
    assert_eq!(tree.changed_files(&work).unwrap(), ["a.txt", "new.txt"]);
    let users = Rig::marker_users(&marker);
    assert!(!users.is_empty(), "no git command triggered core.fsmonitor");
    assert!(users.iter().all(|u| u == USER), "{users:?}");
}
