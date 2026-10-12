use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};

use crate::event::Event;
use crate::runtime::service::ServiceState;
use crate::tui::keymap::SESSION_KEYS;

use super::{App, Layer, Message, ServiceMenuAction};

impl App {
    pub(super) fn map_event(&self, event: Event) -> Option<Message> {
        match event {
            Event::Tick => Some(Message::Tick),
            Event::Resize(w, h) => Some(Message::Resize(w, h)),
            Event::Key(key) => self.map_key(key),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left)
                    if mouse.modifiers == KeyModifiers::NONE =>
                {
                    Some(Message::MouseClick {
                        x: mouse.column,
                        y: mouse.row,
                    })
                }
                // Any click with modifiers (Shift+Cmd link click, etc.) — reset cursor shape
                MouseEventKind::Down(_) | MouseEventKind::Up(_)
                    if mouse.modifiers != KeyModifiers::NONE =>
                {
                    Some(Message::ResetCursor)
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    Some(Message::MouseDrag { y: mouse.row })
                }
                MouseEventKind::Up(MouseButton::Left) => Some(Message::MouseRelease),
                MouseEventKind::ScrollDown => Some(Message::MouseScroll {
                    x: mouse.column,
                    y: mouse.row,
                    down: true,
                }),
                MouseEventKind::ScrollUp => Some(Message::MouseScroll {
                    x: mouse.column,
                    y: mouse.row,
                    down: false,
                }),
                _ => None,
            },
            Event::FocusGained => Some(Message::ResetCursor),
            Event::Paste(text) => Some(Message::Paste(text)),
        }
    }

    pub(super) fn map_key(&self, key: crossterm::event::KeyEvent) -> Option<Message> {
        // Defensive: the reader already drops non-Press, but mapping must never double-fire.
        if key.kind != crossterm::event::KeyEventKind::Press {
            return None;
        }
        // Ctrl+C quits from every layer except an attached session (where it kills the
        // session). Handled before any layer mapping so text inputs can't swallow it.
        if key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && !matches!(self.layer, Layer::SessionAttached(_))
        {
            return Some(Message::Quit);
        }
        // Ctrl+Z suspends in every layer (raw mode swallows the tty's own SIGTSTP).
        if key.code == KeyCode::Char('z') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(Message::Suspend);
        }
        // ── Attached mode — all keys forwarded to session stdin except Ctrl+B / Ctrl+C ──
        if let Layer::SessionAttached(label) = &self.layer {
            return self.map_key_attached(key, label.clone());
        }

        // Second key of a `]` / `[` chord: anything else just cancels it.
        if self.logs.chord.is_some()
            && matches!(self.layer, Layer::Dashboard)
            && !self.logs.log_searching
        {
            let first = self.logs.chord.unwrap_or(']');
            return match key.code {
                KeyCode::Char(c) => self.chord_key(first, c).or(Some(Message::ChordStart('\0'))),
                _ => Some(Message::ChordStart('\0')),
            };
        }

        self.map_key_tui(key)
    }

    /// Key handling when ATTACHED to a session.
    /// Ctrl+B → detach. Ctrl+C → kill session + detach. Everything else → stdin.
    fn map_key_attached(&self, key: crossterm::event::KeyEvent, label: String) -> Option<Message> {
        match key.code {
            // Ctrl+B — detach back to Dashboard
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Message::DetachSession)
            }
            // Ctrl+C — kill the session and detach
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(Message::StopSession(label))
            }
            // All other chars — forward raw, checking input_map first
            KeyCode::Char(c) => {
                let bytes = self
                    .manager
                    .actions
                    .iter()
                    .find(|a| a.label == label)
                    .and_then(|a| {
                        let k = c.to_string();
                        a.input_map.get(&k).map(|v| v.as_bytes().to_vec())
                    })
                    .unwrap_or_else(|| c.to_string().into_bytes());
                Some(Message::SessionInput { label, bytes })
            }
            code => SESSION_KEYS
                .iter()
                .find(|(k, _)| *k == code)
                .map(|(_, bytes)| Message::SessionInput {
                    label,
                    bytes: bytes.to_vec(),
                }),
        }
    }

    /// Normal TUI key handling.
    fn map_key_tui(&self, key: crossterm::event::KeyEvent) -> Option<Message> {
        match &self.layer {
            Layer::Help => match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => Some(Message::HideHelp),
                KeyCode::Char('j') | KeyCode::Down => Some(Message::HelpScroll(1)),
                KeyCode::Char('k') | KeyCode::Up => Some(Message::HelpScroll(-1)),
                KeyCode::PageDown => Some(Message::HelpScroll(8)),
                KeyCode::PageUp => Some(Message::HelpScroll(-8)),
                KeyCode::Home | KeyCode::Char('g') => Some(Message::HelpScroll(isize::MIN)),
                KeyCode::End | KeyCode::Char('G') => Some(Message::HelpScroll(isize::MAX)),
                _ => None,
            },
            // Attached: [Detach (default)] [Stop everything]; Detach never touches services.
            Layer::Quit if self.remote.is_some() => match key.code {
                KeyCode::Char('d') => Some(Message::Detach),
                KeyCode::Char('s') => Some(Message::StopRunner),
                KeyCode::Enter => Some(if self.quit_selected == 1 {
                    Message::StopRunner
                } else {
                    Message::Detach
                }),
                KeyCode::Esc | KeyCode::Char('n') => Some(Message::HideQuit),
                KeyCode::Tab | KeyCode::Left | KeyCode::Right => Some(Message::ToggleQuitSelection),
                _ => None,
            },
            Layer::Quit => match key.code {
                KeyCode::Char('y') => Some(Message::Quit),
                KeyCode::Enter => {
                    if self.quit_selected == 1 {
                        Some(Message::Quit)
                    } else {
                        Some(Message::HideQuit)
                    }
                }
                KeyCode::Esc | KeyCode::Char('n') => Some(Message::HideQuit),
                KeyCode::Tab | KeyCode::Left | KeyCode::Right => Some(Message::ToggleQuitSelection),
                _ => None,
            },
            Layer::Confirm { .. } => match key.code {
                KeyCode::Enter | KeyCode::Char('y') => Some(Message::ConfirmAccept),
                KeyCode::Esc | KeyCode::Char('n') => Some(Message::ConfirmCancel),
                _ => None,
            },
            Layer::CommandPalette => match key.code {
                KeyCode::Esc => Some(Message::HidePalette),
                KeyCode::Enter => Some(Message::PaletteSelect),
                KeyCode::Backspace => Some(Message::PaletteBackspace),
                KeyCode::Up => Some(Message::PaletteUp),
                KeyCode::Down => Some(Message::PaletteDown),
                KeyCode::PageUp => Some(Message::PaletteUp),
                KeyCode::PageDown => Some(Message::PaletteDown),
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Message::PaletteClear)
                }
                KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Message::PaletteDeleteWord)
                }
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Message::PaletteDown)
                }
                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Message::PaletteUp)
                }
                // Other Ctrl/Alt chords are never text.
                KeyCode::Char(_)
                    if key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    None
                }
                KeyCode::Char(c) => Some(Message::PaletteInput(c)),
                _ => None,
            },
            Layer::Dashboard => {
                // The custom time-range prompt owns the keyboard while open.
                if self.logs.time_input.is_some() {
                    return self.map_time_key(key);
                }
                // On a session tab, `i` attaches; no stdin forwarding in read-only mode
                let on_session_tab = self
                    .tabs
                    .get(self.active_tab)
                    .map(|t| self.sessions.tabs.contains(t.as_str()))
                    .unwrap_or(false);

                if on_session_tab {
                    match key.code {
                        KeyCode::Char('i') => return Some(Message::AttachSession),
                        // Navigation works normally on session tabs
                        KeyCode::Tab => return Some(Message::NextTab),
                        KeyCode::BackTab => return Some(Message::PrevTab),
                        KeyCode::Char(c @ '0'..='9') => {
                            let idx = (c as usize) - ('0' as usize);
                            return if idx < self.tabs.len() {
                                Some(Message::SwitchTab(idx))
                            } else {
                                None
                            };
                        }
                        KeyCode::Char('j') | KeyCode::Down => return Some(Message::ScrollDown),
                        KeyCode::Char('k') | KeyCode::Up => return Some(Message::ScrollUp),
                        KeyCode::PageDown => return Some(Message::ScrollPageDown),
                        KeyCode::PageUp => return Some(Message::ScrollPageUp),
                        KeyCode::End => return Some(Message::ScrollToBottom),
                        KeyCode::Home => return Some(Message::ScrollToTop),
                        KeyCode::Char('G') => return Some(Message::ScrollToBottom),
                        KeyCode::Char('g') => return Some(Message::ScrollToTop),
                        KeyCode::Char('f') => return Some(Message::ToggleFollow),
                        KeyCode::Char('/') => return Some(Message::StartLogSearch),
                        KeyCode::Char(
                            c @ ('n' | 'N' | 'W' | 'T' | 'V' | 'J' | 'z' | 'o' | 'y' | '[' | ']'
                            | '{' | '}'),
                        ) if self.log_view_key(c).is_some() => {
                            return self.log_view_key(c);
                        }
                        KeyCode::Left if !self.logs.log_wrap => {
                            return Some(Message::PanLogs { right: false });
                        }
                        KeyCode::Right if !self.logs.log_wrap => {
                            return Some(Message::PanLogs { right: true });
                        }
                        KeyCode::Char('v') => return Some(Message::CycleLevelFilter),
                        KeyCode::Char('M') => return Some(Message::ToggleMouseCapture),
                        KeyCode::Char('L') => return Some(Message::CopyLogs),
                        KeyCode::Char('P') => return Some(Message::CopyLogsForAI),
                        KeyCode::Char('?') => return Some(Message::ShowHelp),
                        KeyCode::Char(':') => return Some(Message::ShowPalette),
                        KeyCode::Char('q') => return Some(Message::ShowQuit),
                        KeyCode::Char('D') => return Some(Message::Detach),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Some(Message::Quit);
                        }
                        // Everything else is a no-op on session tabs (read-only mode)
                        _ => return None,
                    }
                }
                // Log search intercept — typed keys go to the search bar
                if self.logs.log_searching {
                    return self.map_search_key(key);
                }

                match key.code {
                    KeyCode::Char('D') => Some(Message::Detach),
                    KeyCode::Char('M') => Some(Message::ToggleMouseCapture),
                    KeyCode::Char('v') => Some(Message::CycleLevelFilter),
                    KeyCode::Char('q') => Some(Message::ShowQuit),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        Some(Message::Quit)
                    }
                    KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        Some(Message::ClearLogs)
                    }
                    KeyCode::Char('?') => Some(Message::ShowHelp),
                    KeyCode::Char('r') => {
                        if self.active_tab > 0 {
                            self.tabs
                                .get(self.active_tab)
                                .cloned()
                                .map(Message::RestartService)
                        } else {
                            Some(Message::RestartAll)
                        }
                    }
                    KeyCode::Char(':') => Some(Message::ShowPalette),
                    KeyCode::Char('L') => Some(Message::CopyLogs),
                    KeyCode::Char('P') => Some(Message::CopyLogsForAI),
                    KeyCode::Char('E') => Some(Message::CopyErrors),
                    KeyCode::Tab => Some(Message::NextTab),
                    KeyCode::BackTab => Some(Message::PrevTab),
                    KeyCode::Char(c @ '0'..='9') => {
                        let idx = (c as usize) - ('0' as usize);
                        if idx < self.tabs.len() {
                            Some(Message::SwitchTab(idx))
                        } else {
                            None
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Down => Some(Message::ScrollDown),
                    KeyCode::Char('k') | KeyCode::Up => Some(Message::ScrollUp),
                    KeyCode::PageDown => Some(Message::ScrollPageDown),
                    KeyCode::PageUp => Some(Message::ScrollPageUp),
                    KeyCode::End => Some(Message::ScrollToBottom),
                    KeyCode::Home => Some(Message::ScrollToTop),
                    KeyCode::Char('G') => Some(Message::ScrollToBottom),
                    KeyCode::Char('g') => Some(Message::ScrollToTop),
                    KeyCode::Char('s') => {
                        if self.active_tab > 0 {
                            self.tabs
                                .get(self.active_tab)
                                .cloned()
                                .map(Message::StopService)
                        } else {
                            None
                        }
                    }
                    KeyCode::Char('S') => {
                        if self.active_tab > 0 {
                            self.tabs
                                .get(self.active_tab)
                                .cloned()
                                .map(Message::RestartService)
                        } else {
                            None
                        }
                    }
                    KeyCode::Char('t') if self.manager.has_tunnel() => {
                        Some(Message::ToggleTunnelOnExit)
                    }
                    KeyCode::Char('f') => Some(Message::ToggleFollow),
                    KeyCode::Char('m') => {
                        if self.active_tab > 0 {
                            self.tabs
                                .get(self.active_tab)
                                .cloned()
                                .map(Message::ShowServiceMenu)
                        } else {
                            None
                        }
                    }
                    // Log search
                    KeyCode::Char('/') => Some(Message::StartLogSearch),
                    KeyCode::Esc if !self.logs.log_search_input.is_empty() => {
                        Some(Message::LogSearchClear)
                    }
                    // Log view
                    KeyCode::Char(
                        c @ ('n' | 'N' | 'W' | 'T' | 'V' | 'J' | '-' | 'z' | 'o' | 'y' | '[' | ']'
                        | '{' | '}'),
                    ) if self.log_view_key(c).is_some() => self.log_view_key(c),
                    KeyCode::Left if !self.logs.log_wrap => Some(Message::PanLogs { right: false }),
                    KeyCode::Right if !self.logs.log_wrap => Some(Message::PanLogs { right: true }),
                    KeyCode::Char(c) => {
                        // Dynamic open_key bindings from atlas.yaml
                        if self.manager.open_services().iter().any(|(k, _, _)| *k == c) {
                            Some(Message::OpenService(c))
                        // Dynamic action bindings from atlas.yaml
                        } else if self.manager.actions.iter().any(|a| a.key == c) {
                            Some(Message::RunAction(c))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            Layer::ServiceMenu(name) => {
                let action = match key.code {
                    KeyCode::Esc => return Some(Message::HideServiceMenu),
                    KeyCode::Char('r') | KeyCode::Char('1') => ServiceMenuAction::Restart,
                    KeyCode::Char('s') | KeyCode::Char('2') => ServiceMenuAction::Stop,
                    KeyCode::Char('t') | KeyCode::Char('3') => ServiceMenuAction::Start,
                    _ => return None,
                };
                let state = self
                    .manager
                    .services()
                    .iter()
                    .find(|s| s.name == *name)
                    .map(|s| s.state.clone())
                    .unwrap_or(ServiceState::Stopped);
                crate::tui::views::service_menu::visible_actions(&state)
                    .contains(&action)
                    .then(|| Message::ServiceMenuAction {
                        name: name.clone(),
                        action,
                    })
            }
            // Attached mode is handled in map_key before reaching map_key_tui
            Layer::SessionAttached(_) => None,
            // Device picker navigation
            Layer::DevicePicker { .. } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => Some(Message::DevicePickerCancel),
                KeyCode::Enter => Some(Message::DevicePickerConfirm),
                KeyCode::Char('j') | KeyCode::Down => Some(Message::DevicePickerDown),
                KeyCode::Char('k') | KeyCode::Up => Some(Message::DevicePickerUp),
                _ => None,
            },
        }
    }
}
