//! The `locate` tool through a real hostd, the way the agent's MCP server calls it: its MCP
//! `tools/call`, the hook route, the capture with `dom: true` (a fake runner prints a canned DOM
//! report), the work tree of the crewmate. The real browser run is in `crew/capture/tests.rs`.

use std::time::Duration;

use serde_json::{Value, json};

use crate::crew::capture::{self, TEST_RUNNER};
use crate::crew::locate::tests::{dom_runner, png_header, pricing_dom, sveltekit_tree};

fn use_runner(prog: &std::path::Path) {
    *TEST_RUNNER.lock() = Some((prog.to_path_buf(), atlas_core::scaling::HANG));
}

fn clear_runner() {
    *TEST_RUNNER.lock() = None;
}

// What the agent reads back: its MCP client's `tools/call` -> the MCP server's HTTP request -> the
// hook route of a real hostd -> the capture -> the work tree.
#[tokio::test(flavor = "multi_thread")]
async fn the_mcp_locate_tool_names_the_route_component_and_selector_of_a_region() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "Loc").await;
    let pane = crate::crew::pane::key(&id, "task9");
    // The default work tree of a crewmate without a project, where #209 stages attachments.
    let work = crate::crew::project::work_dir(&dir, &id);
    sveltekit_tree(&work);
    let shot = work.join(".atlas/attachments/task9/shot.png");
    std::fs::create_dir_all(shot.parent().unwrap()).unwrap();
    std::fs::write(&shot, png_header(1440, 2000)).unwrap();
    use_runner(&dom_runner(
        tmp.path(),
        &pricing_dom("http://localhost:5173/pricing", 1440),
    ));

    let ok = crate::crew::mcp::call_for_test(
        &dir,
        &id,
        &pane,
        "locate",
        json!({"image": ".atlas/attachments/task9/shot.png", "rect": [235, 615, 170, 58], "url": "http://localhost:5173/pricing"}),
    )
    .await
    .expect("a located region is a tool result");
    let v: Value = serde_json::from_str(ok["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(v["ambiguous"], false, "{v}");
    let top = &v["candidates"][0];
    assert_eq!(top["selector"], "[data-testid=\"buy-button\"]", "{v}");
    assert_eq!(top["route_file"], "src/routes/pricing/+page.svelte");
    assert_eq!(top["component_file"], "src/lib/PlanCard.svelte");

    // An image outside the work tree is an error the agent reads, never a file read.
    std::fs::write(tmp.path().join("secret.png"), png_header(1440, 10)).unwrap();
    let err = crate::crew::mcp::call_for_test(
        &dir,
        &id,
        &pane,
        "locate",
        json!({"image": "../../../../secret.png", "url": "http://localhost:5173/pricing"}),
    )
    .await
    .expect_err("an outside image must be refused");
    assert!(
        err.contains("outside your work tree") || err.contains("no image"),
        "{err}"
    );

    // Not a task pane: refused before anything is read.
    let err = crate::crew::mcp::call_for_test(
        &dir,
        &id,
        "crew:someone-else:t1",
        "locate",
        json!({"image": "x.png", "url": "http://x/"}),
    )
    .await
    .expect_err("another crewmate's pane must be refused");
    assert!(err.contains("only a crewmate's task"), "{err}");
    clear_runner();
}

// L1 of the sixth review: listing a crewmate's tree and grepping its sources is blocking work. Done on an async
// thread, one big tree holds a worker of hostd. Here hostd has ONE worker and the scan takes 1 s: a ticker running
// next to it must keep ticking.
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn scanning_a_big_tree_does_not_hold_the_async_thread() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "Slow").await;
    let task = super::fixture::log_task(&dir, &id, "task9");
    let pane = crate::crew::pane::key(&id, &task);
    let work = crate::crew::project::work_dir(&dir, &id);
    sveltekit_tree(&work);
    let shot = work.join(".atlas/attachments/task9/shot.png");
    std::fs::create_dir_all(shot.parent().unwrap()).unwrap();
    std::fs::write(&shot, png_header(1440, 2000)).unwrap();
    use_runner(&dom_runner(
        tmp.path(),
        &pricing_dom("http://localhost:5173/pricing", 1440),
    ));
    *crate::crew::locate::TEST_SCAN_PAUSE.lock() = Some(Duration::from_millis(1000));

    let ticks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let ticker = {
        let ticks = ticks.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(20)).await;
                ticks.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        })
    };
    let before = ticks.load(std::sync::atomic::Ordering::Relaxed);
    let found = crate::crew::mcp::call_for_test(
        &dir,
        &id,
        &pane,
        "locate",
        json!({"image": ".atlas/attachments/task9/shot.png", "rect": [235, 615, 170, 58], "url": "http://localhost:5173/pricing"}),
    )
    .await;
    let during = ticks.load(std::sync::atomic::Ordering::Relaxed) - before;
    ticker.abort();
    *crate::crew::locate::TEST_SCAN_PAUSE.lock() = None;
    clear_runner();
    assert!(found.is_ok(), "{found:?}");
    assert!(
        during >= 10,
        "the scan held the only async thread: the ticker ticked {during} times in a 1 s scan"
    );
}
