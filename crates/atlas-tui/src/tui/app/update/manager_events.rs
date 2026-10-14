//! Folding `ManagerEvent`s (from the local manager) into app state.

use std::time::Instant;

use super::super::messages::Effect;
use super::super::{App, Message, ToastVariant};
use super::classify_note;
use crate::runtime::manager::ManagerEvent;
use crate::runtime::service::ServiceState;
use crate::tui::widgets::events::EventKind;

impl App {
    pub(in crate::tui::app) fn apply_manager_event(&mut self, event: ManagerEvent) {
        match event {
            ManagerEvent::StateChanged { name, state } => {
                // Watchers re-emit the same state on every poll; log only transitions.
                let changed = self.manager.get_state(&name).as_ref() != Some(&state);
                if changed {
                    self.push_state_event(&name, &state);
                }
                self.update(Message::ServiceStateChanged { name, state });
            }
            ManagerEvent::LogLine { name, line, stream } => {
                self.update(Message::LogLineFrom { name, line, stream });
            }
            ManagerEvent::ServiceRestarted { name } => self.log_service_restarted(&name),
            ManagerEvent::AllStarted => {
                self.started = true;
                self.splash_done = true;
            }
            ManagerEvent::AllStopped => {}
            ManagerEvent::CriticalAlert { service, message } => {
                if self.toast.is_none() {
                    self.set_toast(
                        format!("CRITICAL: {service} – {message}"),
                        ToastVariant::Warning,
                    );
                }
                self.push_event(EventKind::Alert, service, message);
            }
            ManagerEvent::ActionProgress { label, line } => {
                if let Some(op) = self.operation.as_mut()
                    && op.label == label
                    && op.failed.is_none()
                {
                    op.detail = line;
                }
            }
            ManagerEvent::ActionCompleted { label, ok, resume } => {
                let (msg, variant) = if ok {
                    (label.clone(), ToastVariant::Success)
                } else {
                    (format!("{label} failed"), ToastVariant::Error)
                };
                self.toast = Some((msg, variant, Instant::now()));
                self.finish_operation(&label, ok);
                for name in resume {
                    self.effects.push_back(Effect::RestartService(name));
                }
            }
            ManagerEvent::StatusChanged => {}
            ManagerEvent::ServiceNote { name, message } => {
                let kind = classify_note(&message);
                self.push_event(kind, name, message);
            }
            ManagerEvent::LogReady { name } => self.manager.mark_log_ready(&name),
            ManagerEvent::TunnelState { state } => {
                let name = crate::runtime::tunnel::LOG_NAME;
                if state != ServiceState::Stopped && !self.tabs.iter().any(|t| t == name) {
                    self.tabs.push(name.to_string());
                }
                self.push_event(
                    EventKind::Tunnel,
                    "tunnel".to_string(),
                    format!("{state:?}").to_lowercase(),
                );
                self.manager.write_status_file();
            }
            ManagerEvent::DepsInstalled => {
                self.set_toast("Dependencies installed", ToastVariant::Info);
                self.deps_done();
            }
            // Like tools/dev: a failed install is reported but does not block the
            // services (they fail with their own clear errors if deps are missing).
            ManagerEvent::DepsFailed { reason } => {
                self.set_toast(format!("pre_command failed: {reason}"), ToastVariant::Error);
                self.deps_done();
            }
            ManagerEvent::InfraFailed { names, reason } => {
                self.manager.record_failure(&names, &reason);
            }
            ManagerEvent::InfraReady { app_names } => {
                if self.deps_pending {
                    self.held_app_start = Some(app_names);
                } else {
                    self.effects.push_back(Effect::StartAppServices(app_names));
                }
            }
            ManagerEvent::Quit => self.should_quit = true,
            ManagerEvent::SessionStarted { label } => {
                // Track platform for this session if it's a mobile action
                if let Some(action) = self.config.actions.iter().find(|a| a.label == label)
                    && let Some(platform) = action.platform.clone()
                {
                    self.sessions.platforms.insert(label.clone(), platform);
                }
                self.update(Message::SessionStarted { label });
            }
            ManagerEvent::SessionEnded { label, ok: _ } => {
                self.sessions.phases.remove(&label);
                self.sessions.platforms.remove(&label);
                self.update(Message::SessionEnded { label });
            }
            ManagerEvent::MobileSessionPhaseChanged { label, phase } => {
                self.update(Message::MobileSessionPhaseChanged { label, phase });
            }
            ManagerEvent::DeviceDisconnected { label, device } => {
                self.update(Message::DeviceDisconnected { label, device });
            }
        }
    }

    /// The install finished (either way): release the held app-service start.
    pub(in crate::tui::app) fn deps_done(&mut self) {
        self.deps_pending = false;
        if let Some(names) = self.held_app_start.take() {
            self.effects.push_back(Effect::StartAppServices(names));
        }
    }

    /// Mark the tracked operation done: success clears it, failure keeps it
    /// visible (with the last output line) until the next run or 30s.
    /// Empty labels (pause_services resume pings) are ignored.
    pub(in crate::tui::app) fn finish_operation(&mut self, label: &str, ok: bool) {
        if label.is_empty() {
            return;
        }
        if self.operation.as_ref().is_some_and(|o| o.label == label) {
            if ok {
                self.operation = None;
            } else if let Some(op) = self.operation.as_mut() {
                op.failed = Some((Instant::now(), op.detail.clone()));
            }
        }
    }
}
