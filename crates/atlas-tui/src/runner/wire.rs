//! The JSON shapes shared by the attach stream, the snapshot and `rows`:
//! stream/state names and the per-log-entry and per-service objects.

use serde_json::{Value, json};

use crate::runtime::logs::{LogEntry, LogStream};
use crate::runtime::manager::ServiceManager;
use crate::runtime::service::ServiceState;

pub(super) fn stream_name(stream: LogStream) -> &'static str {
    match stream {
        LogStream::Stdout => "stdout",
        LogStream::Stderr => "stderr",
    }
}

/// Wire form of one log entry. `t`/`name`/`line`/`stream` are the original
/// fields; `seq`, `ts` (epoch ms), `level` (`debug|info|warn|error`), `kind`
/// (`line|restart|mark`) and `trunc` are additive: old readers ignore them.
pub(super) fn log_json(e: &LogEntry) -> Value {
    let mut v = json!({
        "t": "log", "name": e.service, "line": e.content,
        "stream": stream_name(e.stream), "seq": e.seq, "ts": e.timestamp,
        "level": e.level.wire(),
    });
    if e.restart {
        v["kind"] = "restart".into();
    } else if e.mark {
        v["kind"] = "mark".into();
    }
    if e.truncated {
        v["trunc"] = true.into();
    }
    v
}

pub(super) fn state_name(state: &ServiceState) -> &'static str {
    match state {
        ServiceState::Stopped => "stopped",
        ServiceState::Starting => "starting",
        ServiceState::Running => "running",
        ServiceState::Failed => "failed",
        ServiceState::Disabled => "disabled",
        ServiceState::Suspended => "suspended",
    }
}

pub(crate) fn parse_state(s: &str) -> Option<ServiceState> {
    Some(match s {
        "stopped" => ServiceState::Stopped,
        "starting" => ServiceState::Starting,
        "running" => ServiceState::Running,
        "failed" => ServiceState::Failed,
        "disabled" => ServiceState::Disabled,
        "suspended" => ServiceState::Suspended,
        _ => return None,
    })
}

/// State + facts of one service, as sent on the wire (`state` events and `rows`).
pub(super) fn service_json(manager: &ServiceManager, name: &str, state: &ServiceState) -> Value {
    json!({
        "name": name,
        "state": state_name(state),
        "exit_code": manager.exit_code(name),
        "restarts": manager.restart_count(name),
        "note": manager.service_note(name),
        "completed_ok": manager.completed_ok(name),
        "paused_by": manager.paused_by(name),
        "deselected": manager.is_deselected(name),
        "port_conflict": manager.port_conflict(name),
        // Ad-hoc (`atlas -- <cmd>`): an attaching client rebuilds the service from this.
        "adhoc": manager.adhoc_command(name),
        // Whole MB / whole percent: finer values would re-send `rows` every sample.
        "mem_mb": manager.usage(name).map(|u| u.mem >> 20),
        "cpu": manager.usage(name).map(|u| u.cpu.round() as u32),
    })
}

pub(super) fn tunnel_json(manager: &ServiceManager) -> Value {
    match manager.tunnel_status() {
        Some(t) => json!({
            "state": state_name(&t.state),
            "local_port": t.local_port,
            "remote": t.remote,
            "ssh_host": t.ssh_host,
        }),
        None => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_names_roundtrip() {
        for s in [
            ServiceState::Stopped,
            ServiceState::Starting,
            ServiceState::Running,
            ServiceState::Failed,
            ServiceState::Disabled,
            ServiceState::Suspended,
        ] {
            assert_eq!(parse_state(state_name(&s)), Some(s));
        }
        assert_eq!(parse_state("nope"), None);
    }

    // Contract (#133): `apps/desktop/src/lib/__fixtures__/service-port-conflict.json` is the REAL
    // `service_json` of a failed service whose port a foreign process holds; `portConflict.test.ts`
    // feeds it to the desktop's own comparison, and `remote_tests` to the attached TUI.
    // `UPDATE_FIXTURES=1` regenerates it.
    #[test]
    fn service_json_carries_port_conflict_as_its_own_object_and_matches_the_fixture() {
        use crate::config::{ProjectConfig, ServiceDef};
        use crate::runtime::port_conflict::PortConflict;
        let cfg = ProjectConfig {
            name: "t".into(),
            services: std::collections::HashMap::from([(
                "web".to_string(),
                ServiceDef {
                    command: Some("true".into()),
                    port: Some(4321),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut m = ServiceManager::new(std::env::temp_dir().join("atlas-wire-fixture"), &cfg, tx);
        let conflict = PortConflict {
            pid: 12345,
            cmd: "node".into(),
            cwd: Some("/work/app".into()),
        };
        m.set_port_conflict("web", Some(conflict));
        m.set_state_if_changed("web", ServiceState::Failed);

        let fresh = service_json(&m, "web", &ServiceState::Failed);
        assert_eq!(
            fresh["port_conflict"],
            json!({"pid": 12345, "cmd": "node", "cwd": "/work/app"})
        );
        // Its own field: the note names the holder, the object is not folded into it.
        let stop = if cfg!(windows) {
            "taskkill /PID 12345 /F"
        } else {
            "kill 12345"
        };
        assert_eq!(
            fresh["note"],
            format!(
                "port 4321 held by PID 12345 (node, cwd /work/app): stop it yourself ({stop}) or change the service port"
            )
        );
        // A service without a conflict says `null`, not a missing key.
        m.set_port_conflict("web", None);
        assert!(service_json(&m, "web", &ServiceState::Failed)["port_conflict"].is_null());

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/service-port-conflict.json");
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(&fresh, &mut ser).unwrap();
            std::fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let mut committed: Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .expect("fixture missing: run with UPDATE_FIXTURES=1 to create it"),
        )
        .unwrap();
        let mut fresh = fresh;
        if cfg!(windows) {
            // The committed note is the Unix form (`kill 12345`); Windows advises `taskkill`.
            committed["note"] = Value::Null;
            fresh["note"] = Value::Null;
        }
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }
}
