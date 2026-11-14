//! Service control: restart/stop/run action, the service menu and tunnel policy.

use super::super::{App, Effect, Layer, Message, ServiceMenuAction, ToastVariant};

impl App {
    /// Handle the services messages; anything else is handed back for the next domain.
    pub(super) fn reduce_services(&mut self, msg: Message) -> Option<Message> {
        match msg {
            Message::ServiceStateChanged { name, state } => {
                self.manager.update_state(&name, state);
            }
            Message::RestartAll => self.ask_restart_all(),
            Message::RestartAllConfirmed => {
                let names = self.restart_all_targets();
                self.set_toast(
                    format!("Restarting {} services", names.len()),
                    ToastVariant::Progress,
                );
                for name in &names {
                    self.audit_human("restart_service", name);
                }
                self.effects.push_back(Effect::RestartServices(names));
            }
            Message::StopService(name) => {
                self.audit_human("stop_service", &name);
                self.set_toast(format!("Stopping {name}"), ToastVariant::Progress);
                self.effects.push_back(Effect::StopService(name));
            }
            Message::RestartService(name) => {
                self.audit_human("restart_service", &name);
                self.set_toast(format!("Restarting {name}"), ToastVariant::Progress);
                self.effects.push_back(Effect::RestartService(name));
            }
            Message::RunAction(key) => {
                // `confirm:` gates every trigger path (key, palette) behind a modal.
                match self.manager.find_action(key).and_then(|a| {
                    let pauses = a.pause_services.clone();
                    a.confirm.filter(|c| !c.is_empty()).map(|body| {
                        // Say what else stops, so Enter is an informed choice.
                        let body = if pauses.is_empty() {
                            body
                        } else {
                            format!("{body}\n\nPauses: {}", pauses.join(", "))
                        };
                        (a.label, body)
                    })
                }) {
                    Some((title, body)) => {
                        self.layer = Layer::Confirm {
                            title,
                            body,
                            on_confirm: Box::new(Message::RunActionConfirmed(key)),
                        };
                    }
                    None => {
                        self.audit_human_action(key);
                        self.effects.push_back(Effect::RunAction(key));
                    }
                }
            }
            Message::RunActionConfirmed(key) => {
                self.audit_human_action(key);
                self.effects.push_back(Effect::RunAction(key));
            }
            Message::OpenService(key) => {
                self.effects.push_back(Effect::OpenService(key));
            }
            Message::ToggleTunnelOnExit => {
                // No `tunnel:` block → the key does nothing (and must not claim otherwise).
                if !self.manager.has_tunnel() {
                    return None;
                }
                // The runner owns the tunnel exit policy (it always closes it on exit).
                if self.remote.is_some() {
                    self.set_toast(
                        "The runner owns the tunnel: it closes when the runner stops",
                        ToastVariant::Info,
                    );
                    return None;
                }
                self.tunnel_kill_on_exit = !self.tunnel_kill_on_exit;
                self.manager
                    .set_tunnel_kill_on_exit(self.tunnel_kill_on_exit);
                let msg = if self.tunnel_kill_on_exit {
                    "Tunnel will close on exit"
                } else {
                    "Tunnel will stay on exit"
                };
                self.set_toast(msg, ToastVariant::Info);
            }
            Message::ShowServiceMenu(name) => {
                self.layer = Layer::ServiceMenu(name);
            }
            Message::HideServiceMenu => {
                self.layer = Layer::Dashboard;
            }
            Message::ServiceMenuAction { name, action } => {
                self.layer = Layer::Dashboard;
                match action {
                    ServiceMenuAction::Restart => {
                        self.audit_human("restart_service", &name);
                        self.set_toast(format!("Restarting {name}"), ToastVariant::Progress);
                        self.effects.push_back(Effect::RestartService(name));
                    }
                    ServiceMenuAction::Stop => {
                        self.audit_human("stop_service", &name);
                        self.set_toast(format!("Stopping {name}"), ToastVariant::Progress);
                        self.effects.push_back(Effect::StopService(name));
                    }
                    ServiceMenuAction::Start => {
                        self.audit_human("start_service", &name);
                        self.set_toast(format!("Starting {name}"), ToastVariant::Progress);
                        self.effects.push_back(Effect::RestartService(name));
                    }
                }
            }
            Message::MobileSessionPhaseChanged { label, phase } => {
                self.sessions.phases.insert(label, phase);
            }
            Message::DeviceDisconnected { label, device } => {
                self.set_toast(
                    format!("⚠ Device disconnected: {device} ({label})"),
                    ToastVariant::Warning,
                );
            }
            other => return Some(other),
        }
        None
    }
}
