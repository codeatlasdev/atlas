//! SSH port-forward lifecycle.
//!
//! One `ssh -N -L local:remote_host:remote_port ssh_host` child owned by the
//! `ServiceManager`. The manager drives it from its 1s health tick (`tick`) and
//! on demand (`ensure_up`, used by `open_tunnel: true` actions). Nothing here
//! ever kills by port or by `pkill` pattern: only the pid we spawned, or a
//! pid recorded in `.logs/.tunnel.pid` that is verified to still be an `ssh`
//! forwarding the same port.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::logs::spawn_line_pump;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use super::health;
use super::logs::LogStream;
use super::manager::ManagerEvent;
use super::service::ServiceState;
use crate::config::TunnelConfig;

/// Name of the tunnel's log stream / tab / disk log (`.logs/tunnel.log`).
pub const LOG_NAME: &str = "tunnel";
/// Alert + event source name for tunnel failures.
pub const ALERT_NAME: &str = "tunnel";
/// Pid record of a tunnel intentionally left running on exit.
pub const PID_FILE: &str = ".tunnel.pid";
/// Env var overriding the ssh binary (tests only — see docs/runtime.md).
pub const SSH_BIN_ENV: &str = "ATLAS_SSH_BIN";
/// First reconnect delay; doubles up to `reconnect_cooldown`.
const BACKOFF_START: Duration = Duration::from_secs(5);
/// How long a freshly spawned ssh gets to open the local port.
const READY_TIMEOUT: Duration = Duration::from_secs(15);

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
    name.rsplit('/').next().unwrap_or(name) == "ssh"
}

/// Identity of `pid` for the reuse/kill decision: its executable name, except
/// under a test-only `ATLAS_SSH_BIN` override whose script interpreter hides
/// the script name — there the command line must name the override binary.
fn ssh_identity(pid: i32) -> String {
    let name = super::process::proc_name(pid);
    if is_ssh_name(&name) {
        return name;
    }
    let Some(over) = std::env::var(SSH_BIN_ENV).ok().filter(|b| !b.is_empty()) else {
        return name;
    };
    let args = std::process::Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    if args.split_whitespace().any(|a| a == over) {
        "ssh".to_string()
    } else {
        name
    }
}

/// Last non-empty line of ssh's stderr, for the alert text.
fn reason_or(last: &str, fallback: &str) -> String {
    let last = last.trim();
    if last.is_empty() {
        fallback.to_string()
    } else {
        last.to_string()
    }
}

/// Snapshot published to the status file / MCP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelStatus {
    pub state: ServiceState,
    pub local_port: u16,
    pub remote: String,
    pub ssh_host: String,
}

pub struct Tunnel {
    cfg: TunnelConfig,
    log_dir: PathBuf,
    tx: mpsc::UnboundedSender<ManagerEvent>,
    state: ServiceState,
    /// ssh we spawned this run.
    child: Option<Child>,
    /// Reused leftover from a previous run (not our child; never reaped by us).
    adopted_pid: Option<i32>,
    /// stderr pump of `child`; aborted when the ssh is stopped or replaced.
    pump: Option<tokio::task::JoinHandle<()>>,
    last_attempt: Option<Instant>,
    backoff: Duration,
    /// Last stderr line from ssh (shared with the stderr pump).
    last_err: std::sync::Arc<std::sync::Mutex<String>>,
    /// Alert once per failure episode, not on every retry.
    alerted: bool,
    /// A start/stop is deliberate: `tick` must not reconnect.
    wanted: bool,
    net_sig: String,
}

impl Tunnel {
    pub fn new(
        cfg: TunnelConfig,
        log_dir: PathBuf,
        tx: mpsc::UnboundedSender<ManagerEvent>,
    ) -> Self {
        Self {
            cfg,
            log_dir,
            tx,
            state: ServiceState::Stopped,
            child: None,
            adopted_pid: None,
            pump: None,
            last_attempt: None,
            backoff: Duration::ZERO,
            last_err: Default::default(),
            alerted: false,
            wanted: false,
            net_sig: String::new(),
        }
    }

    pub fn config(&self) -> &TunnelConfig {
        &self.cfg
    }

    pub fn state(&self) -> &ServiceState {
        &self.state
    }

    pub fn status(&self) -> TunnelStatus {
        TunnelStatus {
            state: self.state.clone(),
            local_port: self.cfg.local_port,
            remote: format!("{}:{}", self.cfg.remote_host, self.cfg.remote_port),
            ssh_host: self.cfg.ssh_host.clone(),
        }
    }

    fn pid_path(&self) -> PathBuf {
        self.log_dir.join(PID_FILE)
    }

    fn set_state(&mut self, state: ServiceState) {
        if self.state != state {
            self.state = state.clone();
            let _ = self.tx.send(ManagerEvent::TunnelState { state });
        }
    }

    fn note(&self, line: String) {
        let _ = self.tx.send(ManagerEvent::LogLine {
            name: LOG_NAME.into(),
            line,
            stream: LogStream::Stdout,
        });
    }

    async fn port_open(&self) -> bool {
        health::check_port("127.0.0.1", self.cfg.local_port, Duration::from_secs(1)).await
    }

    /// Adopt the tunnel recorded in `.tunnel.pid` if it is still ours to reuse.
    /// A stale/foreign record is deleted; nothing is killed.
    async fn try_adopt(&mut self) -> bool {
        let Some(rec) = std::fs::read_to_string(self.pid_path())
            .ok()
            .and_then(|t| PidRecord::parse(&t))
        else {
            return false;
        };
        let alive = health::check_process_alive(rec.pid as u32).await && rec.pid > 1;
        let name = if alive {
            ssh_identity(rec.pid)
        } else {
            String::new()
        };
        let open = alive && self.port_open().await;
        if should_reuse(&rec, &self.cfg, alive, &name, open) {
            self.adopted_pid = Some(rec.pid);
            self.note(format!(
                "reusing tunnel from previous run (pid {}, 127.0.0.1:{})",
                rec.pid, rec.local_port
            ));
            true
        } else {
            let _ = std::fs::remove_file(self.pid_path());
            false
        }
    }

    /// Spawn ssh and wait (bounded) for the local port. Never blocks the UI
    /// thread longer than `READY_TIMEOUT`; callers run it from a manager task
    /// that is itself polled by the event loop, or from `ensure_up`.
    async fn spawn_and_wait(&mut self) -> Result<(), String> {
        let mut cmd = Command::new(ssh_bin());
        cmd.args(ssh_args(&self.cfg))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(false);
        // New session: no controlling terminal, so ssh can never grab the TUI's tty.
        // SAFETY: setsid is async-signal-safe and runs in the forked child only.
        unsafe {
            cmd.pre_exec(|| {
                nix::unistd::setsid()
                    .map(|_| ())
                    .map_err(|e| std::io::Error::from_raw_os_error(e as i32))
            });
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot run {}: {e}", ssh_bin()))?;
        self.last_err.lock().map(|mut g| g.clear()).ok();

        if let Some(stderr) = child.stderr.take() {
            let tx = self.tx.clone();
            let last = std::sync::Arc::clone(&self.last_err);
            let mut disk = super::manager::open_stream_log(&self.log_dir, LOG_NAME);
            let pump = spawn_line_pump(stderr, move |line| {
                if let Some(w) = disk.as_mut() {
                    w.write_line(LogStream::Stderr, &line);
                }
                if !line.trim().is_empty()
                    && let Ok(mut g) = last.lock()
                {
                    *g = line.clone();
                }
                let _ = tx.send(ManagerEvent::LogLine {
                    name: LOG_NAME.into(),
                    line,
                    stream: LogStream::Stderr,
                });
            });
            // A reconnect replaces the pump of the previous (dead) ssh.
            if let Some(old) = self.pump.replace(pump) {
                old.abort();
            }
        }
        self.note(format!("{} {}", ssh_bin(), ssh_args(&self.cfg).join(" ")));

        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                // Let the stderr pump publish ssh's last words.
                tokio::time::sleep(Duration::from_millis(100)).await;
                let last = self.last_err.lock().map(|g| g.clone()).unwrap_or_default();
                return Err(reason_or(&last, &format!("ssh exited ({status})")));
            }
            if self.port_open().await {
                break;
            }
            if Instant::now() >= deadline {
                if let Some(pid) = child.id() {
                    kill_pid(pid as i32);
                }
                let _ = child.wait().await;
                return Err(format!(
                    "local port {} did not open within {}s",
                    self.cfg.local_port,
                    READY_TIMEOUT.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        self.child = Some(child);
        Ok(())
    }

    /// Bring the tunnel up (or confirm it is). Returns the failure reason.
    pub async fn ensure_up(&mut self) -> Result<(), String> {
        self.wanted = true;
        if self.owns_live_process() && self.port_open().await {
            self.set_state(ServiceState::Running);
            return Ok(());
        }
        self.set_state(ServiceState::Starting);
        self.last_attempt = Some(Instant::now());
        if self.try_adopt().await {
            self.mark_up();
            return Ok(());
        }
        if self.port_open().await {
            // Something else already serves the port (a tunnel from outside
            // atlas, or a different forward). We cannot own or kill it; use it.
            self.note(format!(
                "127.0.0.1:{} already accepting connections; not starting ssh",
                self.cfg.local_port
            ));
            self.mark_up();
            return Ok(());
        }
        match self.spawn_and_wait().await {
            Ok(()) => {
                self.mark_up();
                Ok(())
            }
            Err(reason) => {
                self.mark_failed(&reason);
                Err(reason)
            }
        }
    }

    fn mark_up(&mut self) {
        self.backoff = Duration::ZERO;
        self.alerted = false;
        self.set_state(ServiceState::Running);
        self.net_sig = net_signature();
    }

    fn mark_failed(&mut self, reason: &str) {
        self.set_state(ServiceState::Failed);
        self.note(format!("tunnel failed: {reason}"));
        if !self.alerted {
            self.alerted = true;
            let _ = self.tx.send(ManagerEvent::CriticalAlert {
                service: ALERT_NAME.into(),
                message: format!("SSH tunnel to {} failed: {reason}", self.cfg.ssh_host),
            });
        }
    }

    /// True while a process we (or a previous run) own backs the tunnel.
    fn owns_live_process(&mut self) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return true,
                _ => self.child = None,
            }
        }
        match self.adopted_pid {
            Some(pid) if pid_alive(pid) => true,
            Some(_) => {
                self.adopted_pid = None;
                false
            }
            None => false,
        }
    }

    /// 1s health pass: detect death / network change, reconnect with backoff.
    /// Does nothing until `ensure_up` has run (tunnel not wanted yet).
    pub async fn tick(&mut self) {
        if !self.wanted {
            return;
        }
        let sig = net_signature();
        let net_changed = should_reconnect_on_net_change(&self.net_sig, &sig);
        if !sig.is_empty() {
            self.net_sig = sig;
        }
        let process_ok = self.owns_live_process();
        let port_ok = self.port_open().await;
        if process_ok && port_ok && !net_changed {
            self.backoff = Duration::ZERO;
            self.set_state(ServiceState::Running);
            return;
        }
        if net_changed && process_ok {
            self.note("network changed; reconnecting tunnel".into());
            self.stop_owned().await;
        }
        if !process_ok && port_ok && self.child.is_none() && self.adopted_pid.is_none() {
            // Someone else's listener (see ensure_up): healthy as long as it answers.
            self.set_state(ServiceState::Running);
            return;
        }
        // Dead (or just torn down): respect backoff unless the network moved.
        if !net_changed
            && let Some(last) = self.last_attempt
            && last.elapsed() < self.backoff
        {
            self.set_state(ServiceState::Failed);
            return;
        }
        self.backoff = next_backoff(self.backoff, self.cfg.reconnect_cooldown);
        self.note(format!(
            "tunnel down; reconnecting (next retry in {}s if this fails)",
            self.backoff.as_secs()
        ));
        let _ = self.ensure_up().await;
    }

    /// SIGTERM the ssh we own (our child, or an adopted pid) and reap it.
    async fn stop_owned(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Some(pid) = child.id() {
                kill_pid(pid as i32);
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill().await;
            }
        }
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
        if let Some(pid) = self.adopted_pid.take()
            && is_ssh_name(&ssh_identity(pid))
        {
            kill_pid(pid);
        }
        let _ = std::fs::remove_file(self.pid_path());
    }

    /// Shutdown. `kill == true`: SIGTERM our ssh. Otherwise leave it running and
    /// record it in `.tunnel.pid` for the next start to reuse.
    pub async fn shutdown(&mut self, kill: bool) {
        self.wanted = false;
        if kill {
            self.stop_owned().await;
            self.set_state(ServiceState::Stopped);
            return;
        }
        let pid = self
            .child
            .as_ref()
            .and_then(Child::id)
            .map(|p| p as i32)
            .or(self.adopted_pid);
        match pid {
            Some(pid) if pid_alive(pid) => {
                let _ = std::fs::create_dir_all(&self.log_dir);
                let rec = PidRecord {
                    pid,
                    ssh_host: self.cfg.ssh_host.clone(),
                    local_port: self.cfg.local_port,
                };
                let _ = std::fs::write(self.pid_path(), rec.render());
                // Detach: dropping a tokio Child never kills it (kill_on_drop(false)).
                self.child = None;
                self.adopted_pid = None;
            }
            _ => {
                let _ = std::fs::remove_file(self.pid_path());
            }
        }
        self.set_state(ServiceState::Stopped);
    }
}

// ── background actor ─────────────────────────────────────────────────────

enum Cmd {
    /// Ensure the tunnel is up; answer with the failure reason on error.
    EnsureUp(tokio::sync::oneshot::Sender<Result<(), String>>),
    /// Stop supervising. `kill`: SIGTERM our ssh; else leave it + `.tunnel.pid`.
    Shutdown {
        kill: bool,
        done: tokio::sync::oneshot::Sender<()>,
    },
}

/// Handle to the tunnel task. All slow work (ssh spawn, readiness wait,
/// reconnect backoff) happens on that task, so neither the TUI loop nor the
/// manager's health tick ever waits on it.
#[derive(Clone)]
pub struct TunnelHandle {
    tx: mpsc::UnboundedSender<Cmd>,
    status: tokio::sync::watch::Receiver<TunnelStatus>,
    cfg: TunnelConfig,
}

impl TunnelHandle {
    /// Spawn the supervisor. `autostart`: bring the tunnel up immediately
    /// (`tunnel.enabled: true`); otherwise it idles until `ensure_up`.
    pub fn spawn(
        cfg: TunnelConfig,
        log_dir: PathBuf,
        events: mpsc::UnboundedSender<ManagerEvent>,
        autostart: bool,
    ) -> Self {
        let mut tunnel = Tunnel::new(cfg.clone(), log_dir, events);
        let (status_tx, status_rx) = tokio::sync::watch::channel(tunnel.status());
        let (tx, mut rx) = mpsc::unbounded_channel::<Cmd>();
        tokio::spawn(async move {
            if autostart {
                let _ = tunnel.ensure_up().await;
                let _ = status_tx.send(tunnel.status());
            }
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    cmd = rx.recv() => match cmd {
                        Some(Cmd::EnsureUp(reply)) => {
                            let r = tunnel.ensure_up().await;
                            let _ = status_tx.send(tunnel.status());
                            let _ = reply.send(r);
                        }
                        Some(Cmd::Shutdown { kill, done }) => {
                            tunnel.shutdown(kill).await;
                            let _ = status_tx.send(tunnel.status());
                            let _ = done.send(());
                            break;
                        }
                        None => {
                            // Handle dropped without shutdown: leave ssh alone.
                            tunnel.shutdown(false).await;
                            break;
                        }
                    },
                    _ = tick.tick() => {
                        tunnel.tick().await;
                        let _ = status_tx.send(tunnel.status());
                    }
                }
            }
        });
        Self {
            tx,
            status: status_rx,
            cfg,
        }
    }

    pub fn config(&self) -> &TunnelConfig {
        &self.cfg
    }

    /// True once the supervisor task has exited (after `shutdown`).
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    pub fn status(&self) -> TunnelStatus {
        self.status.borrow().clone()
    }

    /// Ask the task to bring the tunnel up and wait for the verdict.
    pub async fn ensure_up(&self) -> Result<(), String> {
        let (reply, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(Cmd::EnsureUp(reply))
            .map_err(|_| "tunnel supervisor is not running".to_string())?;
        rx.await
            .unwrap_or_else(|_| Err("tunnel supervisor stopped".to_string()))
    }

    /// Stop supervising and apply the exit policy; returns once done.
    pub async fn shutdown(&self, kill: bool) {
        let (done, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(Cmd::Shutdown { kill, done }).is_ok() {
            let _ = rx.await;
        }
    }
}

fn pid_alive(pid: i32) -> bool {
    pid > 1 && nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

/// SIGTERM exactly `pid` (never a group, never by port).
fn kill_pid(pid: i32) {
    if pid > 1 {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGTERM,
        );
    }
}

/// Network fingerprint: default-route gateway line (macOS `route`), or empty
/// where unavailable (then network changes are simply never detected).
fn net_signature() -> String {
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

    #[tokio::test]
    async fn new_tunnel_is_stopped_and_tick_is_inert_until_wanted() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut t = Tunnel::new(cfg(), dir.path().to_path_buf(), tx);
        assert_eq!(*t.state(), ServiceState::Stopped);
        t.tick().await;
        assert_eq!(*t.state(), ServiceState::Stopped);
        assert!(rx.try_recv().is_err(), "no events before ensure_up");
    }

    #[tokio::test]
    async fn state_change_emits_event_once() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut t = Tunnel::new(cfg(), dir.path().to_path_buf(), tx);
        t.set_state(ServiceState::Starting);
        t.set_state(ServiceState::Starting);
        t.set_state(ServiceState::Running);
        let mut states = vec![];
        while let Ok(ManagerEvent::TunnelState { state }) = rx.try_recv() {
            states.push(state);
        }
        assert_eq!(states, [ServiceState::Starting, ServiceState::Running]);
    }

    #[tokio::test]
    async fn failure_alerts_once_per_episode_then_rearms_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut t = Tunnel::new(cfg(), dir.path().to_path_buf(), tx);
        t.mark_failed("Permission denied (publickey).");
        t.mark_failed("Permission denied (publickey).");
        let mut alerts = 0;
        while let Ok(ev) = rx.try_recv() {
            if let ManagerEvent::CriticalAlert { service, message } = ev {
                assert_eq!(service, "tunnel");
                assert!(message.contains("Permission denied"), "{message}");
                alerts += 1;
            }
        }
        assert_eq!(alerts, 1);
        t.mark_up();
        assert!(!t.alerted);
        assert_eq!(*t.state(), ServiceState::Running);
    }

    #[tokio::test]
    async fn stale_pid_record_is_removed_not_reused() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut t = Tunnel::new(cfg(), dir.path().to_path_buf(), tx);
        // i32::MAX is not a live pid.
        std::fs::write(
            dir.path().join(PID_FILE),
            PidRecord {
                pid: i32::MAX,
                ssh_host: "test-server".into(),
                local_port: 54320,
            }
            .render(),
        )
        .unwrap();
        assert!(!t.try_adopt().await);
        assert!(!dir.path().join(PID_FILE).exists());
    }

    #[tokio::test]
    async fn live_non_ssh_pid_record_is_never_adopted_or_killed() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut t = Tunnel::new(cfg(), dir.path().to_path_buf(), tx);
        // Our own test process: alive, but not ssh.
        std::fs::write(
            dir.path().join(PID_FILE),
            PidRecord {
                pid: std::process::id() as i32,
                ssh_host: "test-server".into(),
                local_port: 54320,
            }
            .render(),
        )
        .unwrap();
        assert!(!t.try_adopt().await);
        assert!(t.adopted_pid.is_none());
    }
}
