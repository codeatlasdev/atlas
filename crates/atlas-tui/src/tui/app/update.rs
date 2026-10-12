use std::time::Instant;

use crate::runtime::logs::{self, ClipboardContext, LogEntry};
use crate::runtime::manager::{ManagerEvent, ServiceManager};
use crate::runtime::service::ServiceState;

use super::messages::Effect;

/// "Copy logs": how far back the clipboard copy reaches.
const COPY_RECENT_SECS: u64 = 180;
/// "Copy for AI": wider window so the prompt carries context around the failure.
const COPY_CONTEXT_SECS: u64 = 300;

/// How one clipboard copy words its outcome.
struct CopyKind {
    /// Toast text from the number of copied entries.
    success: fn(usize) -> String,
    /// Toast shown instead of copying when nothing matched (None = copy anyway).
    empty_toast: Option<&'static str>,
    /// Header filter label used when no tab narrows the copy.
    fallback_label: Option<&'static str>,
}

impl CopyKind {
    fn plain(success: fn(usize) -> String) -> Self {
        Self {
            success,
            empty_toast: None,
            fallback_label: None,
        }
    }
}

use super::{App, Layer, Leave, Message, ServiceMenuAction, ToastVariant};
use crate::tui::widgets::events::{EventKind, SideEvent};

impl App {
    pub(super) fn apply_manager_event(&mut self, event: ManagerEvent) {
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

    pub(super) async fn execute_effects(&mut self) {
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
                        format!("Restarted {} services", names.len()),
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
                        self.set_toast("No browser launcher found", ToastVariant::Error);
                    }
                }
                Effect::OpenPath { path, line, col } => {
                    if crate::runtime::opener::open_path(&path, line, col) {
                        self.set_toast(format!("Opening {path}:{line}"), ToastVariant::Info);
                    } else {
                        // No GUI editor configured: hand the location to the clipboard.
                        let loc = format!("{path}:{line}");
                        self.copy_text(
                            loc.clone(),
                            format!("No GUI editor ($VISUAL/$EDITOR/code/zed) — copied {loc}"),
                        );
                    }
                }
                Effect::StopService(name) => {
                    self.manager.stop_service_disabled(&name).await;
                    self.set_toast(format!("Stopped {name}"), ToastVariant::Success);
                }
                Effect::RestartService(name) => {
                    self.manager.restart_service(&name).await;
                    self.set_toast(format!("Restarting {name}"), ToastVariant::Progress);
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
                        self.set_toast(format!("{} already running", a.label), ToastVariant::Info);
                        continue;
                    }
                    if let Some(label) = self.manager.run_action(key).await
                        && !self.manager.session_active(&label)
                    {
                        self.operation = Some(super::Operation::start(label.clone()));
                        self.set_toast(format!("Running {label}"), ToastVariant::Progress);
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
                        self.set_toast(format!("Running {label}"), ToastVariant::Progress);
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
                        self.set_toast(format!("Opening {name}…"), ToastVariant::Progress);
                    }
                }
            }
        }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            // Attached: Ctrl+C / SIGTERM / `y` must never stop services that belong to
            // the runner; leaving is always a detach (stopping is an explicit choice).
            Message::Quit if self.remote.is_some() => self.leave_remote(Leave::Detach),
            Message::Quit => self.should_quit = true,
            Message::Tick => {
                self.tick_count += 1;
                // Auto-dismiss toast
                if let Some((_, _, created)) = &self.toast
                    && created.elapsed() > std::time::Duration::from_secs(3)
                {
                    self.toast = None;
                }
                // Failed operation lingers 30s, then the sidebar block goes away.
                if self.operation.as_ref().is_some_and(|o| o.expired()) {
                    self.operation = None;
                }
                // Newer release found by the background check: offer "update when you
                // quit" once per version, after the splash, queued behind any open modal.
                if let Some(v) = crate::update_notice::available() {
                    self.offer_update(v);
                }
                // Splash done
                if !self.splash_done
                    && self.splash_start.elapsed() > std::time::Duration::from_millis(1500)
                {
                    self.splash_done = true;
                }
                // Enqueue periodic health check
                // Fast initial probe (tick 3 = 300ms after start), then every second
                let due = self.tick_count == 3
                    || (self.tick_count > 3 && self.tick_count.is_multiple_of(10));
                if self.started && due {
                    self.effects.push_back(Effect::CheckHealth);
                }
            }
            Message::Resize(w, h) => {
                self.size = (w, h);
            }
            Message::SwitchTab(idx) => {
                if idx < self.tabs.len() {
                    self.active_tab = idx;
                    self.reset_log_view();
                    self.save_ui_state();
                }
            }
            Message::NextTab => {
                self.active_tab = (self.active_tab + 1) % self.tabs.len();
                self.reset_log_view();
                self.save_ui_state();
            }
            Message::PrevTab => {
                self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
                self.reset_log_view();
                self.save_ui_state();
            }
            Message::ServiceStateChanged { name, state } => {
                self.manager.update_state(&name, state);
            }
            Message::LogLineFrom { name, line, stream } => {
                self.logs.log_buffer.push(name, line, stream);
                // Paused: grow the offset by the new rows so the view stays put.
                if !self.logs.follow_logs {
                    self.bump_paused_scroll_for_last();
                }
            }
            Message::RestartAll => self.ask_restart_all(),
            Message::RestartAllConfirmed => {
                let names = self.restart_all_targets();
                self.set_toast(
                    format!("Restarting {} services", names.len()),
                    ToastVariant::Progress,
                );
                self.effects.push_back(Effect::RestartServices(names));
            }
            Message::ShowHelp => {
                self.help_scroll = 0;
                self.layer = Layer::Help;
            }
            Message::HelpScroll(d) => {
                let max = self.help_max_scroll.get();
                self.help_scroll = if d == isize::MIN {
                    0
                } else if d == isize::MAX {
                    max
                } else {
                    (self.help_scroll as isize)
                        .saturating_add(d)
                        .clamp(0, max as isize) as usize
                };
            }
            Message::HideHelp => self.layer = Layer::Dashboard,
            Message::ShowQuit => {
                self.layer = Layer::Quit;
                self.quit_selected = 0;
            }
            Message::HideQuit => self.layer = Layer::Dashboard,
            Message::ToggleQuitSelection => {
                self.quit_selected = 1 - self.quit_selected;
            }
            Message::Detach => self.detach_or_hint(false),
            Message::StopRunner => self.detach_or_hint(true),
            Message::Toast(msg, variant) => {
                self.set_toast(msg, variant);
            }
            Message::DismissToast => {
                self.toast = None;
            }
            Message::ScrollDown => self.scroll_logs(-1),
            Message::ScrollUp => self.scroll_logs(1),
            Message::ScrollPageDown => self.scroll_logs(-self.log_page()),
            Message::ScrollPageUp => self.scroll_logs(self.log_page()),
            Message::ScrollToBottom => self.reset_log_view(),
            Message::ScrollToTop => self.scroll_logs(isize::MAX),
            Message::HealthCheckDone => {}
            // Mouse — click map is the single source of truth
            Message::MouseClick { x, y } => {
                use crate::tui::clicks::Target;
                match self.clicks.hit(x, y).map(|(_, t)| t) {
                    Some(Target::Click(msg)) => self.update(msg),
                    Some(Target::Scrollbar(geo)) => {
                        // Click on track: jump to position and start drag.
                        let n = geo.scroll_for_row(y);
                        self.logs.log_scroll = n.min(geo.max_scroll);
                        self.set_follow(self.logs.log_scroll == 0);
                        self.drag_scrollbar = Some(geo);
                    }
                    Some(Target::Inert) | Some(Target::Wheel(_)) | None => {}
                }
            }
            Message::MouseDrag { y } => {
                if let Some(geo) = self.drag_scrollbar {
                    let n = geo.scroll_for_row(y);
                    self.logs.log_scroll = n.min(geo.max_scroll);
                    self.set_follow(self.logs.log_scroll == 0);
                }
            }
            Message::MouseRelease => {
                self.drag_scrollbar = None;
            }
            Message::MouseScroll { x, y, down } => {
                use crate::tui::clicks::{Target, Wheel};
                match self.clicks.hit(x, y).map(|(_, t)| t) {
                    Some(Target::Wheel(Wheel::Palette)) => {
                        if down {
                            self.update(Message::PaletteDown);
                        } else {
                            self.update(Message::PaletteUp);
                        }
                    }
                    Some(Target::Wheel(Wheel::Picker)) => {
                        if down {
                            self.update(Message::DevicePickerDown);
                        } else {
                            self.update(Message::DevicePickerUp);
                        }
                    }
                    _ if self.layer == Layer::Help => {
                        self.update(Message::HelpScroll(if down { 3 } else { -3 }));
                    }
                    // Anything else (including explicit Logs wheel) scrolls the log.
                    _ => self.scroll_logs(if down { -3 } else { 3 }),
                }
            }
            Message::ScrollLogsTo(n) => {
                let max = self.logs.log_viewport.get().max_scroll;
                self.logs.log_scroll = n.min(max);
                self.set_follow(self.logs.log_scroll == 0);
            }
            Message::ResetCursor => {
                // Reset terminal cursor shape after Shift+Cmd link click or focus regain.
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::cursor::SetCursorStyle::DefaultUserShape
                );
            }
            // Command Palette
            Message::ShowPalette => {
                self.layer = Layer::CommandPalette;
                self.palette_input.clear();
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
                self.palette_offset = 0;
            }
            Message::HidePalette => self.layer = Layer::Dashboard,
            Message::PaletteInput(c) => {
                self.palette_input.push(c);
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
                self.palette_offset = 0;
            }
            Message::PaletteBackspace => {
                self.palette_input.pop();
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
                self.palette_offset = 0;
            }
            Message::PaletteClear => {
                self.palette_input.clear();
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
                self.palette_offset = 0;
            }
            Message::PaletteDeleteWord => {
                let trimmed = self.palette_input.trim_end().len();
                self.palette_input.truncate(trimmed);
                let keep = self.palette_input.rfind(' ').map_or(0, |i| i + 1);
                self.palette_input.truncate(keep);
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
                self.palette_offset = 0;
            }
            Message::PaletteUp => {
                self.palette_selected = self.palette_selected.saturating_sub(1);
                self.palette_scroll_into_view(self.palette_rows.get());
            }
            Message::PaletteDown => {
                if self.palette_selected < self.palette_filtered.len().saturating_sub(1) {
                    self.palette_selected += 1;
                }
                self.palette_scroll_into_view(self.palette_rows.get());
            }
            Message::PaletteSelect => {
                if let Some(cmd) = self.palette_filtered.get(self.palette_selected).cloned() {
                    self.layer = Layer::Dashboard;
                    self.remember_command(&cmd.id);
                    self.update(cmd.action);
                }
            }
            Message::CopyLogs => self.copy_logs(
                |b| b.since(COPY_RECENT_SECS),
                logs::format_for_clipboard,
                CopyKind::plain(|_| "Logs copied (3min)".to_string()),
            ),
            Message::CopyLogsForAI => self.copy_logs(
                |b| b.since(COPY_CONTEXT_SECS),
                logs::format_for_ai_prompt,
                CopyKind::plain(|_| "AI prompt copied".to_string()),
            ),
            Message::CopyErrors => self.copy_logs(
                |b| {
                    // One `── service ──` header per service, not per alternation.
                    let mut errors = b.errors();
                    errors.sort_by(|a, b| a.service.cmp(&b.service));
                    errors
                },
                logs::format_for_clipboard,
                CopyKind {
                    success: |n| format!("{n} errors copied"),
                    empty_toast: Some("No errors found"),
                    fallback_label: Some("errors only"),
                },
            ),
            Message::ClipboardResult { success, ok } => {
                let variant = if ok {
                    ToastVariant::Success
                } else {
                    ToastVariant::Error
                };
                self.set_toast(success, variant);
            }
            Message::StopService(name) => {
                self.set_toast(format!("Stopping {name}"), ToastVariant::Progress);
                self.effects.push_back(Effect::StopService(name));
            }
            Message::RestartService(name) => {
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
                    None => self.effects.push_back(Effect::RunAction(key)),
                }
            }
            Message::RunActionConfirmed(key) => {
                self.effects.push_back(Effect::RunAction(key));
            }
            Message::UpdateOnExit(version) => {
                self.update_on_exit = true;
                crate::update_notice::request_on_exit(version);
                self.set_toast("atlas will update when you quit", ToastVariant::Info);
            }
            Message::ConfirmAccept => {
                if let Layer::Confirm { on_confirm, .. } =
                    std::mem::replace(&mut self.layer, Layer::Dashboard)
                {
                    self.update(*on_confirm);
                }
            }
            Message::ConfirmCancel => {
                if matches!(self.layer, Layer::Confirm { .. }) {
                    self.layer = Layer::Dashboard;
                }
            }
            Message::OpenService(key) => {
                self.effects.push_back(Effect::OpenService(key));
            }
            Message::ToggleTunnelOnExit => {
                // No `tunnel:` block → the key does nothing (and must not claim otherwise).
                if !self.manager.has_tunnel() {
                    return;
                }
                // The runner owns the tunnel exit policy (it always closes it on exit).
                if self.remote.is_some() {
                    self.set_toast(
                        "The runner owns the tunnel: it closes when the runner stops",
                        ToastVariant::Info,
                    );
                    return;
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
            Message::ToggleFollow => {
                // The footer shows Following / Paused; no toast needed.
                let follow = !self.logs.follow_logs;
                self.set_follow(follow);
                if follow {
                    self.logs.log_scroll = 0;
                }
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
                        self.set_toast(format!("Restarting {name}"), ToastVariant::Progress);
                        self.effects.push_back(Effect::RestartService(name));
                    }
                    ServiceMenuAction::Stop => {
                        self.set_toast(format!("Stopping {name}"), ToastVariant::Progress);
                        self.effects.push_back(Effect::StopService(name));
                    }
                    ServiceMenuAction::Start => {
                        self.set_toast(format!("Starting {name}"), ToastVariant::Progress);
                        self.effects.push_back(Effect::RestartService(name));
                    }
                }
            }
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
                self.set_toast(format!("⏹ {label} ended"), ToastVariant::Info);
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
            Message::ClearLogs => {
                self.logs.log_buffer.clear();
                self.reset_log_view();
                self.set_toast("Logs cleared", ToastVariant::Info);
            }
            Message::StartLogSearch => {
                self.logs.log_search_input.clear();
                self.logs.log_searching = true;
                self.find_requery();
            }
            Message::LogSearchInput(c) => {
                self.logs.log_search_input.push(c);
                self.find_requery();
            }
            Message::LogSearchBackspace => {
                self.logs.log_search_input.pop();
                self.find_requery();
            }
            Message::LogSearchCommit => {
                self.logs.log_searching = false;
            }
            Message::LogSearchClear => {
                self.logs.log_search_input.clear();
                self.reset_log_view();
                self.logs.log_searching = false;
            }
            Message::CycleTimeRange => self.cycle_time_range(),
            Message::TimeInput(c) => self.time_input(c),
            Message::TimeBackspace => self.time_backspace(),
            Message::TimeCommit => self.time_commit(),
            Message::TimeCancel => self.time_cancel(),
            Message::ToggleSearchMode => self.toggle_search_mode(),
            Message::ToggleSearchRegex => self.toggle_search_regex(),
            Message::FindNext => self.find_step(true),
            Message::FindPrev => self.find_step(false),
            Message::ToggleWrap => self.toggle_wrap(),
            Message::ToggleTimestamps => self.toggle_timestamps(),
            Message::ToggleJson => self.toggle_json(),
            Message::PanLogs { right } => self.pan_logs(right),
            Message::InsertMark => self.insert_mark(),
            Message::OpenTabErrors(idx) => self.open_tab_errors(idx),
            Message::JumpProblem { forward, warn } => {
                self.logs.chord = None;
                self.jump_problem(forward, warn);
            }
            Message::StepLink { forward } => self.step_link(forward),
            Message::OpenLink => self.open_link(),
            Message::CopyLink => self.copy_link(),
            Message::ChordStart(c) => self.logs.chord = (c != '\0').then_some(c),
            Message::MobileSessionPhaseChanged { label, phase } => {
                self.sessions.phases.insert(label, phase);
            }
            Message::DeviceDisconnected { label, device } => {
                self.set_toast(
                    format!("⚠ Device disconnected: {device} ({label})"),
                    ToastVariant::Warning,
                );
            }
            Message::Paste(text) => self.apply_paste(&text),
            Message::Suspend => self.effects.push_back(Effect::Suspend),
            Message::ToggleMouseCapture => {
                self.mouse_capture = !self.mouse_capture;
                let msg = if self.mouse_capture {
                    "Mouse capture on"
                } else {
                    "Mouse capture off — select text freely"
                };
                self.set_toast(msg, ToastVariant::Info);
                self.effects
                    .push_back(Effect::SetMouseCapture(self.mouse_capture));
            }
            Message::CycleLevelFilter => {
                self.logs.level_filter = self.logs.level_filter.next();
                self.reset_log_view();
                self.save_ui_state();
            }
            Message::ToggleZoom => {
                self.zoom = !self.zoom;
                self.save_ui_state();
            }
            Message::GotoTab(name) | Message::SelectService(name) => {
                // Sidebar/strip click: switch to that service's log tab (no-op if not found).
                if let Some(idx) = self.tabs.iter().position(|t| t == &name) {
                    self.active_tab = idx;
                    self.reset_log_view();
                    self.save_ui_state();
                }
            }
            Message::PaletteClick(idx) => {
                // Row click: select + run, same as navigating with ↑↓ then Enter.
                if let Some(cmd) = self.palette_filtered.get(idx).cloned() {
                    self.palette_selected = idx;
                    self.layer = Layer::Dashboard;
                    self.remember_command(&cmd.id);
                    self.update(cmd.action);
                }
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
        }
    }

    /// Route a bracketed paste to whichever text input is active; ignore otherwise.
    /// Text inputs are single-line, so newlines/control chars are dropped.
    pub(super) fn apply_paste(&mut self, text: &str) {
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
    pub(super) fn push_state_event(&mut self, name: &str, state: &ServiceState) {
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
        let ts_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
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
    pub(super) fn service_open_url(&self, name: &str, port: Option<u16>) -> String {
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

    /// The install finished (either way): release the held app-service start.
    fn deps_done(&mut self) {
        self.deps_pending = false;
        if let Some(names) = self.held_app_start.take() {
            self.effects.push_back(Effect::StartAppServices(names));
        }
    }

    /// Mark the tracked operation done: success clears it, failure keeps it
    /// visible (with the last output line) until the next run or 30s.
    /// Empty labels (pause_services resume pings) are ignored.
    fn finish_operation(&mut self, label: &str, ok: bool) {
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

    /// Queue a clipboard copy of the entries `select` picks from the buffer, narrowed to
    /// the active tab, formatted by `format`; `copy` words the outcome (see `CopyKind`).
    fn copy_logs(
        &mut self,
        select: impl for<'a> FnOnce(&'a logs::LogBuffer) -> Vec<&'a LogEntry>,
        format: fn(&[&LogEntry], &ClipboardContext) -> String,
        copy: CopyKind,
    ) {
        let filter = self.tab_filter();
        let entries: Vec<&LogEntry> = select(&self.logs.log_buffer)
            .into_iter()
            .filter(|e| filter.as_deref().is_none_or(|s| e.service == s))
            .collect();
        if entries.is_empty()
            && let Some(msg) = copy.empty_toast
        {
            self.set_toast(msg, ToastVariant::Info);
            return;
        }
        let filter = match copy.fallback_label {
            Some(label) => filter.or_else(|| Some(label.to_string())),
            None => filter,
        };
        let ctx = self.clipboard_ctx(filter);
        let text = format(&entries, &ctx);
        let success = (copy.success)(entries.len());
        self.effects
            .push_back(Effect::CopyToClipboard { text, success });
    }

    /// Current tab filter: Some(service_name) when on a service tab, None on All.
    fn tab_filter(&self) -> Option<String> {
        if self.active_tab > 0 {
            self.tabs.get(self.active_tab).cloned()
        } else {
            None
        }
    }

    /// Queue a clipboard copy of `text`, toasting `success` on completion.
    pub(super) fn copy_text(&mut self, text: String, success: String) {
        self.effects
            .push_back(Effect::CopyToClipboard { text, success });
    }

    /// Build a ClipboardContext scoped to the active tab.
    fn clipboard_ctx(&self, filter: Option<String>) -> ClipboardContext {
        ClipboardContext {
            project_name: self.config.name.clone(),
            services: self
                .manager
                .service_names()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            filter,
        }
    }
}

/// Map a scheduler note to a sidebar event kind.
pub(super) fn classify_note(message: &str) -> EventKind {
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
