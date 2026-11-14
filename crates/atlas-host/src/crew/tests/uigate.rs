//! The `ui_gate` tool through a real hostd, the way the agent's MCP client calls it: the MCP
//! dispatch, the hook route, the capture runner (a fake script), the task cache, the regression file
//! in the crewmate's work folder.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::crew::capture::tests::{fake_runner, full_report};
use crate::crew::capture::{self, TEST_RUNNER};
use crate::crew::uigate::TEST_CACHE;

fn use_runner(prog: &Path) {
    *TEST_RUNNER.lock() = Some((prog.to_path_buf(), Duration::from_secs(20)));
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
    let body = format!("echo x >> {}\n{}", counter.display(), full_report(&[]));
    use_runner(&fake_runner(tmp.path(), &body));
    *TEST_CACHE.lock() = Some(tmp.path().join("cache"));
    let runs = || {
        std::fs::read_to_string(&counter)
            .map(|s| s.lines().count())
            .unwrap_or(0)
    };

    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "Gate").await;
    let pane = crate::crew::pane::key(&id, "task9");
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
