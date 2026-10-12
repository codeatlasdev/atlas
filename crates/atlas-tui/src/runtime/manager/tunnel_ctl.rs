//! `ServiceManager` side of the SSH tunnel: boot start, on-demand start for
//! `open_tunnel: true` actions, status snapshot, exit policy.

use std::sync::{Arc, Mutex};

use super::ServiceManager;
use crate::runtime::service::ServiceState;
use crate::runtime::tunnel::{self, TunnelHandle, TunnelStatus};

/// Who is using the tunnel besides the boot supervisor.
#[derive(Debug, Default)]
pub(crate) struct TunnelLeases {
    /// Running `open_tunnel` actions holding the tunnel.
    refs: usize,
    /// The supervisor was created on demand by an action (so an action may close
    /// it); false for a boot-enabled tunnel, which actions never close.
    on_demand: bool,
}

/// One `open_tunnel` action's claim on the tunnel.
pub(crate) struct TunnelLease {
    pub(crate) handle: TunnelHandle,
    leases: Arc<Mutex<TunnelLeases>>,
    events: tokio::sync::mpsc::UnboundedSender<super::ManagerEvent>,
}

impl TunnelLease {
    /// End of the action (success or failure). The last lease on an on-demand
    /// tunnel closes it (SIGTERM of our own ssh, no `.tunnel.pid` left behind).
    pub(crate) async fn release(self) {
        let close = self.leases.lock().is_ok_and(|mut u| {
            u.refs = u.refs.saturating_sub(1);
            let last = u.refs == 0 && u.on_demand;
            if last {
                u.on_demand = false;
            }
            last
        });
        if close {
            self.handle.shutdown(true).await;
            // The supervisor is gone now: this second Stopped event makes the UI /
            // status file observe "no tunnel" (the first one fired mid-shutdown).
            let _ = self.events.send(super::ManagerEvent::TunnelState {
                state: ServiceState::Stopped,
            });
        }
    }
}

impl ServiceManager {
    /// Boot: with `tunnel.enabled: true` start supervising right away. The
    /// task owns every slow step, so this returns immediately.
    pub(crate) fn start_tunnel_if_enabled(&mut self) {
        let Some(cfg) = self.tunnel_cfg.clone() else {
            return;
        };
        if cfg.is_enabled() && self.tunnel.is_none() {
            let _ = std::fs::create_dir_all(&self.log_dir);
            super::rotate_stream_log(&self.log_dir, tunnel::LOG_NAME);
            self.tunnel = Some(TunnelHandle::spawn(
                cfg,
                self.log_dir.clone(),
                self.event_tx.clone(),
                true,
            ));
        }
    }

    /// Handle for an `open_tunnel: true` action, spawning an idle supervisor
    /// when the tunnel is configured but not enabled at boot. `Err` when there
    /// is no `tunnel:` block (validation normally rejects that at load).
    ///
    /// The returned [`TunnelLease`] must be released when the action ends: a
    /// tunnel opened on demand (not boot-enabled, not already up) is closed once
    /// its last lease is released.
    pub(crate) fn tunnel_for_action(&mut self) -> Option<Result<TunnelLease, String>> {
        let Some(cfg) = self.tunnel_cfg.clone() else {
            return Some(Err(
                "action has open_tunnel: true but no tunnel: is configured".into(),
            ));
        };
        // A supervisor shut down by a previous lease release is gone.
        if self.tunnel.as_ref().is_some_and(TunnelHandle::is_closed) {
            self.tunnel = None;
        }
        if self.tunnel.is_none() {
            let _ = std::fs::create_dir_all(&self.log_dir);
            super::rotate_stream_log(&self.log_dir, tunnel::LOG_NAME);
            self.tunnel = Some(TunnelHandle::spawn(
                cfg,
                self.log_dir.clone(),
                self.event_tx.clone(),
                false,
            ));
            if let Ok(mut u) = self.tunnel_leases.lock() {
                u.on_demand = true;
            }
        }
        let handle = self.tunnel.clone()?;
        if let Ok(mut u) = self.tunnel_leases.lock() {
            u.refs += 1;
        }
        Some(Ok(TunnelLease {
            handle,
            leases: Arc::clone(&self.tunnel_leases),
            events: self.event_tx.clone(),
        }))
    }

    /// Current tunnel snapshot, or None when no tunnel supervisor exists (or an
    /// on-demand one has been closed).
    pub fn tunnel_status(&self) -> Option<TunnelStatus> {
        if let Some(m) = &self.mirror {
            return m.tunnel.clone();
        }
        self.tunnel
            .as_ref()
            .filter(|t| !t.is_closed())
            .map(TunnelHandle::status)
    }

    /// Set the exit policy (the TUI `t` key).
    pub fn set_tunnel_kill_on_exit(&mut self, kill: bool) {
        self.tunnel_kill_on_exit = kill;
    }

    pub fn tunnel_kill_on_exit(&self) -> bool {
        self.tunnel_kill_on_exit
    }

    /// Apply the exit policy; called by `stop_all`.
    pub(crate) async fn shutdown_tunnel(&mut self) {
        if let Some(handle) = self.tunnel.take() {
            handle.shutdown(self.tunnel_kill_on_exit).await;
            self.write_status_file();
        }
    }

    /// `tunnel` object for `.tui-status.json`.
    pub(crate) fn tunnel_json(&self) -> serde_json::Value {
        match self.tunnel_status() {
            Some(t) => serde_json::json!({
                "state": t.state,
                "local_port": t.local_port,
                "remote": t.remote,
                "ssh_host": t.ssh_host,
            }),
            None => serde_json::Value::Null,
        }
    }

    /// True while the tunnel is mid-transition (drives the sidebar spinner).
    pub(crate) fn tunnel_transitioning(&self) -> bool {
        self.tunnel_status()
            .is_some_and(|t| t.state == ServiceState::Starting)
    }
}
