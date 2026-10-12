//! Attached mode: this `App` is a client of a runner (`atlas dev` while
//! `atlas dev --detach` is running). Rendering and input are untouched; only the
//! data source changes:
//!
//! * `app.manager` is a *mirror* (`ServiceManager::into_mirror`): config-derived
//!   data as usual, state and facts fed from the runner.
//! * the runner's event stream is turned into ordinary [`ManagerEvent`]s on the
//!   channel the App already reads, so `apply_manager_event` is shared.
//! * effects that would act on services (restart, stop, run action) become
//!   control requests instead of manager calls.

use std::path::PathBuf;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::control::{self, Subscription};
use crate::runner::parse_state;
use crate::runtime::logs::LogStream;
use crate::runtime::manager::{ManagerEvent, MirrorFacts};
use crate::runtime::service::ServiceState;
use crate::runtime::tunnel::TunnelStatus;
use crate::tui::widgets::events::EventKind;

use super::{App, Operation, ToastVariant};

/// How an attached client ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    /// Leave the runner alone (`D`, `q` → Detach).
    Detach,
    /// The runner was asked to stop everything (`q` → Stop everything).
    Stopped,
    /// The runner went away on its own.
    RunnerGone,
}

/// Connection state of an attached client.
pub struct Remote {
    pub root: PathBuf,
    /// Failures of fire-and-forget requests, shown as toasts by the loop.
    pub errors_rx: mpsc::UnboundedReceiver<String>,
    errors_tx: mpsc::UnboundedSender<String>,
    pub leave: Option<Leave>,
    pub runner_pid: i64,
}

impl Remote {
    pub fn new(root: PathBuf, runner_pid: i64) -> Self {
        let (errors_tx, errors_rx) = mpsc::unbounded_channel();
        Self {
            root,
            errors_rx,
            errors_tx,
            leave: None,
            runner_pid,
        }
    }

    /// Send a control request without blocking the UI; a refusal comes back as a toast.
    pub fn request(&self, cmd: &'static str, args: Value) {
        let root = self.root.clone();
        let errors = self.errors_tx.clone();
        tokio::spawn(async move {
            match control::call(&root, cmd, args).await {
                Ok(r) if r.ok => {}
                Ok(r) => {
                    let _ = errors.send(r.error.unwrap_or_else(|| format!("{cmd} refused")));
                }
                Err(e) => {
                    let _ = errors.send(e);
                }
            }
        });
    }
}

/// Parse the facts a `state` event / `rows` entry carries.
fn facts(v: &Value) -> MirrorFacts {
    MirrorFacts {
        exit_code: v["exit_code"].as_i64().map(|c| c as i32),
        restarts: v["restarts"].as_u64().unwrap_or(0) as u32,
        note: v["note"].as_str().map(str::to_string),
        completed_ok: v["completed_ok"].as_bool().unwrap_or(false),
        paused_by: v["paused_by"].as_str().map(str::to_string),
        deselected: v["deselected"].as_bool().unwrap_or(false),
    }
}

fn tunnel_status(v: &Value) -> Option<TunnelStatus> {
    let o = v.as_object()?;
    Some(TunnelStatus {
        state: parse_state(o.get("state")?.as_str()?)?,
        local_port: o.get("local_port")?.as_u64()? as u16,
        remote: o.get("remote")?.as_str()?.to_string(),
        ssh_host: o.get("ssh_host")?.as_str()?.to_string(),
    })
}

fn stream_of(v: &Value) -> LogStream {
    if v["stream"] == "stderr" {
        LogStream::Stderr
    } else {
        LogStream::Stdout
    }
}

impl App {
    /// Apply a runner snapshot (`subscribe` reply) before the first frame.
    pub fn apply_snapshot(&mut self, snap: &Value) {
        self.started = snap["started"].as_bool().unwrap_or(false);
        // Attached clients never see a boot: nothing to splash over.
        self.splash_done = true;

        // Side events first (they classify against the facts of their time), then the
        // present state overwrites every row.
        for ev in snap["events"].as_array().into_iter().flatten() {
            self.replay_event(ev);
        }
        self.apply_rows(&snap["services"], &snap["tunnel"]);
        for l in snap["logs"].as_array().into_iter().flatten() {
            self.logs.log_buffer.push_kind(
                l["name"].as_str().unwrap_or_default().to_string(),
                l["line"].as_str().unwrap_or_default().to_string(),
                stream_of(l),
                l["kind"].as_str().unwrap_or_default(),
                l["ts"].as_u64(),
            );
        }
        if let Some(op) = snap["operation"].as_object() {
            let mut o = Operation::start(
                op.get("label")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            );
            o.detail = op
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.operation = Some(o);
        }
        // Tabs the runner added at runtime (on-demand tunnel).
        if snap["tunnel"].is_object() {
            self.ensure_tunnel_tab();
        }
        self.dirty = true;
    }

    fn ensure_tunnel_tab(&mut self) {
        let name = crate::runtime::tunnel::LOG_NAME;
        if !self.tabs.iter().any(|t| t == name) {
            self.tabs.push(name.to_string());
        }
    }

    fn apply_rows(&mut self, services: &Value, tunnel: &Value) {
        for s in services.as_array().into_iter().flatten() {
            let (Some(name), Some(state)) = (
                s["name"].as_str(),
                s["state"].as_str().and_then(parse_state),
            ) else {
                continue;
            };
            self.manager.mirror_set(name, state, facts(s));
        }
        self.manager.mirror_set_tunnel(tunnel_status(tunnel));
    }

    /// History replay: rebuild the sidebar event log only. No toasts, no log lines.
    fn replay_event(&mut self, ev: &Value) {
        match ev["t"].as_str() {
            Some("state") => {
                if let (Some(name), Some(state)) = (
                    ev["name"].as_str(),
                    ev["state"].as_str().and_then(parse_state),
                ) {
                    // Same dedupe as the live path: the health watcher re-emits `running`.
                    let prior = self.manager.get_state(name);
                    if prior.as_ref() != Some(&state) {
                        if let Some(prior) = prior {
                            self.manager.mirror_set(name, prior, facts(ev));
                        }
                        self.push_state_event(name, &state);
                    }
                    self.manager.mirror_set(name, state, facts(ev));
                }
            }
            Some("note") => {
                let (name, msg) = (
                    ev["name"].as_str().unwrap_or_default(),
                    ev["message"].as_str().unwrap_or_default(),
                );
                self.push_event(
                    super::update::classify_note(msg),
                    name.to_string(),
                    msg.to_string(),
                );
            }
            Some("alert") => self.push_event(
                EventKind::Alert,
                ev["service"].as_str().unwrap_or_default().to_string(),
                ev["message"].as_str().unwrap_or_default().to_string(),
            ),
            Some("tunnel") => self.push_event(
                EventKind::Tunnel,
                "tunnel".into(),
                ev["state"].as_str().unwrap_or_default().to_string(),
            ),
            _ => {}
        }
    }

    /// One live stream event → the matching `ManagerEvent` through the shared path.
    /// Returns `false` when the runner said goodbye.
    pub fn apply_stream_event(&mut self, ev: &Value) -> bool {
        let event = match ev["t"].as_str() {
            Some("log") if ev["kind"] == "restart" => ManagerEvent::ServiceRestarted {
                name: ev["name"].as_str().unwrap_or_default().to_string(),
            },
            Some("log") => ManagerEvent::LogLine {
                name: ev["name"].as_str().unwrap_or_default().to_string(),
                line: ev["line"].as_str().unwrap_or_default().to_string(),
                stream: stream_of(ev),
            },
            Some("state") => {
                let (Some(name), Some(state)) = (
                    ev["name"].as_str(),
                    ev["state"].as_str().and_then(parse_state),
                ) else {
                    return true;
                };
                // The runner builds this event before applying it, so its facts are the
                // ones `push_state_event` must classify against (paused_by,
                // completed_ok, exit_code of the *previous* state): keep the old
                // state, load the facts, classify, then move to the new state.
                if let Some(prior) = self.manager.get_state(name) {
                    if prior != state {
                        self.manager.mirror_set(name, prior, facts(ev));
                        self.push_state_event(name, &state);
                    }
                    self.manager.mirror_set(name, state, facts(ev));
                }
                self.dirty = true;
                return true;
            }
            Some("rows") => {
                self.apply_rows(&ev["services"], &ev["tunnel"]);
                self.dirty = true;
                return true;
            }
            Some("note") => ManagerEvent::ServiceNote {
                name: ev["name"].as_str().unwrap_or_default().to_string(),
                message: ev["message"].as_str().unwrap_or_default().to_string(),
            },
            Some("alert") => ManagerEvent::CriticalAlert {
                service: ev["service"].as_str().unwrap_or_default().to_string(),
                message: ev["message"].as_str().unwrap_or_default().to_string(),
            },
            Some("tunnel") => {
                self.manager.mirror_set_tunnel(tunnel_status(&ev["tunnel"]));
                ManagerEvent::TunnelState {
                    state: ev["state"]
                        .as_str()
                        .and_then(parse_state)
                        .unwrap_or(ServiceState::Stopped),
                }
            }
            Some("action_progress") => ManagerEvent::ActionProgress {
                label: ev["label"].as_str().unwrap_or_default().to_string(),
                line: ev["line"].as_str().unwrap_or_default().to_string(),
            },
            Some("action_done") => ManagerEvent::ActionCompleted {
                label: ev["label"].as_str().unwrap_or_default().to_string(),
                ok: ev["ok"].as_bool().unwrap_or(false),
                // The runner resumes paused services itself.
                resume: Vec::new(),
            },
            Some("started") => ManagerEvent::AllStarted,
            Some("bye") => return false,
            _ => return true,
        };
        self.apply_manager_event(event);
        true
    }

    /// Attached: send an effect's service action to the runner. `false` = not a
    /// remote-routable effect (caller handles it locally or drops it).
    pub(super) fn remote_effect(&mut self, effect: &super::messages::Effect) -> bool {
        use super::messages::Effect;
        let Some(remote) = &self.remote else {
            return false;
        };
        match effect {
            Effect::RestartService(name) => {
                remote.request("restart_service", json!({ "service": name }));
                self.set_toast(format!("Restarting {name}"), ToastVariant::Progress);
            }
            Effect::RestartServices(names) => {
                for name in names {
                    remote.request("restart_service", json!({ "service": name }));
                }
                self.set_toast(
                    format!("Restarted {} services", names.len()),
                    ToastVariant::Success,
                );
            }
            Effect::StopService(name) => {
                remote.request("stop_service", json!({ "service": name }));
                self.set_toast(format!("Stopped {name}"), ToastVariant::Success);
            }
            Effect::RunAction(key) => {
                let action = self.manager.find_action(*key);
                match action {
                    Some(a) if a.session || a.platform.is_some() => {
                        self.set_toast(
                            format!(
                                "{} is an interactive session: owner-only. atlas down, then atlas dev",
                                a.label
                            ),
                            ToastVariant::Warning,
                        );
                    }
                    Some(a) => {
                        // The modal (if the action has `confirm:`) already ran client-side.
                        remote.request(
                            "run_action",
                            json!({ "action": key.to_string(), "confirmed": true }),
                        );
                        self.operation = Some(Operation::start(a.label.clone()));
                        self.set_toast(format!("Running {}", a.label), ToastVariant::Progress);
                    }
                    None => {}
                }
            }
            // The runner starts services and runs health checks on its own.
            Effect::StartAppServices(_) | Effect::CheckHealth => {}
            // Sessions and the device picker never arise remotely (`RunAction` refuses them).
            Effect::StopSession(_)
            | Effect::SendSessionInput { .. }
            | Effect::DetectDevices { .. }
            | Effect::RunSessionWithDevice { .. } => {}
            Effect::CopyToClipboard { .. }
            | Effect::OpenUrl(_)
            | Effect::OpenPath { .. }
            | Effect::OpenService(_)
            | Effect::SetMouseCapture(_)
            | Effect::Suspend => return false,
        }
        true
    }

    /// Drain request failures into toasts. Returns whether anything changed.
    pub(super) fn drain_remote_errors(&mut self) -> bool {
        let mut changed = false;
        while let Some(err) = self
            .remote
            .as_mut()
            .and_then(|r| r.errors_rx.try_recv().ok())
        {
            self.set_toast(err, ToastVariant::Error);
            changed = true;
        }
        changed
    }

    /// Detach (`D`, `q` → Detach): leave the runner running.
    pub(super) fn leave_remote(&mut self, leave: Leave) {
        if let Some(r) = self.remote.as_mut() {
            r.leave = Some(leave);
        }
        self.should_quit = true;
    }

    /// `StopRunner`: ask the runner to stop everything, then wait for it in `run`.
    pub(super) fn stop_remote(&mut self) {
        if let Some(r) = &self.remote {
            r.request("shutdown", json!({}));
        }
        self.leave_remote(Leave::Stopped);
    }
}

/// Everything the attached loop receives from the socket reader task.
#[derive(Debug)]
pub enum RemoteEvent {
    Event(Value),
    /// Stream ended (runner gone or connection lost).
    Closed,
}

/// Spawn the reader task: stream lines → channel the App's loop selects on.
pub fn spawn_reader(mut sub: Subscription) -> mpsc::UnboundedReceiver<RemoteEvent> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some(ev) = sub.next().await {
            if tx.send(RemoteEvent::Event(ev)).is_err() {
                return;
            }
        }
        let _ = tx.send(RemoteEvent::Closed);
    });
    rx
}

impl App {
    pub(super) fn handle_remote_event(&mut self, ev: RemoteEvent) {
        match ev {
            RemoteEvent::Event(v) => {
                if !self.apply_stream_event(&v) {
                    self.leave_remote(Leave::RunnerGone);
                }
            }
            RemoteEvent::Closed => {
                // After a requested stop the runner closing is the expected end.
                if self.remote.as_ref().is_some_and(|r| r.leave.is_none()) {
                    self.leave_remote(Leave::RunnerGone);
                }
            }
        }
        self.dirty = true;
    }

    /// `Message::Detach` / `StopRunner` entry points (owner TUI: explain instead).
    pub(super) fn detach_or_hint(&mut self, stop: bool) {
        if self.remote.is_none() {
            self.set_toast(
                "run `atlas dev --detach` to keep services running in the background",
                ToastVariant::Info,
            );
            return;
        }
        if stop {
            self.stop_remote();
        } else {
            self.leave_remote(Leave::Detach);
        }
    }
}
