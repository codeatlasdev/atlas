//! The TEA reducer: `update` routes every `Message` through the domain
//! reducers (`reduce_*`, one file each); the rest of this file is the state
//! helpers they share (toasts, sidebar events, update offer, paste).

mod clipboard;
mod effects;
mod logs;
mod manager_events;
mod nav;
mod palette;
mod services;
mod sessions;
mod shell;

use std::time::Instant;

use super::messages::Effect;
use super::{App, Layer, Message, ToastVariant};
use crate::runtime::service::ServiceState;
use crate::tui::widgets::events::{EventKind, SideEvent};

impl App {
    /// Apply one message. Each domain reducer takes the messages it owns and
    /// hands the rest back, so exactly one of them acts.
    pub fn update(&mut self, msg: Message) {
        let Some(msg) = self.reduce_shell(msg) else {
            return;
        };
        let Some(msg) = self.reduce_nav(msg) else {
            return;
        };
        let Some(msg) = self.reduce_palette(msg) else {
            return;
        };
        let Some(msg) = self.reduce_services(msg) else {
            return;
        };
        let Some(msg) = self.reduce_sessions(msg) else {
            return;
        };
        let leftover = self.reduce_logs(msg);
        debug_assert!(leftover.is_none(), "message handled by no reducer");
    }

    /// Route a bracketed paste to whichever text input is active; ignore otherwise.
    /// Text inputs are single-line, so newlines/control chars are dropped.
    pub(in crate::tui::app) fn apply_paste(&mut self, text: &str) {
        match &self.layer {
            Layer::CommandPalette => {
                self.palette_input
                    .extend(text.chars().filter(|c| !c.is_control()));
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
            }
            Layer::SessionAttached(label) => {
                // Raw to the session's stdin: newlines are meaningful here.
                let bytes = text.replace("\r\n", "\n").replace('\r', "\n").into_bytes();
                self.effects.push_back(Effect::SendSessionInput {
                    label: label.clone(),
                    bytes,
                });
            }
            Layer::Dashboard if self.logs.time_input.is_some() => {
                for c in text.chars().filter(|c| !c.is_control()) {
                    self.time_input(c);
                }
            }
            Layer::Dashboard if self.logs.log_searching => {
                self.logs
                    .log_search_input
                    .extend(text.chars().filter(|c| !c.is_control()));
                self.find_requery();
            }
            _ => {}
        }
    }

    /// Append a typed sidebar event (capped at 50).
    /// Sidebar event for a service entering `state` (reads the manager's facts as
    /// they are *before* the transition is applied).
    pub(in crate::tui::app) fn push_state_event(&mut self, name: &str, state: &ServiceState) {
        let done = self.manager.completed_ok(name);
        let paused_by = self.manager.paused_by(name);
        let ev = match state {
            ServiceState::Failed => {
                let why = self
                    .manager
                    .exit_code(name)
                    .map(|c| format!("crashed (exit {c})"))
                    .unwrap_or_else(|| "failed".to_string());
                Some((EventKind::Crash, why))
            }
            ServiceState::Unhealthy => Some((
                EventKind::Crash,
                self.manager
                    .service_note(name)
                    .unwrap_or_else(|| "unhealthy".to_string()),
            )),
            ServiceState::Degraded => Some((
                EventKind::Alert,
                self.manager
                    .service_note(name)
                    .unwrap_or_else(|| "degraded".to_string()),
            )),
            ServiceState::Running => Some((EventKind::Started, "running".into())),
            ServiceState::Stopped if done => Some((EventKind::Done, "done".into())),
            ServiceState::Stopped | ServiceState::Suspended => match paused_by {
                Some(by) => Some((EventKind::Paused(by), String::new())),
                None => Some((EventKind::Stopped, "stopped".into())),
            },
            _ => None,
        };
        if let Some((kind, text)) = ev {
            self.push_event(kind, name.to_string(), text);
        }
    }

    pub(crate) fn push_event(&mut self, kind: EventKind, subject: String, text: String) {
        let ts_ms = crate::runtime::clock::now_ms();
        if self.event_log.len() >= 50 {
            self.event_log.pop_front();
        }
        self.event_log.push_back(SideEvent {
            ts_ms,
            kind,
            subject,
            text,
        });
    }

    pub(crate) fn set_toast(&mut self, msg: impl Into<String>, variant: ToastVariant) {
        self.toast = Some((msg.into(), variant, Instant::now()));
    }

    /// Offer "update when you quit" for `version`: once per version per session,
    /// only after the splash and only when no other modal is open (otherwise it is
    /// retried on the next tick, i.e. queued). Already accepted → nothing to offer.
    pub(crate) fn offer_update(&mut self, version: String) {
        if !self.splash_done
            || self.update_on_exit
            || self.layer != Layer::Dashboard
            || self.update_announced.as_ref() == Some(&version)
        {
            return;
        }
        let body = if self.remote.is_some() {
            format!(
                "Atlas v{version} is available — update when you quit? The runner keeps the old version until `atlas down && atlas dev --detach`."
            )
        } else {
            format!("Atlas v{version} is available — update when you quit?")
        };
        self.layer = Layer::Confirm {
            title: "Update available".to_string(),
            body,
            on_confirm: Box::new(Message::UpdateOnExit(version.clone())),
        };
        self.update_announced = Some(version);
    }

    /// URL for an `open_key` service: `open_url` from atlas.yaml wins,
    /// otherwise `http://localhost:<port>`.
    pub(in crate::tui::app) fn service_open_url(&self, name: &str, port: Option<u16>) -> String {
        self.config
            .services
            .get(name)
            .and_then(|d| d.open_url.clone())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| {
                port.map(|p| format!("http://localhost:{p}"))
                    .unwrap_or_else(|| "http://localhost".to_string())
            })
    }
}

/// Map a scheduler note to a sidebar event kind.
pub(in crate::tui::app) fn classify_note(message: &str) -> EventKind {
    let m = message.to_lowercase();
    if m.starts_with("waiting") {
        EventKind::Waiting
    } else if m.contains("gave up") || m.contains("crashed") || m.contains("timeout") {
        EventKind::Crash
    } else if m.contains("completed") || m.contains("exited cleanly") {
        EventKind::Done
    } else if m.contains("restart") {
        EventKind::Restart
    } else {
        EventKind::Info
    }
}
