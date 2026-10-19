use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use super::disk_log::{self, action_service_name, log_file_stem};
use super::{ManagerEvent, ServiceManager};
use crate::config::ActionConfig;
use crate::runtime::logs::LogStream;
use crate::runtime::service::ServiceState;

impl ServiceManager {
    /// Find the action bound to `key` (cloned).
    pub fn find_action(&self, key: char) -> Option<ActionConfig> {
        self.actions.iter().find(|a| a.key == key).cloned()
    }

    /// Run the atlas.yaml action bound to `key`:
    /// 1. stop pause_services, 2. run command in background,
    /// 3. send ActionCompleted so TUI can restart services + show result.
    pub async fn run_action(&mut self, key: char) -> Option<String> {
        let action = self.find_action(key)?;

        // Session actions go through run_session instead
        if action.session {
            return self.run_session(key).await;
        }

        // Same action already running: a second copy would resume (restart) the
        // paused services while the first is still working against them.
        if !self.try_begin_action(&action.label) {
            return None;
        }

        // Suspend services before the action runs — Suspended is intentional,
        // unlike Stopped (which signals an error). Health checks skip Suspended.
        // Only services that were live are paused — and later resumed.
        let resume = self
            .pause_for_action(&action.pause_services, Some(&action.label))
            .await;

        // Run command in background; notify TUI when done
        let command = action.command.clone();
        let root_dir = self.root_dir.clone();
        let tx = self.event_tx.clone();
        let label = action.label.clone();
        let file_env = self.file_env.clone();
        let log_dir = self.log_dir.clone();
        let in_flight = Arc::clone(&self.actions_in_flight);
        // `open_tunnel: true`: the tunnel must be up before the command runs.
        // Bringing it up happens inside the spawned task (never blocks the UI).
        let tunnel = if action.open_tunnel {
            self.tunnel_for_action()
        } else {
            None
        };
        let mut guard = ActionGuard {
            in_flight,
            label: label.clone(),
            tx: tx.clone(),
            done: false,
        };
        tokio::spawn(async move {
            let (gate, lease) = match tunnel {
                Some(Ok(lease)) => (
                    tunnel_gate(Some(&lease.handle), &label, &tx).await,
                    Some(lease),
                ),
                Some(Err(reason)) => (Err(reason), None),
                None => (Ok(()), None),
            };
            let ok = match gate {
                Ok(()) => {
                    run_action_streaming(&command, &root_dir, &file_env, &log_dir, &label, &tx)
                        .await
                }
                Err(reason) => {
                    let _ = tx.send(ManagerEvent::ActionProgress {
                        label: label.clone(),
                        line: format!("tunnel unavailable: {reason}"),
                    });
                    false
                }
            };
            // An on-demand tunnel closes with the action, success or failure.
            if let Some(lease) = lease {
                lease.release().await;
            }
            // Release before notifying so the TUI can re-trigger on ActionCompleted.
            guard.done = true;
            end_action(&guard.in_flight, &label);
            let _ = tx.send(ManagerEvent::ActionCompleted { label, ok, resume });
        });

        Some(action.label.clone())
    }

    /// True if `label` is currently running (non-session action).
    pub fn action_running(&self, label: &str) -> bool {
        self.actions_in_flight
            .lock()
            .is_ok_and(|set| set.contains(label))
    }

    /// Stop and suspend the `names` that are Running/Starting; returns exactly
    /// those (the ones to resume later). Stopped/Failed/Disabled services are
    /// left alone. `label` marks who paused them (status UI).
    pub(super) async fn pause_for_action(
        &mut self,
        names: &[String],
        label: Option<&str>,
    ) -> Vec<String> {
        let live: Vec<String> = names
            .iter()
            .filter(|n| {
                self.services.get(n.as_str()).is_some_and(|s| {
                    matches!(
                        s.status.state,
                        ServiceState::Running | ServiceState::Starting
                    )
                })
            })
            .cloned()
            .collect();
        for name in &live {
            self.stop_service(name).await;
            self.set_state_if_changed(name, ServiceState::Suspended);
            if let (Some(label), Some(svc)) = (label, self.services.get_mut(name)) {
                svc.sched.paused_by = Some(label.to_string());
            }
        }
        live
    }

    /// Mark `label` in flight; false when it already was.
    pub(super) fn try_begin_action(&self, label: &str) -> bool {
        self.actions_in_flight
            .lock()
            .is_ok_and(|mut set| set.insert(label.to_string()))
    }
}

/// Release `label` from the in-flight set.
pub(super) fn end_action(in_flight: &Mutex<HashSet<String>>, label: &str) {
    if let Ok(mut set) = in_flight.lock() {
        set.remove(label);
    }
}

/// Releases an action's in-flight slot if its task dies before finishing
/// (panic, abort): otherwise the label stays blocked forever.
pub(super) struct ActionGuard {
    pub(super) in_flight: Arc<Mutex<HashSet<String>>>,
    pub(super) label: String,
    pub(super) tx: mpsc::UnboundedSender<ManagerEvent>,
    pub(super) done: bool,
}

impl Drop for ActionGuard {
    fn drop(&mut self) {
        if !self.done {
            end_action(&self.in_flight, &self.label);
            let _ = self.tx.send(ManagerEvent::ActionCompleted {
                label: std::mem::take(&mut self.label),
                ok: false,
                resume: vec![],
            });
        }
    }
}

/// Gate for `open_tunnel: true` actions: Ok when no tunnel is required or it
/// came up; Err(reason) otherwise (the command must then NOT run).
async fn tunnel_gate(
    tunnel: Option<&crate::runtime::tunnel::TunnelHandle>,
    label: &str,
    tx: &mpsc::UnboundedSender<ManagerEvent>,
) -> Result<(), String> {
    let Some(handle) = tunnel else {
        return Ok(());
    };
    let _ = tx.send(ManagerEvent::ActionProgress {
        label: label.to_string(),
        line: "opening tunnel…".to_string(),
    });
    handle.ensure_up().await
}

/// Run a non-session action to completion. Each non-empty (ANSI-stripped)
/// stdout/stderr line becomes an `ActionProgress` (operation row), a `LogLine`
/// under `action:<label>` (All tab) and an entry in `.logs/action-<slug>.log`
/// (previous run rotated to `.prev.log`). Returns true on exit 0.
pub(super) async fn run_action_streaming(
    command: &str,
    root_dir: &std::path::Path,
    file_env: &HashMap<String, String>,
    log_dir: &std::path::Path,
    label: &str,
    tx: &mpsc::UnboundedSender<ManagerEvent>,
) -> bool {
    use tokio::io::AsyncRead;

    let service = action_service_name(label);
    let stem = log_file_stem(&service);
    let (log_out, log_err) = disk_log::open_service_writers(log_dir, &stem);

    let mut cmd = atlas_core::sys::shell_tokio(command);
    atlas_core::sys::CommandExt::no_window(&mut cmd);
    cmd.current_dir(root_dir)
        .envs(atlas_core::login_env::vars())
        .envs(file_env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let Ok(mut child) = cmd.spawn() else {
        let msg = format!("failed to spawn: {command}");
        if let Some(mut w) = log_out {
            w.write_line(LogStream::Stderr, &msg);
        }
        let _ = tx.send(ManagerEvent::LogLine {
            name: service,
            line: msg,
            stream: LogStream::Stderr,
        });
        return false;
    };

    fn pump(
        stream: impl AsyncRead + Unpin + Send + 'static,
        label: String,
        service: String,
        kind: LogStream,
        mut log: Option<disk_log::LogWriter>,
        tx: mpsc::UnboundedSender<ManagerEvent>,
    ) -> tokio::task::JoinHandle<()> {
        crate::runtime::logs::spawn_line_pump(stream, move |line| {
            let line = crate::runtime::text::strip_ansi(&line);
            if line.trim().is_empty() {
                return;
            }
            if let Some(w) = log.as_mut() {
                w.write_line(kind, &line);
            }
            let _ = tx.send(ManagerEvent::LogLine {
                name: service.clone(),
                line: line.clone(),
                stream: kind,
            });
            let _ = tx.send(ManagerEvent::ActionProgress {
                label: label.clone(),
                line,
            });
        })
    }

    let readers: Vec<_> = [
        child.stdout.take().map(|s| {
            pump(
                s,
                label.to_string(),
                service.clone(),
                LogStream::Stdout,
                log_out,
                tx.clone(),
            )
        }),
        child.stderr.take().map(|s| {
            pump(
                s,
                label.to_string(),
                service.clone(),
                LogStream::Stderr,
                log_err,
                tx.clone(),
            )
        }),
    ]
    .into_iter()
    .flatten()
    .collect();

    let ok = child.wait().await.map(|s| s.success()).unwrap_or(false);
    drain_pumps(readers).await;
    ok
}

/// After the child exited: give each pump a moment to flush buffered output,
/// then abort it. A background grandchild can keep the pipes open forever.
pub(super) async fn drain_pumps(pumps: Vec<tokio::task::JoinHandle<()>>) {
    for mut p in pumps {
        if tokio::time::timeout(std::time::Duration::from_secs(1), &mut p)
            .await
            .is_err()
        {
            p.abort();
        }
    }
}
