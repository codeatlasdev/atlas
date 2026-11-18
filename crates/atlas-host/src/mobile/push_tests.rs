//! Push for plain agent panes, end to end: a real hostd, a real paired phone over iroh, real hook
//! POSTs as `atlas hook` sends them. The relay is replaced by a channel that records what would be
//! sent (the relay wire itself is covered in `crew::push`).

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedReceiver;

use super::tests::{Phone, pair_start};
use crate::HostClient;
use crate::agents::push::{PushMsg, Sender};
use crate::crew::PushToken;

type Sent = UnboundedReceiver<(Vec<PushToken>, PushMsg)>;

/// A hostd (not a crew server: the Mac case) whose agent pushes land in the returned channel.
async fn daemon(dir: &Path) -> (HostClient, Sent) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let d = dir.to_path_buf();
    let opts = crate::server::ServeOpts {
        no_mobile_relay: true,
        mobile_loopback: true,
        agent_push: Some(Sender(std::sync::Arc::new(move |t, m| {
            let _ = tx.send((t, m));
        }))),
        ..Default::default()
    };
    tokio::spawn(async move { crate::server::serve_with(&d, opts).await });
    for _ in 0..1200 {
        if let Ok(c) = HostClient::connect_dir(dir, "test").await {
            return (c, rx);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("hostd did not start");
}

/// Pair a phone and have it register `fcm-1` itself, over its own connection (so hostd knows whose
/// token it is). Returns the node id.
async fn paired_and_registered(c: &HostClient, phone: &mut Phone, info: &Value) -> String {
    let r = phone
        .call("pair", json!({"code": info["code"], "name": "iPhone"}))
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    let r = phone
        .call(
            "push_register",
            json!({"token": "fcm-1", "platform": "fcm", "grant": "grant-1"}),
        )
        .await
        .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    c.call("mobile_devices", json!({})).await.unwrap()["devices"][0]["node_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn hook(dir: &Path, pane: &str, payload: Value) {
    super::tests::post_hook(
        dir,
        "/hook/omp",
        json!({"pane_key": pane, "payload": payload}),
    );
}

async fn hook_async(dir: &Path, pane: &str, payload: Value) {
    let (d, p) = (dir.to_path_buf(), pane.to_string());
    tokio::task::spawn_blocking(move || hook(&d, &p, payload))
        .await
        .unwrap();
}

/// What was sent within `ms`.
async fn sent_within(rx: &mut Sent, ms: u64) -> Vec<(Vec<PushToken>, PushMsg)> {
    let mut out = Vec::new();
    let end = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Some(m)) = tokio::time::timeout_at(end, rx.recv()).await {
        out.push(m);
    }
    out
}

/// What was sent once a push arrived: waits for the first one (the real event, bounded only by the hang ceiling),
/// then collects what follows within a short settle window, so a duplicate push still shows. An expected push is
/// never missed because a loaded machine delivered it late; "nothing is sent" uses [`sent_within`] (a late push
/// can only hide a regression there, never fail the test).
async fn sent_when_any(rx: &mut Sent) -> Vec<(Vec<PushToken>, PushMsg)> {
    let Ok(Some(first)) = tokio::time::timeout(atlas_core::scaling::HANG, rx.recv()).await else {
        return Vec::new();
    };
    let mut out = vec![first];
    out.extend(sent_within(rx, 700).await);
    out
}

/// Until hostd has seen the (only) phone leave: its device row says `connected: false`. The event, not a pause: a
/// loaded machine can take longer than any fixed wait to notice, and a phone still counted connected is not pushed.
async fn phone_gone(c: &HostClient) {
    let t0 = std::time::Instant::now();
    while c.call("mobile_devices", json!({})).await.unwrap()["devices"][0]["connected"] != false {
        assert!(
            t0.elapsed() < atlas_core::scaling::HANG,
            "hostd never noticed the phone leave"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// What was sent, for failure messages: kinds and token names (a `PushToken` holds a grant and has no `Debug`).
fn names(sent: &[(Vec<PushToken>, PushMsg)]) -> Vec<(String, Vec<String>)> {
    sent.iter()
        .map(|(t, m)| {
            (
                m.kind.to_string(),
                t.iter().map(|t| t.token.clone()).collect(),
            )
        })
        .collect()
}

/// A pane that works, then finishes.
async fn work_then(dir: &Path, pane: &str, last: Value) {
    hook_async(
        dir,
        pane,
        json!({"event": "before_agent_start", "prompt": "secret prompt text"}),
    )
    .await;
    hook_async(
        dir,
        pane,
        json!({"event": "message_end", "text": "secret transcript text"}),
    )
    .await;
    hook_async(dir, pane, last).await;
}

// The acceptance case: a phone that is not connected is pushed exactly once when an agent finishes,
// with kind, pane and title, and nothing of the conversation.
#[tokio::test(flavor = "multi_thread")]
async fn a_finished_agent_pushes_the_offline_phone_once_with_the_documented_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut rx) = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    let mut phone = Phone::dial(&info).await;
    let node = paired_and_registered(&c, &mut phone, &info).await;
    phone.hang_up();
    // hostd notices the hang-up before the agent finishes.
    phone_gone(&c).await;

    work_then(tmp.path(), "p1", json!({"event": "agent_end"})).await;
    // An unchanged status (the hub repeats it as a heartbeat) must not push again.
    hook_async(tmp.path(), "p1", json!({"event": "agent_end"})).await;

    let sent = sent_when_any(&mut rx).await;
    assert_eq!(sent.len(), 1, "{:?}", names(&sent));
    let (tokens, msg) = &sent[0];
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].token, "fcm-1");
    assert_eq!(tokens[0].node_id, node, "the token must be the phone's own");
    assert_eq!(
        serde_json::to_value(msg).unwrap(),
        json!({"kind": "agent_done", "pane": "p1", "title": "omp"}),
    );
    let wire = crate::crew::push::agent_body(msg).to_string();
    assert!(!wire.contains("secret"), "conversation text leaked: {wire}");
    c.call("shutdown", json!({})).await.ok();
}

// A phone that is connected right now sees the change on screen: no push for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_connected_phone_is_not_pushed() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut rx) = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    let mut phone = Phone::dial(&info).await;
    paired_and_registered(&c, &mut phone, &info).await;

    work_then(tmp.path(), "p1", json!({"event": "agent_end"})).await;
    hook_async(tmp.path(), "p2", json!({"event": "before_agent_start"})).await;
    hook_async(
        tmp.path(),
        "p2",
        json!({"event": "tool_approval_requested"}),
    )
    .await;

    assert!(sent_within(&mut rx, 1500).await.is_empty());
    // The same phone, gone, is told the next time.
    phone.hang_up();
    phone_gone(&c).await;
    work_then(tmp.path(), "p3", json!({"event": "agent_end"})).await;
    assert_eq!(sent_when_any(&mut rx).await.len(), 1);
    c.call("shutdown", json!({})).await.ok();
}

// An approval prompt is the other thing that needs the human.
#[tokio::test(flavor = "multi_thread")]
async fn an_approval_needed_pushes_once() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut rx) = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    let mut phone = Phone::dial(&info).await;
    paired_and_registered(&c, &mut phone, &info).await;
    phone.hang_up();
    phone_gone(&c).await;

    hook_async(tmp.path(), "p1", json!({"event": "before_agent_start"})).await;
    hook_async(
        tmp.path(),
        "p1",
        json!({"event": "tool_approval_requested"}),
    )
    .await;
    hook_async(
        tmp.path(),
        "p1",
        json!({"event": "tool_approval_requested"}),
    )
    .await;

    let sent = sent_when_any(&mut rx).await;
    assert_eq!(sent.len(), 1, "{:?}", names(&sent));
    assert_eq!(sent[0].1.kind, "agent_needs_you");
    assert_eq!(sent[0].1.pane, "p1");
    c.call("shutdown", json!({})).await.ok();
}

// A subagent finishing is not the agent finishing: omp runs its extension in subagents under the
// parent's pane key. Only the parent's own end pushes.
#[tokio::test(flavor = "multi_thread")]
async fn a_subagent_finishing_never_pushes() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut rx) = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    let mut phone = Phone::dial(&info).await;
    paired_and_registered(&c, &mut phone, &info).await;
    phone.hang_up();
    phone_gone(&c).await;

    let parent = "/s/abc.jsonl";
    let sub = "/s/abc/Scout.jsonl";
    hook_async(
        tmp.path(),
        "p1",
        json!({"event": "before_agent_start", "session_file": parent}),
    )
    .await;
    hook_async(
        tmp.path(),
        "p1",
        json!({"event": "before_agent_start", "session_file": sub}),
    )
    .await;
    hook_async(
        tmp.path(),
        "p1",
        json!({"event": "agent_end", "session_file": sub}),
    )
    .await;
    assert!(sent_within(&mut rx, 1200).await.is_empty());
    hook_async(
        tmp.path(),
        "p1",
        json!({"event": "agent_end", "session_file": parent}),
    )
    .await;
    assert_eq!(sent_when_any(&mut rx).await.len(), 1);
    c.call("shutdown", json!({})).await.ok();
}

// Crewmates push through the crew hub (task_done / needs_you), never as a plain agent.
#[tokio::test(flavor = "multi_thread")]
async fn a_crewmate_pane_is_not_a_plain_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut rx) = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    let mut phone = Phone::dial(&info).await;
    paired_and_registered(&c, &mut phone, &info).await;
    phone.hang_up();
    phone_gone(&c).await;

    work_then(tmp.path(), "crew:c1:t1", json!({"event": "agent_end"})).await;
    assert!(sent_within(&mut rx, 1200).await.is_empty());
    c.call("shutdown", json!({})).await.ok();
}

// Revoking a phone removes its tokens: it can never be pushed again. (The old test registered from
// the desktop client, so the token had no phone attached and the check held for any code.)
#[tokio::test(flavor = "multi_thread")]
async fn revoking_a_phone_removes_its_tokens_and_the_pushes_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, mut rx) = daemon(tmp.path()).await;
    let info = pair_start(&c).await;
    let mut phone = Phone::dial(&info).await;
    let node = paired_and_registered(&c, &mut phone, &info).await;
    // A second token that is not that phone's (a client with no phone behind it) stays.
    c.call(
        "push_register",
        json!({"token": "fcm-desktop", "platform": "fcm", "grant": "g"}),
    )
    .await
    .unwrap();
    let stored = |dir: &Path| -> Vec<Value> {
        serde_json::from_slice(&std::fs::read(dir.join("push/tokens.json")).unwrap()).unwrap()
    };
    assert_eq!(stored(tmp.path()).len(), 2);

    c.call("mobile_revoke", json!({"node_id": node}))
        .await
        .unwrap();
    let left = stored(tmp.path());
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0]["token"], "fcm-desktop");

    work_then(tmp.path(), "p1", json!({"event": "agent_end"})).await;
    let sent = sent_within(&mut rx, 1500).await;
    assert!(
        sent.iter()
            .all(|(t, _)| t.iter().all(|t| t.token != "fcm-1")),
        "the revoked phone was pushed: {:?}",
        names(&sent)
    );
    c.call("shutdown", json!({})).await.ok();
}

// A Mac that is not a crew server takes `push_register` (it used to answer "not a crew server") and
// advertises it, so the app registers only where it works.
#[tokio::test(flavor = "multi_thread")]
async fn a_plain_mac_takes_push_register_and_advertises_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (c, _rx) = daemon(tmp.path()).await;
    let info = c.call("info", json!({})).await.unwrap();
    let caps: Vec<&str> = info["caps"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        caps.contains(&"push") && !caps.contains(&"crew"),
        "{caps:?}"
    );
    c.call(
        "push_register",
        json!({"token": "t", "platform": "fcm", "grant": "g"}),
    )
    .await
    .expect("a Mac must accept push_register");
    c.call("shutdown", json!({})).await.ok();
}
