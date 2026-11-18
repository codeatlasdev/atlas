//! The `ui_gate` tool through a real hostd, the way the agent's MCP client calls it: the MCP
//! dispatch, the hook route, the capture runner (a fake script), the task cache, the regression file
//! in the crewmate's work folder.

use std::path::Path;

use serde_json::{Value, json};

use crate::crew::capture::tests::{fake_runner, full_report_with_findings};
use crate::crew::capture::{self, TEST_RUNNER};
use crate::crew::uigate::TEST_CACHE;

fn use_runner(prog: &Path) {
    *TEST_RUNNER.lock() = Some((prog.to_path_buf(), atlas_core::scaling::HANG));
}

async fn gate(dir: &Path, id: &str, pane: &str, args: Value) -> Result<Value, String> {
    let v = crate::crew::mcp::call_for_test(dir, id, pane, "ui_gate", args).await?;
    Ok(serde_json::from_str(v["content"][0]["text"].as_str().unwrap()).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ui_gate_round_trip_plans_captures_hits_the_cache_and_replays_a_regression() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    // The runner counts its own invocations in a file: a hit must not start it.
    let counter = tmp.path().join("runs");
    // The gate asks the runner for the detectors' findings: this one finds nothing.
    let body = format!(
        "echo x >> {}\n{}",
        counter.display(),
        full_report_with_findings(&[], "[]")
    );
    use_runner(&fake_runner(tmp.path(), &body));
    *TEST_CACHE.lock() = Some(tmp.path().join("cache"));
    let runs = || {
        std::fs::read_to_string(&counter)
            .map(|s| s.lines().count())
            .unwrap_or(0)
    };

    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "Gate").await;
    let pane = crate::crew::pane::key(&id, &super::fixture::log_task(&dir, &id, "task9"));
    let work = crate::crew::project::work_dir(&dir, &id);
    std::fs::create_dir_all(work.join("src")).unwrap();
    std::fs::write(work.join("src/docs.svelte"), "v1").unwrap();
    std::fs::write(
        work.join("atlas.yaml"),
        "name: shop\nservices:\n  web:\n    command: echo web\n    port: 5173\n\
         affected:\n  rules:\n    - match: ['src/docs.svelte']\n      routes: ['/docs']\n",
    )
    .unwrap();
    let run =
        json!({"action": "run", "base_url": "http://localhost:5173", "files": ["src/docs.svelte"]});

    // Plan + capture: one scenario, 8 shots kept under the task.
    let v = gate(&dir, &id, &pane, run.clone()).await.unwrap();
    assert_eq!(v["passed"], true, "{v}");
    assert_eq!(v["scenarios"].as_array().unwrap().len(), 1);
    assert_eq!(v["scenarios"][0]["captures"].as_array().unwrap().len(), 8);
    assert_eq!(runs(), 1);
    // The gate's own folder (its shots and verdicts), not the task's attachments.
    let kept = dir.join("crew").join(&id).join("ui-gate").join("task9");
    for n in v["scenarios"][0]["captures"].as_array().unwrap() {
        assert!(kept.join(n.as_str().unwrap()).is_file(), "{n}");
    }
    assert!(
        kept.join("ui-gate.json").is_file(),
        "the verdicts are kept on the task"
    );

    // Unchanged: a cache hit, the runner is not started.
    let v = gate(&dir, &id, &pane, run.clone()).await.unwrap();
    assert_eq!(v["scenarios"][0]["source"], "cached", "{v}");
    assert_eq!(runs(), 1, "a hit starts nothing");

    // add_regression writes the file the crewmate commits, and the next plan includes it.
    let v = gate(&dir, &id, &pane, json!({"action": "add_regression", "url": "http://localhost:5173/pricing", "note": "clipped"}))
        .await
        .unwrap();
    assert_eq!(v["file"], ".atlas/ui-regressions.json");
    assert!(work.join(".atlas/ui-regressions.json").is_file());
    let v = gate(&dir, &id, &pane, run).await.unwrap();
    let mut u: Vec<&str> = v["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["url"].as_str().unwrap())
        .collect();
    u.sort();
    assert_eq!(
        u,
        [
            "http://localhost:5173/docs",
            "http://localhost:5173/pricing"
        ],
        "{v}"
    );
    // The registry is a project file, so adding to it changes the bytes docs was keyed on: both
    // scenarios are captured (1 run before + 2 now). The next run, unchanged, hits for both.
    assert_eq!(runs(), 3);
    let v = gate(
        &dir,
        &id,
        &pane,
        json!({"action": "run", "base_url": "http://localhost:5173", "files": ["src/docs.svelte"]}),
    )
    .await
    .unwrap();
    assert!(
        v["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["source"] == "cached"),
        "{v}"
    );
    assert_eq!(runs(), 3, "two hits start nothing");

    // Errors come back as tool errors the agent reads.
    let e = gate(&dir, &id, &pane, json!({"action": "run"}))
        .await
        .unwrap_err();
    assert!(e.contains("base_url"), "{e}");
    let e = gate(&dir, &id, "", json!({"action": "rerun_failed"}))
        .await
        .unwrap_err();
    assert!(e.contains("task"), "{e}");

    // A malformed registry: a clear error through the whole road.
    std::fs::write(work.join(".atlas/ui-regressions.json"), "{oops").unwrap();
    let e = gate(
        &dir,
        &id,
        &pane,
        json!({"action": "run", "base_url": "http://x", "files": []}),
    )
    .await
    .unwrap_err();
    assert!(e.contains("ui-regressions.json"), "{e}");

    *TEST_RUNNER.lock() = None;
    *TEST_CACHE.lock() = None;
}

// An error finding of the detectors fails the scenario through the whole stack: the hook route, the
// runner's report, the findings file in the scratch dir, the check, the verdict the agent reads.
// Fixing the page (a runner that finds nothing) and asking for `rerun_failed` turns it green.
#[tokio::test(flavor = "multi_thread")]
async fn a_detector_error_fails_the_scenario_through_hostd_and_a_fixed_page_passes_the_rerun() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let band = r#"[{"detector":"empty-band","severity":"error","message":"554 px of the page hold no content","rect":{"x":0,"y":81,"width":390,"height":554}},{"detector":"cls","severity":"warn","message":"layout shifted 0.04"}]"#;
    use_runner(&fake_runner(
        tmp.path(),
        &full_report_with_findings(&[], band),
    ));
    *TEST_CACHE.lock() = Some(tmp.path().join("cache"));

    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "GateDet").await;
    let pane = crate::crew::pane::key(&id, &super::fixture::log_task(&dir, &id, "task10"));
    let work = crate::crew::project::work_dir(&dir, &id);
    std::fs::create_dir_all(work.join("src")).unwrap();
    std::fs::write(work.join("src/docs.svelte"), "v1").unwrap();
    std::fs::write(
        work.join("atlas.yaml"),
        "name: shop\nservices:\n  web:\n    command: echo web\n    port: 5173\n\
         affected:\n  rules:\n    - match: ['src/docs.svelte']\n      routes: ['/docs']\n",
    )
    .unwrap();

    let v = gate(
        &dir,
        &id,
        &pane,
        json!({"action": "run", "base_url": "http://localhost:5173", "files": ["src/docs.svelte"]}),
    )
    .await
    .unwrap();
    assert_eq!(v["passed"], false, "{v}");
    let problems = v["scenarios"][0]["problems"].as_array().unwrap();
    assert_eq!(
        problems.len(),
        8,
        "one problem per cell, the warning is not one: {v}"
    );
    assert!(
        problems[0]
            .as_str()
            .unwrap()
            .contains("empty-band: 554 px of the page hold no content"),
        "{v}"
    );

    // The page is fixed: the same runner now finds nothing.
    use_runner(&fake_runner(
        tmp.path(),
        &full_report_with_findings(&[], "[]"),
    ));
    let v = gate(&dir, &id, &pane, json!({"action": "rerun_failed"}))
        .await
        .unwrap();
    assert_eq!(v["passed"], true, "{v}");
    *TEST_CACHE.lock() = None;
    *TEST_RUNNER.lock() = None;
}

/// Review 6, N3: the task part of a crewmate's pane key names a folder hostd writes under (as the crew
/// user, which reaches every crewmate). A crewmate must not be able to name a path of its own choosing
/// there, nor a task it does not have.
#[tokio::test(flavor = "multi_thread")]
async fn a_pane_key_naming_a_path_or_a_task_that_does_not_exist_writes_nothing() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    use_runner(&fake_runner(
        tmp.path(),
        &full_report_with_findings(&[], "[]"),
    ));
    *TEST_CACHE.lock() = Some(tmp.path().join("cache"));

    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "Evil").await;
    let victim = tmp.path().join("victim");
    let work = crate::crew::project::work_dir(&dir, &id);
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("atlas.yaml"), "name: shop\n").unwrap();
    let args = json!({"action": "run", "base_url": "http://localhost:5173", "routes": ["/"]});

    let ids = [
        victim.display().to_string(),
        format!(
            "../../../../{}",
            victim.strip_prefix("/").unwrap().display()
        ),
        "..".to_string(),
        "a/b".to_string(),
        "ghost".to_string(),
    ];
    for task in ids {
        let pane = crate::crew::pane::key(&id, &task);
        for tool in ["ui_gate", "capture"] {
            let a = if tool == "capture" {
                json!({"url": "http://localhost:5173/"})
            } else {
                args.clone()
            };
            let r = crate::crew::mcp::call_for_test(&dir, &id, &pane, tool, a).await;
            assert!(
                r.is_err(),
                "{tool} with task {task:?} must be refused: {r:?}"
            );
        }
    }
    assert!(
        !victim.exists(),
        "hostd wrote outside the crewmate's own task folders"
    );
    let gate = dir.join("crew").join(&id).join("ui-gate");
    assert!(
        !gate.exists(),
        "an unknown task got a gate folder: {gate:?}"
    );
    *TEST_CACHE.lock() = None;
    *TEST_RUNNER.lock() = None;
}
