//! Attached mode through the real `App`: snapshot + stream feed the shared render path,
//! detach never touches services, keys and effects route to the runner.

use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::json;
use tokio::sync::mpsc;

use super::*;
use crate::config::{ProjectConfig, ServiceDef};
use crate::event::EventHandler;
use crate::runtime::manager::ServiceManager;

fn config() -> ProjectConfig {
    let mut services = HashMap::new();
    for (i, n) in ["api", "web"].iter().enumerate() {
        services.insert(
            n.to_string(),
            ServiceDef {
                command: Some("sleep 30".into()),
                port: Some(4000 + i as u16),
                ..Default::default()
            },
        );
    }
    ProjectConfig {
        name: "demo".into(),
        services,
        ..Default::default()
    }
}

/// An attached App plus the (unused) stream sender that keeps its channel open.
fn attached() -> (App, mpsc::UnboundedSender<RemoteEvent>) {
    let cfg = config();
    let (tx, rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from("/tmp/atlas-attach-test"), &cfg, tx).into_mirror();
    let (_, events) = EventHandler::test_channel();
    let mut app = App::new(cfg, mgr, rx, events, false);
    let (stx, srx) = mpsc::unbounded_channel();
    app.attach(
        Remote::new(PathBuf::from("/tmp/atlas-attach-test"), 4242),
        srx,
    );
    app.size = (140, 36);
    (app, stx)
}

fn snapshot() -> serde_json::Value {
    json!({
        "started": true,
        "services": [
            {"name": "api", "state": "running", "exit_code": null, "restarts": 0,
             "note": null, "completed_ok": false, "paused_by": null, "deselected": false},
            {"name": "web", "state": "failed", "exit_code": 7, "restarts": 2,
             "note": "restarting in 4s (2/5)", "completed_ok": false, "paused_by": null,
             "deselected": false},
        ],
        "tunnel": null,
        "logs": [
            {"name": "api", "line": "hello from before attach", "stream": "stdout", "ts": 1_700_000_000_000u64},
        ],
        "events": [
            {"t": "state", "name": "api", "state": "running", "exit_code": null, "restarts": 0,
             "note": null, "completed_ok": false, "paused_by": null, "deselected": false},
        ],
        "operation": null,
    })
}

fn screen(app: &App) -> String {
    let mut term = Terminal::new(TestBackend::new(140, 36)).unwrap();
    term.draw(|f| app.view(f)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn key(app: &mut App, c: char) {
    let msg = app.map_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    if let Some(m) = msg {
        app.update(m);
    }
}

#[tokio::test]
async fn snapshot_fills_the_same_dashboard() {
    let (mut app, _keep) = attached();
    app.apply_snapshot(&snapshot());
    assert!(app.started && app.splash_done);
    assert_eq!(app.manager.get_state("web"), Some(ServiceState::Failed));
    assert_eq!(app.manager.exit_code("web"), Some(7));
    assert_eq!(app.manager.restart_count("web"), 2);
    let out = screen(&app);
    assert!(out.contains("hello from before attach"), "{out}");
    assert!(out.contains("attached"), "brand shows attached: {out}");
    assert!(out.contains("api running"), "event history replayed: {out}");
}

#[tokio::test]
async fn stream_events_update_state_logs_and_events() {
    let (mut app, _keep) = attached();
    app.apply_snapshot(&snapshot());

    assert!(app.apply_stream_event(&json!(
        {"t": "log", "name": "web", "line": "live line", "stream": "stderr"})));
    assert!(
        app.logs
            .log_buffer
            .all()
            .iter()
            .any(|e| e.content == "live line")
    );

    assert!(app.apply_stream_event(&json!(
        {"t": "state", "name": "web", "state": "running", "exit_code": null, "restarts": 0,
         "note": null, "completed_ok": false, "paused_by": null, "deselected": false})));
    assert_eq!(app.manager.get_state("web"), Some(ServiceState::Running));
    assert!(
        app.event_log
            .iter()
            .any(|e| e.subject == "web" && e.text == "running"),
        "state change produced the same sidebar event as the owner path"
    );

    // Re-announcing the same state (health watcher) is not a new event.
    let before = app.event_log.len();
    app.apply_stream_event(&json!(
        {"t": "state", "name": "web", "state": "running", "exit_code": null, "restarts": 0,
         "note": null, "completed_ok": false, "paused_by": null, "deselected": false}));
    assert_eq!(app.event_log.len(), before);

    app.apply_stream_event(&json!({"t": "rows", "services": [
        {"name": "api", "state": "running", "note": "waiting for :4000 (3s)", "restarts": 0,
         "completed_ok": false, "deselected": false}], "tunnel": null}));
    assert_eq!(
        app.manager.service_note("api").as_deref(),
        Some("waiting for :4000 (3s)")
    );

    assert!(
        !app.apply_stream_event(&json!({"t": "bye"})),
        "bye ends the stream"
    );
}

#[tokio::test]
async fn action_done_never_makes_the_client_resume_services() {
    let (mut app, _keep) = attached();
    app.apply_snapshot(&snapshot());
    app.apply_stream_event(&json!({"t": "action_done", "label": "Migrate", "ok": true}));
    // A local RestartService effect would have been queued for each `resume` name.
    assert!(app.effects.is_empty());
}

#[tokio::test]
async fn detach_key_leaves_without_stopping_anything() {
    let (mut app, _keep) = attached();
    app.apply_snapshot(&snapshot());
    key(&mut app, 'D');
    assert!(app.should_quit);
    assert_eq!(app.remote.as_ref().unwrap().leave, Some(Leave::Detach));
}

#[tokio::test]
async fn ctrl_c_and_sigterm_detach_while_attached() {
    let (mut app, _keep) = attached();
    app.update(Message::Quit);
    assert_eq!(app.remote.as_ref().unwrap().leave, Some(Leave::Detach));
}

#[tokio::test]
async fn q_asks_detach_or_stop_with_detach_as_default() {
    let (mut app, _keep) = attached();
    app.apply_snapshot(&snapshot());
    key(&mut app, 'q');
    assert_eq!(app.layer, Layer::Quit);
    assert_eq!(app.quit_selected, 0, "Detach is the default");
    let out = screen(&app);
    assert!(
        out.contains("Detach") && out.contains("Stop everything"),
        "{out}"
    );

    // Enter on the default detaches.
    let m = app.map_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.update(m.unwrap());
    assert_eq!(app.remote.as_ref().unwrap().leave, Some(Leave::Detach));
}

#[tokio::test]
async fn q_then_tab_enter_stops_the_runner() {
    let (mut app, _keep) = attached();
    key(&mut app, 'q');
    let tab = app
        .map_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    app.update(tab);
    let enter = app
        .map_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    app.update(enter);
    assert_eq!(app.remote.as_ref().unwrap().leave, Some(Leave::Stopped));
    assert!(app.should_quit);
}

#[tokio::test]
async fn owner_tui_only_hints_how_to_keep_services_running() {
    let cfg = config();
    let (tx, rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from("/tmp/atlas-attach-test"), &cfg, tx);
    let (_, events) = EventHandler::test_channel();
    let mut app = App::new(cfg, mgr, rx, events, false);
    key(&mut app, 'D');
    assert!(!app.should_quit, "an owner has nothing to detach from");
    let (msg, _, _) = app.toast.clone().expect("hint toast");
    assert!(msg.contains("atlas dev --detach"), "{msg}");

    app.splash_done = true;
    let out = screen(&app);
    assert!(
        !out.contains("D Detach"),
        "footer advertises D only when attached"
    );
}

#[tokio::test]
async fn footer_and_help_advertise_detach_only_when_attached() {
    let (mut app, _keep) = attached();
    app.apply_snapshot(&snapshot());
    assert!(screen(&app).contains("D Detach"));
    app.update(Message::ShowHelp);
    assert!(screen(&app).contains("Detach: leave"));
}

#[tokio::test]
async fn sessions_are_owner_only_with_a_clear_toast() {
    let mut cfg = config();
    cfg.actions.push(
        serde_yaml::from_str("{ key: x, label: Shell, command: sh, session: true }").unwrap(),
    );
    let (tx, rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from("/tmp/atlas-attach-test"), &cfg, tx).into_mirror();
    let (_, events) = EventHandler::test_channel();
    let mut app = App::new(cfg, mgr, rx, events, false);
    let (_stx, srx) = mpsc::unbounded_channel();
    app.attach(Remote::new(PathBuf::from("/tmp/atlas-attach-test"), 1), srx);
    app.update(Message::RunAction('x'));
    app.execute_effects().await;
    let (msg, _, _) = app.toast.clone().unwrap();
    assert!(msg.contains("owner-only"), "{msg}");
    assert!(app.operation.is_none());
}

#[tokio::test]
async fn tunnel_key_is_explained_not_toggled_while_attached() {
    let mut cfg = config();
    cfg.tunnel = Some(
        serde_yaml::from_str(
            "{ ssh_host: h, local_port: 5400, remote_host: db, remote_port: 5432 }",
        )
        .unwrap(),
    );
    let (tx, rx) = mpsc::unbounded_channel();
    let mgr = ServiceManager::new(PathBuf::from("/tmp/atlas-attach-test"), &cfg, tx).into_mirror();
    let (_, events) = EventHandler::test_channel();
    let mut app = App::new(cfg, mgr, rx, events, false);
    let (_stx, srx) = mpsc::unbounded_channel();
    app.attach(Remote::new(PathBuf::from("/tmp/atlas-attach-test"), 1), srx);
    app.update(Message::ToggleTunnelOnExit);
    assert!(app.tunnel_kill_on_exit, "policy untouched");
    assert!(app.toast.clone().unwrap().0.contains("runner"));
}
