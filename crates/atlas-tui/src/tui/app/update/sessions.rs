//! Session tabs (`session: true` actions) and the device picker.

use super::super::{App, Effect, Layer, Message, ToastVariant};
use atlas_i18n::t;

impl App {
    /// Handle the sessions messages; anything else is handed back for the next domain.
    pub(super) fn reduce_sessions(&mut self, msg: Message) -> Option<Message> {
        match msg {
            // Session tab lifecycle
            Message::SessionStarted { label } => {
                self.sessions.tabs.insert(label.clone());
                // Add a dynamic tab for this session if not already present
                if !self.tabs.contains(&label) {
                    self.tabs.push(label.clone());
                }
                // Switch to the new tab immediately
                if let Some(idx) = self.tabs.iter().position(|t| t == &label) {
                    self.active_tab = idx;
                    self.reset_log_view();
                }
                self.set_toast(format!("⏵ {label}"), ToastVariant::Info);
            }
            Message::SessionEnded { label } => {
                self.sessions.tabs.remove(&label);
                // Remove the tab and switch back to All
                if let Some(idx) = self.tabs.iter().position(|t| t == &label) {
                    self.tabs.remove(idx);
                    self.active_tab = 0;
                    self.reset_log_view();
                }
                self.set_toast(
                    format!(
                        "⏹ {}",
                        t!(
                            "tui_runtime_app_toast_session_ended",
                            label = label.as_str()
                        )
                    ),
                    ToastVariant::Info,
                );
            }
            Message::SessionInput { label, bytes } => {
                self.effects
                    .push_back(Effect::SendSessionInput { label, bytes });
            }
            Message::StopSession(label) => {
                // If we were attached, detach first
                if matches!(&self.layer, Layer::SessionAttached(_)) {
                    self.layer = Layer::Dashboard;
                }
                self.effects.push_back(Effect::StopSession(label));
            }
            // Attach/detach mode
            Message::AttachSession => {
                if let Some(label) = self.tabs.get(self.active_tab).cloned()
                    && self.sessions.tabs.contains(&label)
                {
                    self.layer = Layer::SessionAttached(label);
                }
            }
            Message::DetachSession => {
                self.layer = Layer::Dashboard;
            }
            // Device picker
            Message::ShowDevicePicker {
                action_key,
                platform,
            } => {
                self.effects.push_back(Effect::DetectDevices {
                    action_key,
                    platform: platform.clone(),
                });
                self.layer = Layer::DevicePicker {
                    action_key,
                    platform,
                    devices: vec![],
                    selected: 0,
                    detecting: true,
                };
            }
            Message::DevicePickerUp => {
                if let Layer::DevicePicker { selected, .. } = &mut self.layer {
                    *selected = selected.saturating_sub(1);
                }
            }
            Message::DevicePickerDown => {
                if let Layer::DevicePicker {
                    devices, selected, ..
                } = &mut self.layer
                {
                    let max = devices.len().saturating_sub(1);
                    if *selected < max {
                        *selected += 1;
                    }
                }
            }
            Message::DevicePickerConfirm => {
                if let Layer::DevicePicker {
                    action_key,
                    devices,
                    selected,
                    ..
                } = self.layer.clone()
                {
                    let device_id = devices.get(selected).map(|(id, _)| id.clone());
                    self.layer = Layer::Dashboard;
                    self.effects.push_back(Effect::RunSessionWithDevice {
                        key: action_key,
                        device_id,
                    });
                }
            }
            Message::DevicePickerCancel => {
                self.layer = Layer::Dashboard;
            }
            Message::DevicesDetected {
                action_key,
                devices,
            } => {
                if let Layer::DevicePicker {
                    action_key: k,
                    devices: d,
                    detecting,
                    ..
                } = &mut self.layer
                    && *k == action_key
                {
                    *d = devices;
                    *detecting = false;
                }
            }
            Message::RunSessionWithDevice { key, device_id } => {
                self.effects
                    .push_back(Effect::RunSessionWithDevice { key, device_id });
            }
            Message::DevicePickerPick(idx) => {
                // Row click: select + confirm, same as ↑↓ then Enter.
                if let Layer::DevicePicker {
                    devices, selected, ..
                } = &mut self.layer
                {
                    let max = devices.len().saturating_sub(1);
                    *selected = idx.min(max);
                }
                self.update(Message::DevicePickerConfirm);
            }
            other => return Some(other),
        }
        None
    }
}
