//! The tunnel's decisions and OS probes, free of the supervisor's state: what
//! ssh to run, how long to back off, whether a leftover tunnel can be reused,
//! and which process a pid really is.

use std::path::Path;
use std::time::Duration;

use super::{PID_FILE, SSH_BIN_ENV};
use crate::config::TunnelConfig;
use crate::runtime::process;

/// First reconnect delay; doubles up to `reconnect_cooldown`.
pub(super) const BACKOFF_START: Duration = Duration::from_secs(5);

/// The ssh executable: `$ATLAS_SSH_BIN` when set, else `ssh` from PATH.
pub fn ssh_bin() -> String {
    std::env::var(SSH_BIN_ENV)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ssh".to_string())
}

/// Argument vector for the forwarding ssh. `BatchMode` so it can never prompt
/// on a terminal the TUI owns; `ExitOnForwardFailure` so a taken local port is
/// an exit, not a silent no-op.
pub fn ssh_args(cfg: &TunnelConfig) -> Vec<String> {
    vec![
        "-N".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        format!("ServerAliveInterval={}", cfg.keepalive_interval),
        "-o".into(),
        format!("ServerAliveCountMax={}", cfg.keepalive_max),
        "-o".into(),
        "BatchMode=yes".into(),
        "-L".into(),
        format!("{}:{}:{}", cfg.local_port, cfg.remote_host, cfg.remote_port),
        cfg.ssh_host.clone(),
    ]
}

/// Next reconnect delay: 5s, then doubling, capped at `cooldown_secs`
/// (never below the first step).
pub fn next_backoff(prev: Duration, cooldown_secs: u16) -> Duration {
    let cap = Duration::from_secs(u64::from(cooldown_secs)).max(BACKOFF_START);
    if prev.is_zero() {
        BACKOFF_START.min(cap)
    } else {
        (prev * 2).min(cap)
    }
}

/// Contents of `.tunnel.pid` — enough to tell whether a leftover tunnel is the
/// one the current config wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PidRecord {
    pub pid: i32,
    pub ssh_host: String,
    pub local_port: u16,
}

impl PidRecord {
    pub fn render(&self) -> String {
        format!("{}\n{}\n{}\n", self.pid, self.ssh_host, self.local_port)
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        Some(Self {
            pid: lines.next()?.trim().parse().ok()?,
            ssh_host: lines.next()?.trim().to_string(),
            local_port: lines.next()?.trim().parse().ok()?,
        })
    }
}

/// Pure reuse decision for a recorded tunnel: it matches the config, its pid is
/// alive, that pid is an `ssh`, and the local port accepts connections.
pub fn should_reuse(
    rec: &PidRecord,
    cfg: &TunnelConfig,
    pid_alive: bool,
    proc_name: &str,
    port_open: bool,
) -> bool {
    rec.ssh_host == cfg.ssh_host
        && rec.local_port == cfg.local_port
        && rec.pid > 1
        && pid_alive
        && port_open
        && is_ssh_name(proc_name)
}

/// True when the (lowercase) executable path/name is an ssh client.
pub fn is_ssh_name(name: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    atlas_core::sys::is_program(base, "ssh")
}

/// Identity of `pid` for the reuse/kill decision: its executable name, except
/// under a test-only `ATLAS_SSH_BIN` override whose script interpreter hides
/// the script name — there the command line must name the override binary.
pub(super) fn ssh_identity(pid: i32) -> String {
    let name = process::proc_name(pid);
    if is_ssh_name(&name) {
        return name;
    }
    let Some(over) = std::env::var(SSH_BIN_ENV).ok().filter(|b| !b.is_empty()) else {
        return name;
    };
    let args = atlas_core::sys::command_line(pid as u32).unwrap_or_default();
    if args.split_whitespace().any(|a| a == over) {
        "ssh".to_string()
    } else {
        name
    }
}

/// Last non-empty line of ssh's stderr, for the alert text.
pub(super) fn reason_or(last: &str, fallback: &str) -> String {
    let last = last.trim();
    if last.is_empty() {
        fallback.to_string()
    } else {
        last.to_string()
    }
}

pub(super) fn pid_alive(pid: i32) -> bool {
    pid > 1 && atlas_core::sys::pid_alive(pid as u32)
}

/// SIGTERM exactly `pid` (never a group, never by port).
pub(super) fn kill_pid(pid: i32) {
    if pid > 1 {
        atlas_core::sys::terminate(pid as u32);
    }
}

/// Network fingerprint: default-route gateway line (macOS `route`), or empty
/// where unavailable (then network changes are simply never detected).
pub(super) fn net_signature() -> String {
    std::process::Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8(o.stdout).ok().and_then(|s| {
                s.lines()
                    .find(|l| l.trim_start().starts_with("gateway:"))
                    .map(|l| l.trim().to_string())
            })
        })
        .unwrap_or_default()
}

/// A change only counts when both signatures are known.
pub fn should_reconnect_on_net_change(prev: &str, cur: &str) -> bool {
    !prev.is_empty() && !cur.is_empty() && prev != cur
}

/// Read the recorded leftover tunnel pid (for tests/diagnostics).
pub fn read_pid_record(log_dir: &Path) -> Option<PidRecord> {
    std::fs::read_to_string(log_dir.join(PID_FILE))
        .ok()
        .and_then(|t| PidRecord::parse(&t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> TunnelConfig {
        TunnelConfig {
            enabled: Some(true),
            local_port: 54320,
            remote_host: "localhost".to_string(),
            remote_port: 5432,
            ssh_host: "test-server".to_string(),
            keepalive_interval: 15,
            keepalive_max: 3,
            reconnect_cooldown: 30,
        }
    }

    #[test]
    fn ssh_args_exact() {
        assert_eq!(
            ssh_args(&cfg()).join(" "),
            "-N -o ExitOnForwardFailure=yes -o ServerAliveInterval=15 \
             -o ServerAliveCountMax=3 -o BatchMode=yes -L 54320:localhost:5432 test-server"
        );
    }

    #[test]
    fn ssh_args_follow_keepalive_config() {
        let mut c = cfg();
        c.keepalive_interval = 7;
        c.keepalive_max = 9;
        let a = ssh_args(&c);
        assert!(a.contains(&"ServerAliveInterval=7".to_string()));
        assert!(a.contains(&"ServerAliveCountMax=9".to_string()));
    }

    #[test]
    fn backoff_doubles_and_caps_at_cooldown() {
        let mut d = Duration::ZERO;
        let mut seen = vec![];
        for _ in 0..6 {
            d = next_backoff(d, 30);
            seen.push(d.as_secs());
        }
        assert_eq!(seen, [5, 10, 20, 30, 30, 30]);
        // A tiny cooldown never goes below the first step.
        assert_eq!(next_backoff(Duration::ZERO, 1).as_secs(), 5);
        assert_eq!(next_backoff(Duration::from_secs(5), 1).as_secs(), 5);
    }

    #[test]
    fn pid_record_roundtrip_and_garbage() {
        let r = PidRecord {
            pid: 4242,
            ssh_host: "h".into(),
            local_port: 1234,
        };
        assert_eq!(PidRecord::parse(&r.render()), Some(r));
        assert_eq!(PidRecord::parse(""), None);
        assert_eq!(PidRecord::parse("abc\nh\n1\n"), None);
        assert_eq!(PidRecord::parse("1\nh\n"), None);
    }

    #[test]
    fn reuse_requires_alive_ssh_open_port_and_matching_config() {
        let rec = PidRecord {
            pid: 4242,
            ssh_host: "test-server".into(),
            local_port: 54320,
        };
        let ssh = "/usr/bin/ssh";
        assert!(should_reuse(&rec, &cfg(), true, ssh, true));
        // dead pid
        assert!(!should_reuse(&rec, &cfg(), false, ssh, true));
        // pid recycled into something that is not ssh
        assert!(!should_reuse(&rec, &cfg(), true, "/bin/zsh", true));
        assert!(!should_reuse(&rec, &cfg(), true, "", true));
        // port closed
        assert!(!should_reuse(&rec, &cfg(), true, ssh, false));
        // different host / port than the config now wants
        let mut other = cfg();
        other.ssh_host = "elsewhere".into();
        assert!(!should_reuse(&rec, &other, true, ssh, true));
        let mut other = cfg();
        other.local_port = 1;
        assert!(!should_reuse(&rec, &other, true, ssh, true));
        // never pid 0/1
        let init = PidRecord { pid: 1, ..rec };
        assert!(!should_reuse(&init, &cfg(), true, ssh, true));
    }

    #[test]
    fn ssh_name_matches_basename_only() {
        assert!(is_ssh_name("/usr/bin/ssh"));
        assert!(is_ssh_name("ssh"));
        assert!(!is_ssh_name("/usr/bin/sshd"));
        assert!(!is_ssh_name("/usr/bin/ssh-agent"));
        assert!(!is_ssh_name(""));
    }

    #[test]
    fn net_change_needs_both_signatures() {
        assert!(should_reconnect_on_net_change("gw: 1", "gw: 2"));
        assert!(!should_reconnect_on_net_change("gw: 1", "gw: 1"));
        assert!(!should_reconnect_on_net_change("", "gw: 2"));
        assert!(!should_reconnect_on_net_change("gw: 1", ""));
    }
}
