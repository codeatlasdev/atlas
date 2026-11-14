use serde_json::{Value, json};

use super::*;
use crate::config::ProjectConfig;

fn cfg() -> ProjectConfig {
    serde_norway::from_str(
        r#"
name: demo
services:
  api: { command: "sleep 1", port: 3000 }
  web: { command: "sleep 1" }
actions:
  - { key: m, label: Migrate, command: "true" }
  - { key: x, label: Wipe DB, command: "true", confirm: "This wipes the DB." }
"#,
    )
    .unwrap()
}

#[test]
fn parse_requires_service_arg() {
    assert!(ControlCmd::parse("restart_service", &json!({})).is_err());
    assert!(ControlCmd::parse("restart_service", &json!({"service": "  "})).is_err());
    assert_eq!(
        ControlCmd::parse("stop_service", &json!({"service": "api"})).unwrap(),
        ControlCmd::StopService("api".into())
    );
}

#[test]
fn parse_rejects_unknown_cmd() {
    let e = ControlCmd::parse("rm_rf", &json!({})).unwrap_err();
    assert!(e.contains("unknown cmd"));
}

#[test]
fn resolve_rejects_unknown_service_listing_known() {
    let e = resolve(ControlCmd::RestartService("nope".into()), &cfg()).unwrap_err();
    assert!(e.contains("api, web"), "{e}");
}

#[test]
fn resolve_action_by_key_or_label() {
    let by_key = resolve(
        ControlCmd::RunAction {
            target: "m".into(),
            confirmed: false,
        },
        &cfg(),
    )
    .unwrap();
    let by_label = resolve(
        ControlCmd::RunAction {
            target: "migrate".into(),
            confirmed: false,
        },
        &cfg(),
    )
    .unwrap();
    assert_eq!(by_key, by_label);
}

#[test]
fn confirm_action_refused_without_confirmed_and_returns_text() {
    let cmd = |confirmed| ControlCmd::RunAction {
        target: "x".into(),
        confirmed,
    };
    let e = resolve(cmd(false), &cfg()).unwrap_err();
    assert!(e.contains("This wipes the DB."), "{e}");
    assert!(e.contains("confirmed: true"), "{e}");
    assert_eq!(
        resolve(cmd(true), &cfg()).unwrap(),
        Resolved::RunAction {
            key: 'x',
            label: "Wipe DB".into()
        }
    );
}

#[tokio::test]
async fn call_without_server_is_not_running() {
    let dir = tempfile::TempDir::new().unwrap();
    let e = call(dir.path(), "restart_service", json!({"service": "api"}))
        .await
        .unwrap_err();
    assert_eq!(e, NOT_RUNNING);
}

// Mode bits: Unix sockets only (Windows serves a named pipe).
#[cfg(unix)]
#[tokio::test]
async fn socket_roundtrip_perms_and_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    // A stale socket file from a crashed run must be replaced.
    std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
    std::fs::write(socket_path(dir.path()), "stale").unwrap();

    let (server, mut rx) = ControlServer::bind(dir.path(), Guard::new(&cfg(), dir.path())).unwrap();
    let mode = std::fs::metadata(socket_path(dir.path()))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    let root = dir.path().to_path_buf();
    let client =
        tokio::spawn(
            async move { call(&root, "restart_service", json!({"service": "api"})).await },
        );
    let req = rx.recv().await.unwrap();
    assert_eq!(req.cmd, ControlCmd::RestartService("api".into()));
    req.reply
        .send(ControlResponse::ok(json!({"x": 1})))
        .unwrap();
    let resp = client.await.unwrap().unwrap();
    assert!(resp.ok);
    assert_eq!(resp.data.unwrap()["x"], 1);

    // Malformed input gets an error response, not a hang or a loop request.
    let bad = call(dir.path(), "nope", json!({})).await.unwrap();
    assert!(!bad.ok);
    assert!(rx.try_recv().is_err());

    drop(server);
    assert!(!socket_path(dir.path()).exists());
}

// ── agents policy + audit, through the real socket server and client ──

/// Project whose `agents:` block is `agents_yaml` (indented under `agents:`).
fn cfg_with_agents(agents_yaml: &str) -> ProjectConfig {
    let mut yaml = String::from(
        "name: demo\nservices:\n  api: { command: \"sleep 1\", port: 3000 }\n  web: { command: \"sleep 1\" }\n\
         actions:\n  - { key: m, label: Migrate, command: \"true\" }\n  - { key: w, label: Wipe DB, command: \"true\" }\nagents:\n",
    );
    for l in agents_yaml.lines() {
        yaml.push_str("  ");
        yaml.push_str(l);
        yaml.push('\n');
    }
    serde_norway::from_str(&yaml).unwrap()
}

fn agent() -> Caller {
    Caller::Agent {
        name: Some("omp".into()),
        version: Some("14.1".into()),
        session: None,
    }
}

/// A served project plus a fake run loop that answers every request it gets with `reply_ok`
/// and counts them (a denied call must never reach it).
struct Served {
    dir: tempfile::TempDir,
    seen: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    _server: ControlServer,
}

fn serve(cfg: &ProjectConfig, reply_ok: bool) -> Served {
    let dir = tempfile::TempDir::new().unwrap();
    let (server, mut rx) = ControlServer::bind(dir.path(), Guard::new(cfg, dir.path())).unwrap();
    let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = seen.clone();
    tokio::spawn(async move {
        while let Some(req) = rx.recv().await {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = req.reply.send(if reply_ok {
                ControlResponse::ok(json!({"queued": true}))
            } else {
                ControlResponse::err("unknown service 'x'")
            });
        }
    });
    Served {
        dir,
        seen,
        _server: server,
    }
}

impl Served {
    fn reached_loop(&self) -> usize {
        self.seen.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn audit(&self) -> Vec<audit::AuditEntry> {
        audit::read_recent(&crate::runtime::log_dir(self.dir.path()), 100, false)
    }
}

#[tokio::test]
async fn denied_agent_call_is_refused_before_the_run_loop_and_audited() {
    let s = serve(&cfg_with_agents("control: deny"), true);
    let r = call_as(
        s.dir.path(),
        "restart_service",
        json!({"service": "api"}),
        &agent(),
    )
    .await
    .unwrap();
    assert!(!r.ok);
    let e = r.error.unwrap();
    assert!(
        e.contains("atlas.yaml denies agents restart_service on api"),
        "{e}"
    );
    assert!(e.contains("ask the developer"), "{e}");
    assert_eq!(
        s.reached_loop(),
        0,
        "a denied call must never reach the run loop"
    );

    let log = s.audit();
    assert_eq!(log.len(), 1, "{log:?}");
    let l = &log[0];
    assert_eq!(
        (
            l.caller.as_str(),
            l.tool.as_str(),
            l.target.as_str(),
            l.result
        ),
        ("agent", "restart_service", "api", audit::Outcome::Denied)
    );
    assert_eq!(l.client.as_deref(), Some("omp/14.1"));
    assert_eq!(l.detail.as_deref(), Some(e.as_str()));
}

#[tokio::test]
async fn the_developer_is_never_restricted_by_the_agents_policy_and_is_audited_as_human() {
    let s = serve(&cfg_with_agents("control: deny"), true);
    // `call` is what the CLI, the desktop and the TUI use: no caller on the wire.
    let r = call(s.dir.path(), "stop_service", json!({"service": "api"}))
        .await
        .unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(s.reached_loop(), 1);
    let log = s.audit();
    assert_eq!(log.len(), 1);
    assert_eq!(
        (
            log[0].caller.as_str(),
            log[0].result,
            log[0].client.as_deref()
        ),
        ("human", audit::Outcome::Allowed, None)
    );
}

#[tokio::test]
async fn no_agents_block_keeps_every_agent_call_allowed() {
    // Regression guard for existing users: the default is today's behaviour.
    let cfg: ProjectConfig =
        serde_norway::from_str("name: d\nservices:\n  api: { command: x }\n").unwrap();
    let s = serve(&cfg, true);
    for (cmd, args) in [
        ("restart_service", json!({"service": "api"})),
        ("stop_service", json!({"service": "api"})),
        ("start_service", json!({"service": "api"})),
    ] {
        let r = call_as(s.dir.path(), cmd, args, &agent()).await.unwrap();
        assert!(r.ok, "{cmd}: {r:?}");
    }
    assert_eq!(s.reached_loop(), 3);
    assert!(
        s.audit()
            .iter()
            .all(|l| l.result == audit::Outcome::Allowed)
    );
}

#[tokio::test]
async fn allowed_call_that_atlas_refuses_is_audited_as_error_with_the_reason() {
    let s = serve(&cfg_with_agents("restart_service: allow"), false);
    let r = call_as(
        s.dir.path(),
        "restart_service",
        json!({"service": "x"}),
        &agent(),
    )
    .await
    .unwrap();
    assert!(!r.ok);
    let log = s.audit();
    assert_eq!(log[0].result, audit::Outcome::Error);
    assert_eq!(log[0].detail.as_deref(), Some("unknown service 'x'"));
}

#[tokio::test]
async fn per_service_and_per_action_lists_apply_to_agents_only_where_listed() {
    let s = serve(
        &cfg_with_agents(
            "control: deny\nrestart_service: { allow: [web] }\nrun_action: { allow: [Migrate] }",
        ),
        true,
    );
    let ask = |cmd: &'static str, args: Value| {
        let root = s.dir.path().to_path_buf();
        async move { call_as(&root, cmd, args, &agent()).await.unwrap().ok }
    };
    assert!(ask("restart_service", json!({"service": "web"})).await);
    assert!(!ask("restart_service", json!({"service": "api"})).await);
    assert!(
        !ask("stop_service", json!({"service": "web"})).await,
        "other tools stay denied"
    );
    // The list names the label; the agent may say the key or the label.
    assert!(ask("run_action", json!({"action": "m"})).await);
    assert!(ask("run_action", json!({"action": "migrate"})).await);
    assert!(!ask("run_action", json!({"action": "w"})).await);
    assert!(!ask("run_action", json!({"action": "Wipe DB"})).await);
    assert_eq!(s.reached_loop(), 3);
}

#[tokio::test]
async fn an_agent_cannot_shut_atlas_down_or_add_services_even_with_no_policy() {
    let cfg: ProjectConfig =
        serde_norway::from_str("name: d\nservices:\n  api: { command: x }\n").unwrap();
    let s = serve(&cfg, true);
    for (cmd, args) in [
        ("shutdown", json!({})),
        ("add_service", json!({"command": "ls"})),
    ] {
        let r = call_as(s.dir.path(), cmd, args, &agent()).await.unwrap();
        assert!(!r.ok, "{cmd} must be refused for an agent");
    }
    assert_eq!(s.reached_loop(), 0);
    // The developer still can.
    assert!(call(s.dir.path(), "shutdown", json!({})).await.unwrap().ok);
}

#[test]
fn caller_wire_shape_is_what_atlas_mcp_sends_and_the_server_reads() {
    // Real serialized producer output, parsed by the real consumer type.
    let sent = serde_json::to_value(Caller::Agent {
        name: Some("omp".into()),
        version: Some("14.1".into()),
        session: Some(json!({"OMP_SESSION_ID": "s1"})),
    })
    .unwrap();
    assert_eq!(sent["kind"], "agent");
    assert_eq!(sent["name"], "omp");
    assert_eq!(sent["version"], "14.1");
    assert_eq!(sent["session"]["OMP_SESSION_ID"], "s1");
    assert_eq!(
        serde_json::from_value::<Caller>(sent).unwrap().kind(),
        "agent"
    );
    // No caller on the wire = the developer; an unknown kind is rejected, not read as human.
    let req: protocol::WireRequest = serde_json::from_str(r#"{"cmd":"info"}"#).unwrap();
    assert_eq!(req.caller, Caller::Human);
    assert!(
        serde_json::from_str::<protocol::WireRequest>(
            r#"{"cmd":"info","caller":{"kind":"robot"}}"#
        )
        .is_err()
    );
}

#[test]
fn audit_file_rotates_per_run_and_when_over_the_size_cap() {
    let dir = tempfile::TempDir::new().unwrap();
    let logs = dir.path();
    let entry = |t: &str| {
        audit::AuditEntry::new(
            &Caller::Human,
            "restart_service",
            t,
            &json!({}),
            audit::Outcome::Allowed,
            None,
        )
    };
    audit::record(logs, &entry("run1"));
    // A new run: last run's lines stay readable as .prev, the current file starts empty.
    audit::rotate_for_new_run(logs);
    audit::record(logs, &entry("run2"));
    let cur = audit::read_recent(logs, 10, false);
    assert_eq!(
        cur.iter().map(|e| e.target.as_str()).collect::<Vec<_>>(),
        ["run2"]
    );
    let both = audit::read_recent(logs, 10, true);
    assert_eq!(
        both.iter().map(|e| e.target.as_str()).collect::<Vec<_>>(),
        ["run1", "run2"]
    );
    assert!(logs.join("atlas-audit.prev.log").exists());

    // Over the cap: the next record rotates instead of growing without bound.
    std::fs::write(logs.join("atlas-audit.log"), vec![b'x'; 6 * 1024 * 1024]).unwrap();
    audit::record(logs, &entry("after-cap"));
    let cur = audit::read_recent(logs, 10, false);
    assert_eq!(
        cur.iter().map(|e| e.target.as_str()).collect::<Vec<_>>(),
        ["after-cap"]
    );
    assert!(
        std::fs::metadata(logs.join("atlas-audit.prev.log"))
            .unwrap()
            .len()
            >= 6 * 1024 * 1024
    );
}

#[test]
fn audit_reader_skips_garbage_lines_and_keeps_only_the_newest() {
    let dir = tempfile::TempDir::new().unwrap();
    let e = |t: &str| {
        audit::AuditEntry::new(
            &Caller::Human,
            "stop_service",
            t,
            &json!({}),
            audit::Outcome::Allowed,
            None,
        )
    };
    for t in ["a", "b", "c"] {
        audit::record(dir.path(), &e(t));
    }
    // A half-written line (crash mid-write) must not hide the entries around it.
    let p = dir.path().join("atlas-audit.log");
    let mut text = std::fs::read_to_string(&p).unwrap();
    text.push_str("{\"ts\":\"2026-11-1");
    std::fs::write(&p, text).unwrap();
    let got = audit::read_recent(dir.path(), 2, false);
    assert_eq!(
        got.iter().map(|e| e.target.as_str()).collect::<Vec<_>>(),
        ["b", "c"]
    );
}

#[test]
fn audit_args_are_summarized_without_the_target_and_long_values_are_cut() {
    let long = "x".repeat(5000);
    let e = audit::AuditEntry::new(
        &Caller::Human,
        "run_action",
        "m",
        &json!({"action": "m", "confirmed": true, "note": long}),
        audit::Outcome::Allowed,
        None,
    );
    assert_eq!(e.args["confirmed"], true);
    assert!(e.args.get("action").is_none());
    assert!(e.args["note"].as_str().unwrap().chars().count() < 300);
    let line = serde_json::to_string(&e).unwrap();
    assert!(
        !line.contains('\n') && line.len() < 1000,
        "one short line per call"
    );
}
