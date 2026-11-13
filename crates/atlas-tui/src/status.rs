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
    /// The foreign process holding this service's port (`port_conflict: {pid, cmd, cwd}`).
    pub port_conflict: Option<crate::runtime::port_conflict::PortConflict>,
    /// Readiness column: `None` = no `ready` probe (or not up), `Some(true/false)` = passes/fails.
    pub ready: Option<bool>,
    /// Why the service is `degraded` (failing `ready` probe, or the repeated log line).
    pub probe_error: Option<String>,
    /// Unix seconds the service turned `unhealthy` (None in any other state).
    pub unhealthy_since: Option<u64>,
    /// Liveness misses in a row when it turned `unhealthy`.
    pub unhealthy_misses: Option<u64>,
    /// The last output lines at the moment it turned `unhealthy`.
    pub unhealthy_lines: Vec<String>,
    /// RSS, trend, budget standing and leak suspicion (`memory_*`, `budget_state`).
    pub memory: crate::runtime::manager::MemoryFacts,
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
                    port_conflict: serde_json::from_value(s["port_conflict"].clone()).ok(),
                    ready: s["ready"].as_bool(),
                    probe_error: s["probe_error"].as_str().map(str::to_string),
                    unhealthy_since: s["unhealthy_since"].as_u64(),
                    unhealthy_misses: s["unhealthy_misses"].as_u64(),
                    unhealthy_lines: s["unhealthy_lines"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|l| l.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                    memory: crate::runtime::manager::MemoryFacts::from_json(s),
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

/// What is worth saying about a service's memory, one fact per part: its size, its trend, its
/// budget standing, a suspected leak. Empty when there is nothing notable (over budget, a
/// suspected leak, or a rising trend), so a healthy project's output stays as it was.
pub fn memory_parts(m: &crate::runtime::manager::MemoryFacts) -> Vec<String> {
    use crate::runtime::memory::{Budget, Direction, fmt_size};
    let notable = m.budget != Budget::Ok
        || m.leak.is_some()
        || m.trend.is_some_and(|t| t.direction == Direction::Rising);
    let Some(rss) = m.rss.filter(|_| notable) else {
        return Vec::new();
    };
    let mut parts = vec![format!("memory {}", fmt_size(rss))];
    if let Some(t) = m.trend {
        parts.push(format!("{} {:+} MB/min", t.direction.name(), t.mb_per_min));
    }
    if m.budget != Budget::Ok {
        parts.push(format!("budget {}", m.budget.name()));
    }
    if let Some(leak) = &m.leak {
        parts.push(leak.clone());
    }
    parts
}

/// The memory line of a service in `atlas dev --status`: [`memory_parts`] on one line.
pub fn memory_line(m: &crate::runtime::manager::MemoryFacts) -> Option<String> {
    let parts = memory_parts(m);
    (!parts.is_empty()).then(|| parts.join(" · "))
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
    // READY only for projects that use it, so `--status` stays byte-identical otherwise.
    let with_ready = snap.services.iter().any(|s| s.ready.is_some());
    if with_ready {
        out.push_str(&format!(
            "{:<20} {:<9} {:<6} {:<6} {}\n",
            "SERVICE", "STATE", "PORT", "READY", "HEALTH"
        ));
    } else {
        out.push_str(&format!(
            "{:<20} {:<9} {:<6} {}\n",
            "SERVICE", "STATE", "PORT", "HEALTH"
        ));
    }
    for s in &snap.services {
        let port = s.port.map_or("-".to_string(), |p| p.to_string());
        let state = if s.off { "off" } else { s.state.as_str() };
        let health = s.health.as_deref().unwrap_or("-");
        if with_ready {
            let ready = match s.ready {
                Some(true) => "ok",
                Some(false) => "fail",
                None => "-",
            };
            out.push_str(&format!(
                "{:<20} {:<9} {:<6} {:<6} {}\n",
                s.name, state, port, ready, health
            ));
        } else {
            out.push_str(&format!(
                "{:<20} {:<9} {:<6} {}\n",
                s.name, state, port, health
            ));
        }
        if let Some(e) = &s.probe_error {
            out.push_str(&format!("  ↳ {e}\n"));
        }
        if let Some(since) = s.unhealthy_since {
            let secs = crate::runtime::clock::now_secs().saturating_sub(since);
            out.push_str(&format!(
                "  ↳ unhealthy for {secs}s ({} missed probes); last output:\n",
                s.unhealthy_misses.unwrap_or(0)
            ));
            for l in &s.unhealthy_lines {
                out.push_str(&format!("      {l}\n"));
            }
        }
        if let Some(line) = memory_line(&s.memory) {
            out.push_str(&format!("  ↳ {line}\n"));
        }
        if let (Some(c), Some(p)) = (&s.port_conflict, s.port) {
            out.push_str(&format!("  ! {}\n", c.describe(p as u16)));
        }
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

    fn write_services(dir: &Path, services: serde_json::Value) {
        std::fs::create_dir_all(dir.join(".logs")).unwrap();
        let body = serde_json::json!({
            "pid": std::process::id(), "updated_at": now(), "services": services,
        });
        std::fs::write(dir.join(".logs/.tui-status.json"), body.to_string()).unwrap();
    }

    /// A status file written by an older atlas (no probe keys) parses, and its table has no
    /// READY column: output is byte-identical for projects that never opt in.
    #[test]
    fn no_ready_column_without_probe_facts() {
        let dir = tempfile::tempdir().unwrap();
        write_status(dir.path(), std::process::id(), now());
        let snap = live_snapshot(dir.path()).unwrap();
        let api = &snap.services[0];
        assert_eq!(
            (api.ready, &api.probe_error, api.unhealthy_since),
            (None, &None, None)
        );
        assert!(api.unhealthy_lines.is_empty());
        let out = live_status(dir.path()).unwrap();
        assert!(!out.contains("READY"), "{out}");
    }

    #[test]
    fn degraded_service_shows_ready_column_and_its_probe_error() {
        let dir = tempfile::tempdir().unwrap();
        write_services(
            dir.path(),
            serde_json::json!({
                "api": { "state": "degraded", "port": 3000, "ready": false,
                         "probe_error": "ready probe failed: GET /ready → 500" },
                "web": { "state": "running", "port": 3001, "ready": true },
                "db":  { "state": "running", "port": 5432 },
            }),
        );
        let snap = live_snapshot(dir.path()).unwrap();
        let by = |n: &str| snap.services.iter().find(|s| s.name == n).unwrap();
        assert_eq!(by("api").ready, Some(false));
        assert_eq!(by("web").ready, Some(true));
        assert_eq!(by("db").ready, None);
        let out = live_status(dir.path()).unwrap();
        assert!(out.contains("READY"), "{out}");
        let api = out.lines().find(|l| l.starts_with("api")).unwrap();
        assert!(api.contains("degraded") && api.contains("fail"), "{api}");
        let web = out.lines().find(|l| l.starts_with("web")).unwrap();
        assert!(web.contains("ok"), "{web}");
        let db = out.lines().find(|l| l.starts_with("db")).unwrap();
        assert!(
            !db.contains("ok") && !db.contains("fail"),
            "no probe → no verdict: {db}"
        );
        assert!(
            out.contains("GET /ready → 500"),
            "the error text is shown: {out}"
        );
    }

    #[test]
    fn unhealthy_service_shows_since_and_the_captured_lines() {
        let dir = tempfile::tempdir().unwrap();
        write_services(
            dir.path(),
            serde_json::json!({
                "web": { "state": "unhealthy", "port": 3000,
                         "unhealthy_since": now() - 30, "unhealthy_misses": 3,
                         "unhealthy_lines": ["GET /slow 200 9000ms", "render queue: 412 pending"] },
            }),
        );
        let snap = live_snapshot(dir.path()).unwrap();
        let web = &snap.services[0];
        assert_eq!(web.unhealthy_misses, Some(3));
        assert_eq!(web.unhealthy_lines.len(), 2);
        let out = live_status(dir.path()).unwrap();
        assert!(
            out.contains("unhealthy for 30s") || out.contains("unhealthy for 31s"),
            "{out}"
        );
        assert!(out.contains("render queue: 412 pending"), "{out}");
    }

    /// A status file from an older atlas (no memory keys) reads as nothing known and prints no
    /// memory line: output stays byte-identical for it.
    #[test]
    fn old_status_file_has_no_memory_facts_and_no_memory_line() {
        let dir = tempfile::tempdir().unwrap();
        write_status(dir.path(), std::process::id(), now());
        let snap = live_snapshot(dir.path()).unwrap();
        let m = &snap.services[0].memory;
        assert_eq!(m.rss, None);
        assert_eq!(m.budget, crate::runtime::memory::Budget::Ok);
        assert!(!live_status(dir.path()).unwrap().contains("memory"));
    }

    #[test]
    fn a_rising_service_over_budget_with_a_leak_gets_one_memory_line() {
        let dir = tempfile::tempdir().unwrap();
        write_services(
            dir.path(),
            serde_json::json!({
                "api": { "state": "degraded", "port": 3000,
                         "memory_rss": 2254857830u64 >> 20 << 20, "memory_trend": "rising",
                         "memory_trend_mb_min": 42.0, "budget_state": "warn",
                         "memory_leak": "leak suspected: +600 MB over 12 reloads",
                         "probe_error": "using 2.1 GB, over memory_warn 1.5 GB" },
                "web": { "state": "running", "port": 3001,
                         "memory_rss": 300u64 << 20, "memory_trend": "flat",
                         "memory_trend_mb_min": 0.0, "budget_state": "ok" },
            }),
        );
        let out = live_status(dir.path()).unwrap();
        assert!(
            out.contains(
                "↳ memory 2.1 GB · rising +42 MB/min · budget warn · leak suspected: +600 MB over 12 reloads"
            ),
            "{out}"
        );
        assert!(
            out.contains("using 2.1 GB, over memory_warn 1.5 GB"),
            "{out}"
        );
        let web = out.lines().position(|l| l.starts_with("web")).unwrap();
        assert!(
            !out.lines().nth(web + 1).unwrap_or("").contains("memory"),
            "a flat, in-budget service gets no line: {out}"
        );
    }
}
