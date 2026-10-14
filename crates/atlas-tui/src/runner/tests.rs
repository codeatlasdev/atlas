use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::hub::{ACTION_RUNS, ACTION_TAIL_LINES, EVENT_HISTORY};
use super::*;
use crate::config::ProjectConfig;
use crate::control::{self, ControlServer};
use crate::control::{ControlCmd, ControlRequest};
use crate::runtime::logs::LogStream;
use crate::runtime::manager::{ManagerEvent, ServiceManager};
use crate::runtime::service::ServiceState;

fn cfg() -> ProjectConfig {
    serde_norway::from_str(
        r#"
name: demo
services:
  api: { command: "sleep 30", port: 3000 }
actions:
  - { key: m, label: Migrate, command: "true" }
  - { key: x, label: Shell, command: "sh", session: true }
"#,
    )
    .unwrap()
}

fn manager(cfg: &ProjectConfig) -> (ServiceManager, mpsc::UnboundedReceiver<ManagerEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        ServiceManager::new(std::env::temp_dir().join("atlas-hub-test"), cfg, tx),
        rx,
    )
}

fn log(name: &str, line: &str) -> ManagerEvent {
    ManagerEvent::LogLine {
        name: name.into(),
        line: line.into(),
        stream: LogStream::Stdout,
    }
}

#[test]
fn snapshot_has_tail_then_events_stream_in_order_without_gaps() {
    let c = cfg();
    let (m, _rx) = manager(&c);
    let mut hub = Hub::new("demo");
    hub.on_event(&m, &log("api", "before-1"));
    hub.on_event(&m, &log("api", "before-2"));

    let (tx, mut rx) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
    let snap = hub.subscribe(&m, tx);
    let lines: Vec<&str> = snap["logs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["line"].as_str().unwrap())
        .collect();
    assert_eq!(lines, ["before-1", "before-2"]);
    assert_eq!(snap["project"], "demo");
    assert_eq!(snap["services"][0]["name"], "api");

    hub.on_event(&m, &log("api", "after-1"));
    hub.on_event(&m, &log("api", "after-2"));
    let got: Vec<Value> = std::iter::from_fn(|| rx.try_recv().ok())
        .map(|l| serde_json::from_str(&l).unwrap())
        .collect();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0]["line"], "after-1");
    assert_eq!(got[1]["line"], "after-2");
}

#[test]
fn two_clients_each_get_every_event_and_a_dropped_one_is_pruned() {
    let c = cfg();
    let (m, _rx) = manager(&c);
    let mut hub = Hub::new("demo");
    let (t1, mut r1) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
    let (t2, r2) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
    hub.subscribe(&m, t1);
    hub.subscribe(&m, t2);
    assert_eq!(hub.clients(), 2);

    drop(r2); // client 2 disconnects
    hub.on_event(&m, &log("api", "x"));
    assert_eq!(hub.clients(), 1, "dead subscriber pruned on next broadcast");
    let first: Value = serde_json::from_str(&r1.try_recv().unwrap()).unwrap();
    assert_eq!(first["line"], "x", "survivor unaffected");
}

#[test]
fn history_is_bounded_and_replayed_to_new_clients() {
    let c = cfg();
    let (m, _rx) = manager(&c);
    let mut hub = Hub::new("demo");
    for i in 0..(EVENT_HISTORY + 10) {
        hub.on_event(
            &m,
            &ManagerEvent::ServiceNote {
                name: "api".into(),
                message: format!("n{i}"),
            },
        );
    }
    let (tx, _rx2) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
    let snap = hub.subscribe(&m, tx);
    let ev = snap["events"].as_array().unwrap();
    assert_eq!(ev.len(), EVENT_HISTORY);
    assert_eq!(
        ev.last().unwrap()["message"],
        format!("n{}", EVENT_HISTORY + 9)
    );
}

fn progress(label: &str, line: &str) -> ManagerEvent {
    ManagerEvent::ActionProgress {
        label: label.into(),
        line: line.into(),
    }
}

fn done(label: &str, ok: bool) -> ManagerEvent {
    ManagerEvent::ActionCompleted {
        label: label.into(),
        ok,
        resume: vec![],
    }
}

fn snapshot_runs(hub: &mut Hub, m: &ServiceManager) -> Vec<Value> {
    let (tx, _rx) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
    hub.subscribe(m, tx)["action_runs"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn snapshot_replays_action_runs_with_status_and_tail() {
    let c = cfg();
    let (m, _rx) = manager(&c);
    let mut hub = Hub::new("demo");
    hub.on_event(&m, &progress("Migrate", "step 1"));
    hub.on_event(&m, &progress("Migrate", "step 2"));
    hub.on_event(&m, &done("Migrate", true));
    hub.on_event(&m, &progress("Seed", "rows"));

    let runs = snapshot_runs(&mut hub, &m);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["label"], "Migrate");
    assert_eq!(runs[0]["status"], "ok");
    assert!(runs[0]["end"].is_u64());
    assert_eq!(runs[0]["lines"], json!(["step 1", "step 2"]));
    assert_eq!(runs[1]["label"], "Seed");
    assert_eq!(runs[1]["status"], "running");
    assert!(runs[1]["end"].is_null(), "no end while running");

    // A second run of the same label is a new run, not an append to the finished one.
    hub.on_event(&m, &progress("Migrate", "again"));
    hub.on_event(&m, &done("Migrate", false));
    let runs = snapshot_runs(&mut hub, &m);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[2]["status"], "failed");
    assert_eq!(runs[2]["lines"], json!(["again"]));
}

#[test]
fn action_run_tail_and_run_count_are_bounded() {
    let c = cfg();
    let (m, _rx) = manager(&c);
    let mut hub = Hub::new("demo");
    for i in 0..(ACTION_TAIL_LINES + 7) {
        hub.on_event(&m, &progress("Big", &format!("l{i}")));
    }
    let runs = snapshot_runs(&mut hub, &m);
    let lines = runs[0]["lines"].as_array().unwrap();
    assert_eq!(lines.len(), ACTION_TAIL_LINES);
    assert_eq!(lines[0], "l7");
    assert_eq!(runs[0]["skipped"], 7);

    for i in 0..(ACTION_RUNS + 3) {
        let label = format!("run{i}");
        hub.on_event(&m, &progress(&label, "x"));
        hub.on_event(&m, &done(&label, true));
    }
    let runs = snapshot_runs(&mut hub, &m);
    assert_eq!(runs.len(), ACTION_RUNS);
    assert_eq!(
        runs.last().unwrap()["label"],
        format!("run{}", ACTION_RUNS + 2)
    );
}

#[test]
fn session_resume_pings_and_silent_actions_are_handled() {
    let c = cfg();
    let (m, _rx) = manager(&c);
    let mut hub = Hub::new("demo");
    // Empty label = resume ping: never a run.
    hub.on_event(&m, &done("", true));
    assert!(snapshot_runs(&mut hub, &m).is_empty());
    // An action that printed nothing still shows up once it completes.
    hub.on_event(&m, &done("Quiet", true));
    let runs = snapshot_runs(&mut hub, &m);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["status"], "ok");
    assert_eq!(runs[0]["lines"], json!([]));
}

#[test]
fn rows_only_sent_when_changed() {
    let c = cfg();
    let (mut m, _rx) = manager(&c);
    let mut hub = Hub::new("demo");
    let (tx, mut rx) = mpsc::channel(crate::control::SUBSCRIBER_BUFFER);
    hub.subscribe(&m, tx);
    hub.tick(&mut m);
    assert!(
        rx.try_recv().is_ok(),
        "first tick after subscribe sends rows"
    );
    hub.tick(&mut m);
    assert!(rx.try_recv().is_err(), "unchanged rows are not resent");
    m.update_state("api", ServiceState::Running);
    hub.tick(&mut m);
    let v: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(v["t"], "rows");
    assert_eq!(v["services"][0]["state"], "running");
}

#[tokio::test]
async fn runner_refuses_sessions_and_serves_info() {
    let c = cfg();
    let (mut m, _rx) = manager(&c);
    let mut hub = Hub::new("demo");

    let ask = |cmd| {
        let (reply, rx) = tokio::sync::oneshot::channel();
        (
            ControlRequest {
                cmd,
                reply,
                stream: None,
            },
            rx,
        )
    };
    let (req, rx) = ask(ControlCmd::Info);
    assert!(!hub.handle(&mut m, &c, req).await);
    let info = rx.await.unwrap();
    assert_eq!(info.data.unwrap()["mode"], "runner");

    let (req, rx) = ask(ControlCmd::RunAction {
        target: "x".into(),
        confirmed: true,
    });
    hub.handle(&mut m, &c, req).await;
    let r = rx.await.unwrap();
    assert!(!r.ok);
    assert!(r.error.unwrap().contains("owner-only"));

    let (req, _rx) = ask(ControlCmd::Shutdown);
    assert!(
        hub.handle(&mut m, &c, req).await,
        "shutdown asks the loop to stop"
    );
}

// ── protocol over a real socket ──────────────────────────────────────────────

/// A runner loop in miniature: the real `ControlServer` + `Hub::handle`, with the
/// test pushing manager events through `Hub::on_event` like the real loop does.
struct MiniRunner {
    dir: tempfile::TempDir,
    events: mpsc::UnboundedSender<ManagerEvent>,
    stopped: tokio::sync::oneshot::Receiver<()>,
    task: tokio::task::JoinHandle<()>,
}

fn mini_runner() -> MiniRunner {
    let dir = tempfile::tempdir().unwrap();
    let c = cfg();
    let (server, mut control_rx) = ControlServer::bind(dir.path()).unwrap();
    let (events, mut event_rx) = mpsc::unbounded_channel::<ManagerEvent>();
    let (stop_tx, stopped) = tokio::sync::oneshot::channel();
    let root = dir.path().to_path_buf();
    let task = tokio::spawn(async move {
        let _server = server;
        let (tx, _keep) = mpsc::unbounded_channel();
        let mut m = ServiceManager::new(root, &c, tx);
        let mut hub = Hub::new("demo");
        loop {
            tokio::select! {
                Some(req) = control_rx.recv() => {
                    if hub.handle(&mut m, &c, req).await {
                        hub.bye();
                        let _ = stop_tx.send(());
                        return;
                    }
                }
                Some(ev) = event_rx.recv() => hub.on_event(&m, &ev),
            }
        }
    });
    MiniRunner {
        dir,
        events,
        stopped,
        task,
    }
}

#[tokio::test]
async fn subscribe_over_the_socket_snapshot_then_ordered_stream() {
    let r = mini_runner();
    for i in 0..3 {
        r.events.send(log("api", &format!("old-{i}"))).unwrap();
    }
    // Let the loop apply the three before-events before subscribing.
    let info = loop {
        let i = control::info(r.dir.path()).await.unwrap();
        if i.clients == 0 {
            break i;
        }
    };
    assert_eq!(info.mode, "runner");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut sub = control::subscribe(r.dir.path()).await.unwrap();
    let backlog: Vec<&str> = sub.snapshot["logs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["line"].as_str().unwrap())
        .collect();
    assert_eq!(backlog, ["old-0", "old-1", "old-2"]);

    for i in 0..50 {
        r.events.send(log("api", &format!("new-{i}"))).unwrap();
    }
    for i in 0..50 {
        let ev = tokio::time::timeout(Duration::from_secs(5), sub.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ev["t"], "log");
        assert_eq!(
            ev["line"],
            format!("new-{i}"),
            "stream keeps order, no gap/dupe"
        );
    }
    r.task.abort();
}

#[tokio::test]
async fn many_clients_and_a_disconnect_do_not_disturb_the_runner() {
    let r = mini_runner();
    let mut a = control::subscribe(r.dir.path()).await.unwrap();
    let mut b = control::subscribe(r.dir.path()).await.unwrap();
    let c = control::subscribe(r.dir.path()).await.unwrap();
    assert_eq!(control::info(r.dir.path()).await.unwrap().clients, 3);

    drop(c); // a client vanishes mid-session
    r.events.send(log("api", "after-drop")).unwrap();
    for sub in [&mut a, &mut b] {
        let ev = tokio::time::timeout(Duration::from_secs(5), sub.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(ev["line"], "after-drop");
    }
    // The runner still answers requests, and the dead client is no longer counted.
    let mut clients = 3;
    for _ in 0..40 {
        r.events.send(log("api", "poke")).unwrap();
        clients = control::info(r.dir.path()).await.unwrap().clients;
        if clients == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(clients, 2);
    r.task.abort();
}

#[tokio::test]
async fn shutdown_says_bye_to_clients_and_stops_the_loop() {
    let r = mini_runner();
    let mut sub = control::subscribe(r.dir.path()).await.unwrap();
    let resp = control::call(r.dir.path(), "shutdown", json!({}))
        .await
        .unwrap();
    assert!(resp.ok);
    r.stopped.await.unwrap();
    let bye = tokio::time::timeout(Duration::from_secs(5), sub.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bye["t"], "bye");
    assert!(sub.next().await.is_none(), "stream closes after bye");
}

#[tokio::test]
async fn sending_garbage_on_a_subscription_does_not_break_other_clients() {
    let r = mini_runner();
    let mut good = control::subscribe(r.dir.path()).await.unwrap();
    // A raw subscriber that writes junk and hangs up.
    {
        use tokio::io::AsyncWriteExt;
        let mut raw = atlas_core::ipc::connect(&control::socket_path(r.dir.path()))
            .await
            .unwrap();
        raw.write_all(b"{\"cmd\":\"subscribe\"}\n").await.unwrap();
        raw.write_all(b"not json at all\n").await.unwrap();
    }
    r.events.send(log("api", "still-fine")).unwrap();
    let ev = tokio::time::timeout(Duration::from_secs(5), good.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ev["line"], "still-fine");
    r.task.abort();
}
