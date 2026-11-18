//! Running queued `Effect`s: the only place the TUI awaits the manager.

use super::super::messages::Effect;
use super::super::{App, Message, Operation, ToastVariant};
use crate::runtime::logs;
use crate::runtime::manager::ServiceManager;
use atlas_i18n::t;

impl App {
    pub(in crate::tui::app) async fn execute_effects(&mut self) {
        while let Some(effect) = self.effects.pop_front() {
            // Attached: service actions go to the runner instead of a local manager.
            if self.remote_effect(&effect) {
                continue;
            }
            match effect {
                Effect::StartAppServices(names) => {
                    self.manager.start_app_services(&names).await;
                }
                Effect::RestartServices(names) => {
                    for name in &names {
                        self.manager.restart_service(name).await;
                    }
                    self.set_toast(
                        t!("tui_runtime_app_toast_restarted_n", count = names.len()),
                        ToastVariant::Success,
                    );
                }
                Effect::CheckHealth => self.manager.check_health().await,
                Effect::CopyToClipboard { text, success } => {
                    let ok = logs::copy_to_clipboard(&text).await.is_ok();
                    self.update(Message::ClipboardResult { success, ok });
                }
                Effect::OpenUrl(url) => {
                    if !crate::runtime::opener::open_url(&url) {
                        self.set_toast(t!("tui_runtime_app_toast_no_browser"), ToastVariant::Error);
                    }
                }
                Effect::OpenPath { path, line, col } => {
                    if crate::runtime::opener::open_path(&path, line, col) {
                        self.set_toast(
                            t!(
                                "tui_runtime_app_toast_opening_path",
                                path = path.as_str(),
                                line = line
                            ),
                            ToastVariant::Info,
                        );
                    } else {
                        // No GUI editor configured: hand the location to the clipboard.
                        let loc = format!("{path}:{line}");
                        self.copy_text(
                            loc.clone(),
                            t!("tui_runtime_app_toast_no_editor", location = loc.as_str()),
                        );
                    }
                }
                Effect::StopService(name) => {
                    self.manager.stop_service_disabled(&name).await;
                    self.set_toast(
                        t!("tui_runtime_app_toast_stopped", name = name.as_str()),
                        ToastVariant::Success,
                    );
                }
                Effect::RestartService(name) => {
                    self.manager.restart_service(&name).await;
                    self.set_toast(
                        t!("tui_runtime_app_toast_restarting", name = name.as_str()),
                        ToastVariant::Progress,
                    );
                }
                Effect::RunAction(key) => {
                    // Check if this action has a platform — if so, show device picker first
                    // BUT: if a session with this label is already running, kill it instead
                    if let Some(action) = self.manager.find_action(key) {
                        let label = action.label.clone();
                        if action.session && self.manager.session_active(&label) {
                            // Session already running — kill it on second press
                            self.manager.stop_session(&label).await;
                            self.update(Message::SessionEnded { label });
                            continue;
                        }
                        if let Some(platform) = action.platform {
                            // Skip picker if device is pinned in atlas.yaml
                            if let Some(id) = action.device_id {
                                self.effects.push_back(Effect::RunSessionWithDevice {
                                    key,
                                    device_id: Some(id),
                                });
                            } else {
                                self.update(Message::ShowDevicePicker {
                                    action_key: key,
                                    platform,
                                });
                            }
                            continue;
                        }
                    }
                    if let Some(a) = self.manager.find_action(key)
                        && !a.session
                        && self.manager.action_running(&a.label)
                    {
                        self.set_toast(
                            t!(
                                "tui_runtime_app_toast_action_running",
                                label = a.label.as_str()
                            ),
                            ToastVariant::Info,
                        );
                        continue;
                    }
                    if let Some(label) = self.manager.run_action(key).await
                        && !self.manager.session_active(&label)
                    {
                        self.operation = Some(Operation::start(label.clone()));
                        self.set_toast(
                            t!("tui_runtime_app_toast_running", label = label.as_str()),
                            ToastVariant::Progress,
                        );
                    }
                }
                Effect::StopSession(label) => {
                    self.manager.stop_session(&label).await;
                }
                Effect::SendSessionInput { label, bytes } => {
                    self.manager.send_session_input(&label, &bytes).await;
                }
                Effect::DetectDevices {
                    action_key,
                    platform,
                } => {
                    let devices = ServiceManager::detect_devices(&platform).await;
                    self.update(Message::DevicesDetected {
                        action_key,
                        devices,
                    });
                }
                Effect::RunSessionWithDevice { key, device_id } => {
                    if let Some(label) = self.manager.run_session_with_device(key, device_id).await
                        && !self.manager.session_active(&label)
                    {
                        self.set_toast(
                            t!("tui_runtime_app_toast_running", label = label.as_str()),
                            ToastVariant::Progress,
                        );
                    }
                }
                Effect::SetMouseCapture(on) => crate::tui::set_mouse_capture(on),
                Effect::Suspend => {
                    crate::tui::suspend();
                    // Terminal contents are stale after resume; re-assert mouse state too.
                    crate::tui::set_mouse_capture(self.mouse_capture);
                    self.force_redraw = true;
                    self.dirty = true;
                }
                Effect::OpenService(key) => {
                    if let Some((_, name, port)) = self
                        .manager
                        .open_services()
                        .into_iter()
                        .find(|(k, _, _)| *k == key)
                    {
                        ServiceManager::open_url(&self.service_open_url(&name, port));
                        self.set_toast(
                            t!("tui_runtime_app_toast_opening", name = name.as_str()),
                            ToastVariant::Progress,
                        );
                    }
                }
            }
        }
    }
}
