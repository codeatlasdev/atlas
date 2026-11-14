//! The `capture` tool through a real hostd, the way the agent's MCP server calls it: the hook route,
//! the files kept under the task, the errors the agent reads. The browsers are a fake runner (a shell
//! script printing a canned report); the real Playwright run is in `crew/capture/tests.rs`.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::crew::capture::tests::{fake_runner, full_report};
use crate::crew::capture::{self, TEST_RUNNER};

fn use_runner(prog: &Path, timeout: Duration) {
    *TEST_RUNNER.lock() = Some((prog.to_path_buf(), timeout));
}

fn clear_runner() {
    *TEST_RUNNER.lock() = None;
}

// ── Through a real hostd, the way the agent's MCP server calls it ──────────────────────────────

/// `POST /crew/<id>/capture` to the hook server of the hostd in `dir` (what `mcp::tool_capture`
/// sends); `(status line, body)`.
async fn post_capture(dir: &Path, id: &str, pane: &str, args: Value) -> (String, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let env = std::fs::read_to_string(crate::paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap()
            .to_string()
    };
    let body = json!({"pane_key": pane, "args": args}).to_string();
    let mut s = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", get("ATLAS_HOOK_PORT")))
        .await
        .unwrap();
    let req = format!(
        "POST /crew/{id}/capture HTTP/1.1\r\nContent-Length: {}\r\nX-Atlas-Hook-Token: {}\r\n\r\n{body}",
        body.len(),
        get("ATLAS_HOOK_TOKEN")
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut r = String::new();
    s.read_to_string(&mut r).await.unwrap();
    let status = r.lines().next().unwrap_or("").to_string();
    let body = r.split("\r\n\r\n").nth(1).unwrap_or_default();
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// Put a bare task `task_id` on the crewmate's log (a capture attaches to a task that exists) and
/// return its id.
fn log_task(dir: &Path, crewmate_id: &str, task_id: &str) -> String {
    let store = crate::crew::store::CrewStore::new(dir);
    let mut t = crate::crew::store::Task::new("test", "look");
    t.id = task_id.to_string();
    store.append_task(crewmate_id, &t).unwrap();
    task_id.to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_capture_tool_round_trip_through_a_real_hostd_attaches_every_shot() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    use_runner(
        &fake_runner(tmp.path(), &full_report(&[])),
        Duration::from_secs(20),
    );
    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "Cap").await;
    let pane = crate::crew::pane::key(&id, &log_task(&dir, &id, "task42"));

    let (status, v) =
        post_capture(&dir, &id, &pane, json!({"url": "http://localhost:3000/"})).await;
    clear_runner();

    assert!(status.contains("200"), "{status} {v}");
    assert_eq!(v["visited"], 8, "{v}");
    let shots = v["shots"].as_array().unwrap();
    assert_eq!(shots.len(), 8);
    let kept = crate::crew::attachments::task_dir(
        &crate::crew::store::CrewStore::new(&dir),
        &id,
        "task42",
    );
    for s in shots {
        let name = s["name"].as_str().unwrap();
        assert!(
            std::fs::read(kept.join(name))
                .unwrap()
                .ends_with(name.trim_end_matches(".png").as_bytes()),
            "{name} is kept with the task's attachments"
        );
    }
    assert!(
        std::fs::read_dir(dir.join("capture/runs"))
            .map(|d| d.count() == 0)
            .unwrap_or(true),
        "the scratch dir of the run is removed"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn through_hostd_a_missing_combination_and_a_bad_request_are_errors_the_agent_reads() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    use_runner(
        &fake_runner(tmp.path(), &full_report(&["webkit-1440-dark"])),
        Duration::from_secs(20),
    );
    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "Cap2").await;
    let pane = crate::crew::pane::key(&id, &log_task(&dir, &id, "t1"));

    let (status, v) =
        post_capture(&dir, &id, &pane, json!({"url": "http://localhost:3000/"})).await;
    assert!(status.contains("422"), "{status}");
    assert!(
        v["error"].as_str().unwrap().contains("webkit/1440px/dark"),
        "{v}"
    );
    assert!(
        !dir.join("crew").join(&id).join("attachments").exists(),
        "an incomplete capture attaches nothing"
    );

    let (status, v) = post_capture(&dir, &id, &pane, json!({"url": "file:///etc/passwd"})).await;
    assert!(
        status.contains("422") && v["error"].as_str().unwrap().contains("http"),
        "{status} {v}"
    );

    // Not a task of this crewmate: no pane, or another crewmate's pane.
    let (status, _) = post_capture(&dir, &id, "", json!({"url": "http://x/"})).await;
    assert!(status.contains("422"), "{status}");
    let (status, _) = post_capture(
        &dir,
        &id,
        "crew:someone-else:t1",
        json!({"url": "http://x/"}),
    )
    .await;
    assert!(status.contains("422"), "{status}");
    clear_runner();
}

// The whole path an agent takes: its MCP client's `tools/call` -> the MCP server's HTTP request ->
// the hook route of a real hostd -> the runner -> the files. What the agent reads back is the tool
// result text, or the error message when the matrix is incomplete.
#[tokio::test(flavor = "multi_thread")]
async fn the_mcp_capture_tool_returns_every_combination_visited_and_the_error_when_one_is_missing()
{
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "McpCap").await;
    let pane = crate::crew::pane::key(&id, &log_task(&dir, &id, "task7"));
    let args = json!({"url": "http://localhost:3000/", "widths": [390], "themes": ["dark"]});

    // A full report: the result text is the JSON listing the cells asked for (2 engines x 1 x 1).
    use_runner(
        &fake_runner(tmp.path(), &full_report(&[])),
        Duration::from_secs(20),
    );
    let ok = crate::crew::mcp::call_for_test(&dir, &id, &pane, "capture", args.clone())
        .await
        .expect("a complete capture is a tool result");
    let text = ok["content"][0]["text"].as_str().unwrap();
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(v["visited"], 2, "{v}");
    let names: Vec<&str> = v["shots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"chromium-390-dark.png") && names.contains(&"webkit-390-dark.png"));
    let kept =
        crate::crew::attachments::task_dir(&crate::crew::store::CrewStore::new(&dir), &id, "task7");
    assert!(kept.join("webkit-390-dark.png").is_file());

    // A missing combination: the MCP call is an error whose message names it.
    use_runner(
        &fake_runner(tmp.path(), &full_report(&["webkit-390-dark"])),
        Duration::from_secs(20),
    );
    let err = crate::crew::mcp::call_for_test(&dir, &id, &pane, "capture", args)
        .await
        .expect_err("an incomplete capture must be a tool error");
    assert!(err.contains("webkit/390px/dark"), "{err}");

    // A bad request is an error the agent reads, not a transport failure.
    let err =
        crate::crew::mcp::call_for_test(&dir, &id, &pane, "capture", json!({"url": "ftp://x/"}))
            .await
            .unwrap_err();
    assert!(err.contains("http"), "{err}");
    clear_runner();
}

/// A fake runner that also writes a DOM report per cell, the way the real one does for `dom: true`.
fn dom_report_runner(dir: &Path) -> std::path::PathBuf {
    let mut body = String::new();
    let mut entries = vec![];
    for e in ["chromium", "webkit"] {
        let key = format!("{e}-390px-dark");
        body.push_str(&format!("printf 'PNG-{key}' > \"$OUT/{key}.png\"\n"));
        body.push_str(&format!(
            "printf '{{\"elements\":[]}}' > \"$OUT/{key}.dom.json\"\n"
        ));
        entries.push(format!(
            "{{\"engine\":\"{e}\",\"width\":390,\"theme\":\"dark\",\"path\":\"'\"$OUT\"'/{key}.png\",\"dom\":\"'\"$OUT\"'/{key}.dom.json\"}}"
        ));
    }
    body.push_str(&format!("printf '{{\"shots\":[{}]}}'\n", entries.join(",")));
    fake_runner(dir, &body)
}

// `dom: true` asks the runner for the element boxes; the report lists the file per shot, and the file
// is kept next to its PNG (the scratch dir of the run is removed, so a path into it would dangle).
#[tokio::test(flavor = "multi_thread")]
async fn a_dom_capture_keeps_each_dom_file_next_to_its_png_and_lists_it() {
    let _g = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    use_runner(&dom_report_runner(tmp.path()), Duration::from_secs(20));
    let client = super::fixture::daemon(&dir).await;
    let id = super::fixture::create_crewmate(&client, "CapDom").await;
    let pane = crate::crew::pane::key(&id, "task5");

    let (status, v) = post_capture(
        &dir,
        &id,
        &pane,
        json!({"url": "http://localhost:3000/", "widths": [390], "themes": ["dark"], "dom": true}),
    )
    .await;
    assert!(status.contains("200"), "{status} {v}");
    let kept = dir.join("crew").join(&id).join("captures").join("task5");
    for s in v["shots"].as_array().unwrap() {
        let dom = s["dom"]
            .as_str()
            .unwrap_or_else(|| panic!("no dom listed: {s}"));
        assert!(dom.ends_with(".dom.json"), "{dom}");
        assert!(kept.join(dom).is_file(), "{dom} is kept under the task");
        assert!(s["dom_path"].as_str().unwrap().ends_with(dom), "{s}");
    }

    // Without `dom` the answer has no dom entries and the runner is not asked for one.
    use_runner(
        &fake_runner(tmp.path(), &full_report(&[])),
        Duration::from_secs(20),
    );
    let (_, v) = post_capture(
        &dir,
        &id,
        &pane,
        json!({"url": "http://localhost:3000/", "widths": [390], "themes": ["dark"]}),
    )
    .await;
    clear_runner();
    assert!(v["shots"][0].get("dom").is_none(), "{v}");
}
