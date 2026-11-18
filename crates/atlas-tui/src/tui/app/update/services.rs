//! Service control: restart/stop/run action, the service menu and tunnel policy.

use super::super::{App, Effect, Layer, Message, ServiceMenuAction, ToastVariant};
use atlas_i18n::t;

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
                    t!("tui_runtime_app_toast_restarting_n", count = names.len()),
                    ToastVariant::Progress,
                );
                for name in &names {
                    self.audit_human("restart_service", name);
                }
                self.effects.push_back(Effect::RestartServices(names));
            }
            Message::StopService(name) => {
                self.audit_human("stop_service", &name);
                self.set_toast(
                    t!("tui_runtime_app_toast_stopping", name = name.as_str()),
                    ToastVariant::Progress,
                );
                self.effects.push_back(Effect::StopService(name));
            }
            Message::RestartService(name) => {
                self.audit_human("restart_service", &name);
                self.set_toast(
                    t!("tui_runtime_app_toast_restarting", name = name.as_str()),
                    ToastVariant::Progress,
                );
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
                            format!(
                                "{body}\n\n{}",
                                t!(
                                    "tui_runtime_app_confirm_pauses",
                                    services = pauses.join(", ")
                                )
                            )
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
                        t!("tui_runtime_app_toast_tunnel_runner"),
                        ToastVariant::Info,
                    );
                    return None;
                }
                self.tunnel_kill_on_exit = !self.tunnel_kill_on_exit;
                self.manager
                    .set_tunnel_kill_on_exit(self.tunnel_kill_on_exit);
                let msg = if self.tunnel_kill_on_exit {
                    t!("tui_runtime_app_toast_tunnel_close")
                } else {
                    t!("tui_runtime_app_toast_tunnel_stay")
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
                        self.set_toast(
                            t!("tui_runtime_app_toast_restarting", name = name.as_str()),
                            ToastVariant::Progress,
                        );
                        self.effects.push_back(Effect::RestartService(name));
                    }
                    ServiceMenuAction::Stop => {
                        self.audit_human("stop_service", &name);
                        self.set_toast(
                            t!("tui_runtime_app_toast_stopping", name = name.as_str()),
                            ToastVariant::Progress,
                        );
                        self.effects.push_back(Effect::StopService(name));
                    }
                    ServiceMenuAction::Start => {
                        self.audit_human("start_service", &name);
                        self.set_toast(
                            t!("tui_runtime_app_toast_starting", name = name.as_str()),
                            ToastVariant::Progress,
                        );
                        self.effects.push_back(Effect::RestartService(name));
                    }
                }
            }
            Message::MobileSessionPhaseChanged { label, phase } => {
                self.sessions.phases.insert(label, phase);
            }
            Message::DeviceDisconnected { label, device } => {
                self.set_toast(
                    format!(
                        "⚠ {}",
                        t!(
                            "tui_runtime_app_toast_device_gone",
                            device = device.as_str(),
                            label = label.as_str()
                        )
                    ),
                    ToastVariant::Warning,
                );
            }
            other => return Some(other),
        }
        None
    }
}
