//! `atlas dev --status`: read-only liveness check over `.logs/.tui-status.json`.

use std::path::Path;

use serde_json::Value;

/// A status file older than this is considered stale (crashed/hung run).
const STALE_AFTER_SECS: u64 = 120;

/// One service row from the status file.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceRow {
    pub name: String,
    pub state: String,
    pub port: Option<u64>,
    pub health: Option<String>,
    /// Exit code of the last exited child (None while running / never exited).
    pub exit_code: Option<i64>,
    /// Restart attempts in the current streak.
    pub restarts: u64,
    /// Seconds Running (None when not running).
    pub uptime_s: Option<u64>,
    /// Memory of the process tree, MB (None when not running).
    pub mem_mb: Option<u64>,
    /// CPU of the process tree, percent of one core.
    pub cpu: Option<u64>,
    /// Unix seconds the current/last child was spawned.
    pub started_at: Option<u64>,
    /// Scheduler annotation (`waiting for :3000 (12s)`, `restarting in 2s (1/5)`, …).
    pub note: Option<String>,
    /// Not in this run's selection (`atlas dev SERVICE…` / `--profile`): shown
    /// `disabled`, startable by hand. Cleared once it is started.
    pub off: bool,
}

/// The SSH tunnel row from the status file (absent when no `tunnel:` is running).
#[derive(Debug, Clone, PartialEq)]
pub struct TunnelRow {
    pub state: String,
    pub local_port: u64,
    pub remote: Option<String>,
    pub ssh_host: String,
}

/// Parsed, liveness-checked view of `.logs/.tui-status.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub pid: i32,
    /// `runner` (headless / `--detach`, attachable) or `tui` (owns a terminal).
    pub mode: String,
    /// TUI clients attached to a runner.
    pub clients: u64,
    pub age_secs: u64,
    pub services: Vec<ServiceRow>,
    pub tunnel: Option<TunnelRow>,
}

/// Read the status file of a live atlas run owning `root_dir`.
/// Returns Err(reason) when nothing is running (missing, dead pid, or stale file).
pub fn live_snapshot(root_dir: &Path) -> Result<Snapshot, String> {
    let path = crate::runtime::log_dir(root_dir).join(".tui-status.json");
    let text = std::fs::read_to_string(&path).map_err(|_| "no status file".to_string())?;
    let v: Value =
        serde_json::from_str(&text).map_err(|e| format!("unreadable status file: {e}"))?;

    let pid = v["pid"].as_i64().unwrap_or(0) as i32;
    if pid <= 0 || !atlas_core::sys::pid_alive(pid as u32) {
        return Err(format!("process {pid} is not alive"));
    }

    let now = crate::runtime::clock::now_secs();
    let Some(updated_at) = v["updated_at"].as_u64() else {
        return Err("status file has no heartbeat (written by an older atlas)".into());
    };
    let age_secs = now.saturating_sub(updated_at);
    if age_secs > STALE_AFTER_SECS {
        return Err(format!("status is stale (last heartbeat {age_secs}s ago)"));
    }

    let services = v["services"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(name, s)| ServiceRow {
                    name: name.clone(),
                    state: s["state"].as_str().unwrap_or("?").to_string(),
                    port: s["port"].as_u64(),
                    health: s["health"].as_str().map(str::to_string),
                    exit_code: s["exit_code"].as_i64(),
                    restarts: s["restarts"].as_u64().unwrap_or(0),
                    uptime_s: s["uptime_s"].as_u64(),
                    mem_mb: s["mem_mb"].as_u64(),
                    cpu: s["cpu"].as_u64(),
                    started_at: s["started_at"].as_u64(),
                    note: s["note"].as_str().map(str::to_string),
                    off: s["deselected"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    let tunnel = v["tunnel"].as_object().map(|t| TunnelRow {
        state: t
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string(),
        local_port: t.get("local_port").and_then(Value::as_u64).unwrap_or(0),
        remote: t.get("remote").and_then(Value::as_str).map(str::to_string),
        ssh_host: t
            .get("ssh_host")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string(),
    });
    Ok(Snapshot {
        pid,
        // Files from before runner mode were always written by a TUI or headless run.
        mode: v["mode"].as_str().unwrap_or("tui").to_string(),
        clients: v["clients"].as_u64().unwrap_or(0),
        age_secs,
        services,
        tunnel,
    })
}

/// Render the status table when a live atlas run owns `root_dir`.
/// Returns Err(reason) when nothing is running (missing, dead pid, or stale file).
pub fn live_status(root_dir: &Path) -> Result<String, String> {
    let snap = live_snapshot(root_dir)?;
    let mut out = format!(
        "atlas dev running (pid {}, updated {}s ago)\n",
        snap.pid, snap.age_secs
    );
    out.push_str(&format!("mode: {}", snap.mode));
    if snap.mode == "runner" {
        out.push_str(&format!(" · {} attached", snap.clients));
    }
    out.push('\n');
    out.push_str(&format!(
        "{:<20} {:<9} {:<6} {}\n",
        "SERVICE", "STATE", "PORT", "HEALTH"
    ));
    for s in &snap.services {
        let port = s.port.map_or("-".to_string(), |p| p.to_string());
        out.push_str(&format!(
            "{:<20} {:<9} {:<6} {}\n",
            s.name,
            if s.off { "off" } else { s.state.as_str() },
            port,
            s.health.as_deref().unwrap_or("-"),
        ));
    }
    if let Some(t) = &snap.tunnel {
        out.push_str(&format!(
            "{:<20} {:<9} {:<6} {} -> {}\n",
            "tunnel",
            t.state,
            t.local_port,
            t.ssh_host,
            t.remote.as_deref().unwrap_or("-"),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_status(dir: &Path, pid: u32, updated_at: u64) {
        std::fs::create_dir_all(dir.join(".logs")).unwrap();
        let body = serde_json::json!({
            "pid": pid,
            "updated_at": updated_at,
            "services": { "api": { "state": "running", "port": 3000, "health": null } },
        });
        std::fs::write(dir.join(".logs/.tui-status.json"), body.to_string()).unwrap();
    }

    fn now() -> u64 {
        crate::runtime::clock::now_secs()
    }

    #[test]
    fn tunnel_row_is_parsed_and_rendered() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
        let body = serde_json::json!({
            "pid": std::process::id(),
            "updated_at": now(),
            "services": {},
            "tunnel": {"state": "running", "local_port": 54320,
                       "remote": "localhost:5432", "ssh_host": "prod"},
        });
        std::fs::write(dir.path().join(".logs/.tui-status.json"), body.to_string()).unwrap();
        let snap = live_snapshot(dir.path()).unwrap();
        let t = snap.tunnel.unwrap();
        assert_eq!((t.state.as_str(), t.local_port), ("running", 54320));
        let out = live_status(dir.path()).unwrap();
        assert!(
            out.contains("tunnel") && out.contains("prod -> localhost:5432"),
            "{out}"
        );
    }

    #[test]
    fn mode_and_clients_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
        let body = serde_json::json!({
            "pid": std::process::id(),
            "updated_at": now(),
            "mode": "runner",
            "clients": 2,
            "services": {},
        });
        std::fs::write(dir.path().join(".logs/.tui-status.json"), body.to_string()).unwrap();
        let snap = live_snapshot(dir.path()).unwrap();
        assert_eq!((snap.mode.as_str(), snap.clients), ("runner", 2));
        let out = live_status(dir.path()).unwrap();
        assert!(out.contains("mode: runner · 2 attached"), "{out}");
    }

    #[test]
    fn old_status_file_without_mode_is_a_tui() {
        let dir = tempfile::tempdir().unwrap();
        write_status(dir.path(), std::process::id(), now());
        let snap = live_snapshot(dir.path()).unwrap();
        assert_eq!(snap.mode, "tui");
        assert!(live_status(dir.path()).unwrap().contains("mode: tui\n"));
    }

    #[test]
    fn null_tunnel_is_none() {
        let dir = tempfile::tempdir().unwrap();
        write_status(dir.path(), std::process::id(), now());
        assert!(live_snapshot(dir.path()).unwrap().tunnel.is_none());
    }

    #[test]
    fn deselected_service_is_marked_off() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".logs")).unwrap();
        let body = serde_json::json!({
            "pid": std::process::id(),
            "updated_at": now(),
            "services": {
                "api": { "state": "running", "port": 3000, "deselected": false },
                "worker": { "state": "disabled", "port": null, "deselected": true },
            },
        });
        std::fs::write(dir.path().join(".logs/.tui-status.json"), body.to_string()).unwrap();
        let snap = live_snapshot(dir.path()).unwrap();
        assert!(!snap.services.iter().find(|s| s.name == "api").unwrap().off);
        assert!(
            snap.services
                .iter()
                .find(|s| s.name == "worker")
                .unwrap()
                .off
        );
        let out = live_status(dir.path()).unwrap();
        let w = out.lines().find(|l| l.starts_with("worker")).unwrap();
        assert!(w.contains("off") && !w.contains("disabled"), "{w}");
    }

    #[test]
    fn missing_file_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        assert!(live_status(dir.path()).is_err());
    }

    #[test]
    fn fresh_file_with_live_pid_is_running() {
        let dir = tempfile::tempdir().unwrap();
        write_status(dir.path(), std::process::id(), now());
        let out = live_status(dir.path()).unwrap();
        assert!(out.contains("api") && out.contains("running") && out.contains("3000"));
    }

    #[test]
    fn stale_file_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        write_status(dir.path(), std::process::id(), now() - 500);
        assert!(live_status(dir.path()).unwrap_err().contains("stale"));
    }

    #[test]
    fn dead_pid_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        write_status(dir.path(), i32::MAX as u32, now());
        assert!(live_status(dir.path()).unwrap_err().contains("not alive"));
    }
}
