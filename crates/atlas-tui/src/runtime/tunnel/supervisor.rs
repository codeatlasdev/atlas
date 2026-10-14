//! `Tunnel`: the state machine owning one `ssh -N -L` child (or an adopted
//! leftover), driven by the manager's 1s tick and by `ensure_up`.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use super::plan::{
    is_ssh_name, kill_pid, net_signature, next_backoff, pid_alive, reason_or,
    should_reconnect_on_net_change, should_reuse, ssh_args, ssh_bin, ssh_identity,
};
use super::{ALERT_NAME, LOG_NAME, PID_FILE, PidRecord, TunnelStatus};
use crate::config::TunnelConfig;
use crate::runtime::health;
use crate::runtime::logs::{LogStream, spawn_line_pump};
use crate::runtime::manager::ManagerEvent;
use crate::runtime::service::ServiceState;

/// How long a freshly spawned ssh gets to open the local port.
const READY_TIMEOUT: Duration = Duration::from_secs(15);

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
        // New session: no controlling terminal, so ssh can never grab the TUI's tty
        // (Windows: no console shared with the TUI).
        atlas_core::sys::detach_tokio(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot run {}: {e}", ssh_bin()))?;
        self.last_err.lock().map(|mut g| g.clear()).ok();

        if let Some(stderr) = child.stderr.take() {
            let tx = self.tx.clone();
            let last = std::sync::Arc::clone(&self.last_err);
            let mut disk = crate::runtime::manager::open_stream_log(&self.log_dir, LOG_NAME);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

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
