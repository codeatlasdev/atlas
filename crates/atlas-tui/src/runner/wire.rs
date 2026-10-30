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
}
