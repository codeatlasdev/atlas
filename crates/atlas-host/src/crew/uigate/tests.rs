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
        Ctx {
            capturer: &self.fake,
            cache: &self.cache,
            checks: &[],
            root: &self.root,
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
    let all = regressions::load(&w.root).unwrap();
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
