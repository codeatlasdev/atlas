//! A task that changes a UI must show it (#210, #212): a `ui` task only reaches `done` with at least two
//! images the agent attached (captures), else it parks on a `prove_ui` card the owner answers `override` or
//! `retry` (with new images, if they want). Against a real hostd and a fake agent; the card's images, and
//! the Telegram media group in front of the buttons, are what the clients show.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::attachments::{daemon_with_telegram, fake_telegram, wait_for};
use super::fixture::{Hooks, create_crewmate, daemon, fake_agent, lock_agent_bin, use_agent_bin};
use super::fixture::wait_task_state;
use crate::HostClient;
use crate::crew::capture::tests::{fake_runner, full_report};
use crate::crew::capture::{self, TEST_RUNNER};

/// A real PNG signature plus filler: `attachments::sniff` reads the first bytes.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01";
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    B64.encode(bytes)
}

async fn send(client: &HostClient, id: &str, prompt: &str, ui: bool) -> String {
    client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": prompt, "source": "test", "ui": ui}),
        )
        .await
        .unwrap()["task"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn task_of(client: &HostClient, id: &str, task: &str) -> Value {
    client
        .call("crewmate_tasks", json!({"id": id, "limit": 20}))
        .await
        .unwrap()["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task)
        .unwrap()
        .clone()
}

/// `POST /crew/<id>/<route>` to the hook server of the hostd in `dir`, as the agent's MCP server does.
async fn post_hook(dir: &Path, id: &str, route: &str, body: Value) -> (String, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let env = std::fs::read_to_string(crate::paths::hook_endpoint(dir)).unwrap();
    let get = |k: &str| {
        env.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let body = body.to_string();
    let mut s = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", get("ATLAS_HOOK_PORT")))
        .await
        .unwrap();
    let req = format!(
        "POST /crew/{id}/{route} HTTP/1.1\r\nContent-Length: {}\r\nX-Atlas-Hook-Token: {}\r\n\r\n{body}",
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

/// A fake agent that makes one `capture` call (the hook route the MCP tool posts to) per entry of `calls`,
/// then says it is done with `reply`. The crewmate id comes from the pane key, as a pty agent has no other
/// way to know it (`crew:<crewmate id>:<task id>`). The fake runner's PNGs are what is attached.
fn capture_calls_agent(root: &Path, calls: &[Value], reply: &str) -> std::path::PathBuf {
    let mut sh = String::new();
    for args in calls {
        // The payload is single-quoted in the shell; the pane key is spliced in by closing the quote.
        let body = json!({"pane_key": "@PANE@", "args": args})
            .to_string()
            .replace('\'', "'\\''")
            .replace("@PANE@", "'\"$ATLAS_PANE_KEY\"'");
        sh.push_str(&format!(
            "curl -s -X POST \"http://127.0.0.1:${{ATLAS_HOOK_PORT}}/crew/$(echo $ATLAS_PANE_KEY | cut -d: -f2)/capture\" \\\n  \
             -H \"X-Atlas-Hook-Token: ${{ATLAS_HOOK_TOKEN}}\" -d '{body}' >/dev/null || true\n"
        ));
    }
    let hooks = Hooks::new().done(reply);
    fake_agent(
        root,
        "claude",
        &format!(
            "if [ -f \"$ATLAS_HOOK_ENDPOINT\" ]; then\n. \"$ATLAS_HOOK_ENDPOINT\"\n{sh}fi\n{}",
            hooks.script()
        ),
    )
}

fn use_runner(prog: &Path) {
    *TEST_RUNNER.lock() = Some((prog.to_path_buf(), Duration::from_secs(20)));
}

fn clear_runner() {
    *TEST_RUNNER.lock() = None;
}

// ── the gate ─────────────────────────────────────────────────────────────────────────────────

// The crux of #210: the agent says it is done on a UI task and attached nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_ui_task_finishing_without_captures_parks_as_prove_ui_with_a_clear_reason() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Restyled the header.").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Ui").await;
    let task = send(&client, &id, "restyle the header", true).await;

    let parked = wait_task_state(&client, &id, &task, "waiting_action", Duration::from_secs(30)).await;
    assert_eq!(parked["action"]["kind"], "prove_ui", "{parked}");
    let reason = parked["action"]["reason"].as_str().unwrap();
    assert!(
        reason.contains("0 of 2") && reason.contains("capture"),
        "the reason says how many captures there are and what is needed: {reason}"
    );
    assert_eq!(parked["ui"], true);
    // What the agent said stays above the card.
    assert_eq!(parked["result"], "Restyled the header.");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// One capture is not "before and after": the edge just under the limit parks, with the count in the reason.
#[tokio::test(flavor = "multi_thread")]
async fn one_agent_image_is_not_enough_and_the_reason_counts_it() {
    let _bin = lock_agent_bin().await;
    let _cap = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    // A runner that yields exactly one shot.
    let report = "printf '\\211PNG\\r\\n\\032\\nx' > \"$OUT/chromium-390-light.png\"\n\
                  printf '{\"shots\":[{\"engine\":\"chromium\",\"width\":390,\"theme\":\"light\",\"path\":\"'\"$OUT\"'/chromium-390-light.png\"}]}'";
    use_runner(&fake_runner(tmp.path(), report));
    let dir = tmp.path().join("hostd");
    use_agent_bin(&capturing_one_shot_agent(tmp.path()));
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "OneShot").await;
    let task = send(&client, &id, "restyle", true).await;
    let parked = wait_task_state(&client, &id, &task, "waiting_action", Duration::from_secs(30)).await;
    clear_runner();
    assert_eq!(parked["action"]["kind"], "prove_ui", "{parked}");
    assert!(
        parked["action"]["reason"].as_str().unwrap().contains("1 of 2"),
        "{parked}"
    );
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

/// An agent that captures once with `engines: [chromium], widths: [390], themes: [light]` (one shot).
fn capturing_one_shot_agent(root: &Path) -> std::path::PathBuf {
    capture_calls_agent(
        root,
        &[json!({"url": "http://localhost:3000/", "engines": ["chromium"], "widths": [390], "themes": ["light"]})],
        "Restyled it.",
    )
}

// With the captures the task is done, and they are on it as the agent's images (what the cards show).
#[tokio::test(flavor = "multi_thread")]
async fn a_ui_task_with_two_agent_captures_is_done() {
    let _bin = lock_agent_bin().await;
    let _cap = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    use_runner(&fake_runner(tmp.path(), &full_report(&[])));
    use_agent_bin(&capture_calls_agent(
        tmp.path(),
        &[json!({"url": "http://localhost:3000/"})],
        "Restyled and checked.",
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Proves").await;
    let task = send(&client, &id, "restyle the header", true).await;
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(30)).await;
    clear_runner();
    let atts = done["attachments"].as_array().unwrap();
    assert!(atts.len() >= 2, "{done}");
    assert!(atts.iter().all(|a| a["source"] == "agent"), "{atts:?}");
    assert!(done["action"].is_null(), "{done}");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// Not a ui task: nothing changes (the rule applies only when someone said so).
#[tokio::test(flavor = "multi_thread")]
async fn a_task_that_is_not_ui_is_unaffected() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Renamed a variable.").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Plain").await;
    let task = send(&client, &id, "rename x", false).await;
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(30)).await;
    assert!(done["action"].is_null() && done["ui"].is_null(), "{done}");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// Images the USER sent do not count as proof: only the agent's own captures.
#[tokio::test(flavor = "multi_thread")]
async fn images_the_user_sent_do_not_count_as_proof() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Done.").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "UserImg").await;
    let task = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "match this mock", "source": "test", "ui": true,
                   "attachments": [
                       {"name": "a.png", "mime": "image/png", "data_b64": b64(PNG)},
                       {"name": "b.png", "mime": "image/png", "data_b64": b64(PNG)},
                   ]}),
        )
        .await
        .unwrap()["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let parked = wait_task_state(&client, &id, &task, "waiting_action", Duration::from_secs(30)).await;
    assert_eq!(parked["action"]["kind"], "prove_ui", "{parked}");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// ── how a task becomes ui ────────────────────────────────────────────────────────────────────

// The agent marks its own task through the MCP tool's hook route; the gate then applies to it.
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_can_mark_its_task_ui_and_the_gate_then_applies() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    // Marks the task ui through the hook route (what `set_ui` posts), then says it is done.
    let sh = "curl -s -X POST \"http://127.0.0.1:${ATLAS_HOOK_PORT}/crew/$(echo ${ATLAS_PANE_KEY} | cut -d: -f2)/set_ui\" \\\n  \
              -H \"X-Atlas-Hook-Token: ${ATLAS_HOOK_TOKEN}\" \\\n  \
              -d \"{\\\"pane_key\\\":\\\"${ATLAS_PANE_KEY}\\\"}\" >/dev/null || true\n";
    let hooks = Hooks::new().done("Changed the layout.");
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &format!(
            "if [ -f \"$ATLAS_HOOK_ENDPOINT\" ]; then\n. \"$ATLAS_HOOK_ENDPOINT\"\n{sh}fi\n{}",
            hooks.script()
        ),
    ));
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "SelfMark").await;
    let task = send(&client, &id, "tweak the layout", false).await;
    let parked = wait_task_state(&client, &id, &task, "waiting_action", Duration::from_secs(30)).await;
    assert_eq!(parked["action"]["kind"], "prove_ui", "{parked}");
    assert_eq!(parked["ui"], true);
    // A pane that is not this crewmate's cannot mark anything.
    let (status, _) = post_hook(&dir, &id, "set_ui", json!({"pane_key": "crew:other:t1"})).await;
    assert!(status.contains("422"), "{status}");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// ── the answers: the same override / retry as a refusal ───────────────────────────────────────

async fn parked_ui_task(client: &HostClient, name: &str) -> (String, String) {
    let id = create_crewmate(client, name).await;
    let task = send(client, &id, "restyle the header", true).await;
    wait_task_state(client, &id, &task, "waiting_action", Duration::from_secs(30)).await;
    (id, task)
}

#[tokio::test(flavor = "multi_thread")]
async fn override_accepts_the_work_and_a_wrong_answer_is_an_error_not_a_failed_task() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Restyled the header.").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let (id, task) = parked_ui_task(&client, "Override").await;

    let bad = client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "yes"}),
        )
        .await;
    assert!(bad.is_err(), "{bad:?}");
    assert_eq!(task_of(&client, &id, &task).await["state"], "waiting_action");

    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "override"}),
        )
        .await
        .unwrap();
    let done = wait_task_state(&client, &id, &task, "done", Duration::from_secs(30)).await;
    assert_eq!(done["result"], "Restyled the header.");
    assert!(done["action"].is_null(), "{done}");
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_runs_the_task_again_and_asks_for_captures_in_its_prompt() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Restyled the header.").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let (id, task) = parked_ui_task(&client, "Retry").await;
    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "retry"}),
        )
        .await
        .unwrap();
    // The same task runs again (the fake agent still attaches nothing, so it parks again), with the reason
    // and the instruction to capture in its prompt.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let t = task_of(&client, &id, &task).await;
        let prompt = t["prompt"].as_str().unwrap();
        if t["state"] == "waiting_action" && prompt.contains("could not prove") {
            assert!(prompt.starts_with("restyle the header"), "{prompt}");
            assert!(prompt.contains("capture"), "{prompt}");
            break;
        }
        assert!(std::time::Instant::now() < deadline, "never re-ran: {t}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// #212: the human answers with a new marked screenshot; it becomes part of the retry (staged for the agent,
// listed in its prompt, and shown on the task as the user's image).
#[tokio::test(flavor = "multi_thread")]
async fn a_retry_with_images_carries_them_into_the_next_run() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Restyled the header.").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let (id, task) = parked_ui_task(&client, "RetryImg").await;

    client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "retry",
                   "attachments": [{"name": "marked.png", "mime": "image/png", "data_b64": b64(PNG)}]}),
        )
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let t = task_of(&client, &id, &task).await;
        let atts = t["attachments"].as_array().cloned().unwrap_or_default();
        if t["state"] == "waiting_action" && atts.iter().any(|a| a["name"] == "marked.png") {
            let marked = atts.iter().find(|a| a["name"] == "marked.png").unwrap();
            assert!(marked["source"].is_null(), "a user image has no source: {marked}");
            // The agent was told about it on the second run: the prompt block names the staged file.
            break;
        }
        assert!(std::time::Instant::now() < deadline, "the image never landed: {t}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    // The bytes are fetchable like any attachment.
    let piece = client
        .call(
            "crewmate_attachment",
            json!({"id": id, "task": task, "name": "marked.png", "offset": 0}),
        )
        .await
        .unwrap();
    assert_eq!(piece["last"], true);
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// Images with an answer that is not a retry are refused (an override has nothing to carry them), and a
// bad image refuses the whole answer without answering.
#[tokio::test(flavor = "multi_thread")]
async fn images_only_ride_on_a_retry_and_a_bad_image_refuses_the_answer() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Done.").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let (id, task) = parked_ui_task(&client, "Imgs").await;
    let good = json!([{"name": "m.png", "mime": "image/png", "data_b64": b64(PNG)}]);
    let on_override = client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "override", "attachments": good}),
        )
        .await;
    assert!(on_override.is_err(), "{on_override:?}");
    let not_an_image = client
        .call(
            "crewmate_action_answer",
            json!({"id": id, "task": task, "value": "retry",
                   "attachments": [{"name": "x.png", "mime": "image/png", "data_b64": b64(b"MZ\x90\0")}]}),
        )
        .await;
    assert!(not_an_image.is_err(), "{not_an_image:?}");
    assert_eq!(
        task_of(&client, &id, &task).await["state"],
        "waiting_action",
        "a refused answer leaves the card waiting"
    );
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// ── captures are the agent's attachments ──────────────────────────────────────────────────────

// #250's `attach_captures` now stores through `crew::attachments` as source "agent": the files are the
// task's records, fetchable with `crewmate_attachment`, not a side folder.
#[tokio::test(flavor = "multi_thread")]
async fn captures_land_as_agent_attachments_the_thread_can_fetch() {
    let _cap = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    use_runner(&fake_runner(tmp.path(), &full_report(&[])));
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Caps").await;
    // A task must exist for a capture to attach to: a long-running one.
    let _bin = lock_agent_bin().await;
    use_agent_bin(&fake_agent(tmp.path(), "claude", "sleep 20"));
    let task = send(&client, &id, "look", false).await;
    wait_task_state(&client, &id, &task, "running", Duration::from_secs(20)).await;
    let pane = crate::crew::pane::key(&id, &task);

    let (status, v) = post_hook(
        &dir,
        &id,
        "capture",
        json!({"pane_key": pane, "args": {"url": "http://localhost:3000/"}}),
    )
    .await;
    clear_runner();
    assert!(status.contains("200"), "{status} {v}");
    let t = task_of(&client, &id, &task).await;
    let atts = t["attachments"].as_array().unwrap();
    assert_eq!(atts.len(), 8, "{t}");
    assert!(atts.iter().all(|a| a["source"] == "agent" && a["mime"] == "image/png"));
    let first = atts[0]["name"].as_str().unwrap();
    let piece = client
        .call(
            "crewmate_attachment",
            json!({"id": id, "task": task, "name": first, "offset": 0}),
        )
        .await
        .unwrap();
    assert_eq!(piece["last"], true);
    assert!(
        !dir.join("crew").join(&id).join("captures").exists(),
        "no side folder: the attachments dir is the one place"
    );
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// ── Telegram ─────────────────────────────────────────────────────────────────────────────────

// The card on Telegram: the agent's images as one media group (at most 10), then the message with the
// buttons. Against a fake Bot API that records the order of what it was asked.
#[tokio::test(flavor = "multi_thread")]
async fn telegram_gets_the_agent_images_as_a_media_group_then_the_buttons() {
    let _bin = lock_agent_bin().await;
    let _cap = capture::TEST_LOCK.lock().await;
    let tmp = tempfile::tempdir().unwrap();
    // One shot: a ui task needs two, so it parks with the one capture it has.
    let one = "printf '\\211PNG\\r\\n\\032\\nx' > \"$OUT/chromium-390-light.png\"\n\
               printf '{\"shots\":[{\"engine\":\"chromium\",\"width\":390,\"theme\":\"light\",\"path\":\"'\"$OUT\"'/chromium-390-light.png\"}]}'";
    use_runner(&fake_runner(tmp.path(), one));
    use_agent_bin(&capturing_one_shot_agent(tmp.path()));
    let (base, paths, _sent) = fake_telegram(json!([]), PNG.to_vec(), PNG.len() as u64).await;
    let client = daemon_with_telegram(&tmp.path().join("hostd"), &base).await;
    let id = create_crewmate(&client, "Tg").await;
    let task = send(&client, &id, "restyle", true).await;
    wait_task_state(&client, &id, &task, "waiting_action", Duration::from_secs(30)).await;
    clear_runner();

    let order = wait_for("the card on Telegram", || {
        let p = paths.lock().clone();
        let group = p.iter().position(|x| x.ends_with("/sendMediaGroup"))?;
        let msg = p.iter().rposition(|x| x.ends_with("/sendMessage"))?;
        Some((group, msg))
    })
    .await;
    assert!(
        order.0 < order.1,
        "the media group goes before the message with the buttons: {:?}",
        paths.lock()
    );
    assert!(
        paths.lock().iter().all(|p| p.starts_with("/bot123:abc/") || p.starts_with("/file/")),
        "{:?}",
        paths.lock()
    );
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}

// #212: the phone's "Captain's call" lists crewmates from `crewmate_list`; a task parked on a card has to be
// there with its card and the agent's captures (not the owner's own images), or the list cannot show it.
#[tokio::test(flavor = "multi_thread")]
async fn the_crewmate_list_carries_the_waiting_card_and_only_the_agents_images() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    use_agent_bin(&fake_agent(
        tmp.path(),
        "claude",
        &Hooks::new().sleep(1).done("Restyled the header.").script(),
    ));
    let client = daemon(&tmp.path().join("hostd")).await;
    let id = create_crewmate(&client, "Listed").await;
    let idle = client.call("crewmate_list", json!({})).await.unwrap();
    assert!(idle["crewmates"][0].get("waiting_task").is_none(), "{idle}");

    let task = client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "restyle", "source": "test", "ui": true,
                   "attachments": [{"name": "mock.png", "mime": "image/png", "data_b64": b64(PNG)}]}),
        )
        .await
        .unwrap()["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    wait_task_state(&client, &id, &task, "waiting_action", Duration::from_secs(30)).await;

    let list = client.call("crewmate_list", json!({})).await.unwrap();
    let w = &list["crewmates"][0]["waiting_task"];
    assert_eq!(w["id"], task.as_str(), "{list}");
    assert_eq!(w["action"]["kind"], "prove_ui");
    assert_eq!(w["result"], "Restyled the header.");
    assert_eq!(
        w["attachments"].as_array().unwrap().len(),
        0,
        "the owner's mock is not the agent's capture: {w}"
    );
    client.call("shutdown", json!({})).await.ok();
    crate::crew::runner::set_test_agent_bin(None);
}
