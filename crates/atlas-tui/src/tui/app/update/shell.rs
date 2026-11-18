//! Application shell: quit/tick/resize, toasts, modals (help, quit, confirm), paste,
//! mouse capture, zoom.

use super::super::{App, Effect, Layer, Leave, Message, ToastVariant};
use atlas_i18n::t;

impl App {
    /// Handle the shell messages; anything else is handed back for the next domain.
    pub(super) fn reduce_shell(&mut self, msg: Message) -> Option<Message> {
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
            Message::HealthCheckDone => {}
            Message::ResetCursor => {
                // Reset terminal cursor shape after Shift+Cmd link click or focus regain.
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::cursor::SetCursorStyle::DefaultUserShape
                );
            }
            Message::UpdateOnExit(version) => {
                self.update_on_exit = true;
                crate::update_notice::request_on_exit(version);
                self.set_toast(
                    t!("tui_runtime_app_toast_update_on_exit"),
                    ToastVariant::Info,
                );
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
            Message::Paste(text) => self.apply_paste(&text),
            Message::Suspend => self.effects.push_back(Effect::Suspend),
            Message::ToggleMouseCapture => {
                self.mouse_capture = !self.mouse_capture;
                let msg = if self.mouse_capture {
                    t!("tui_runtime_app_toast_mouse_on")
                } else {
                    t!("tui_runtime_app_toast_mouse_off")
                };
                self.set_toast(msg, ToastVariant::Info);
                self.effects
                    .push_back(Effect::SetMouseCapture(self.mouse_capture));
            }
            Message::ToggleZoom => {
                self.zoom = !self.zoom;
                self.save_ui_state();
            }
            other => return Some(other),
        }
        None
    }
}
