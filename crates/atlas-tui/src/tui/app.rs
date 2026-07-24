use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use crate::config::ProjectConfig;
use crate::event::{Event, EventHandler};
use crate::runtime::logs::{self, ClipboardContext, LogBuffer, LogEntry, LogStream};
use crate::runtime::manager::{ManagerEvent, ServiceManager};
use crate::runtime::service::ServiceState;

use super::views;
use super::widgets;

// --- Messages (TEA) ---
#[derive(Debug, Clone)]
pub enum Message {
    Quit,
    Tick,
    Resize(u16, u16),
    SwitchTab(usize),
    NextTab,
    PrevTab,
    ServiceStateChanged { name: String, state: ServiceState },
    LogLine { name: String, line: String },
    RestartAll,
    ShowHelp,
    HideHelp,
    ShowQuit,
    HideQuit,
    ToggleQuitSelection,
    Toast(String, ToastVariant),
    DismissToast,
    ScrollDown,
    ScrollUp,
    ScrollToBottom,
    ScrollToTop,
    HealthCheckDone,
    // Mouse
    MouseClick { x: u16, y: u16 },
    MouseScroll { down: bool },
    // Command Palette
    ShowPalette,
    HidePalette,
    PaletteInput(char),
    PaletteBackspace,
    PaletteSelect,
    PaletteUp,
    PaletteDown,
    // Log management
    CopyLogs,
    CopyLogsForAI,
    CopyErrors,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToastVariant {
    Info,
    Success,
    Error,
}

// --- Command Palette ---
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub name: String,
    pub description: String,
    pub shortcut: Option<String>,
    pub action: Message,
}

// --- Layers (focus management) ---
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Dashboard,
    Help,
    Quit,
    CommandPalette,
}

// --- App State ---
pub struct App {
    pub config: ProjectConfig,
    pub manager: ServiceManager,
    pub manager_rx: mpsc::UnboundedReceiver<ManagerEvent>,
    pub events: EventHandler,

    // UI state
    pub layer: Layer,
    pub active_tab: usize,
    pub tabs: Vec<String>,
    pub log_buffer: LogBuffer,
    pub log_scroll: usize,
    pub follow_logs: bool,
    pub toast: Option<(String, ToastVariant, Instant)>,
    pub should_quit: bool,
    pub size: (u16, u16),
    pub started: bool,
    pub splash_done: bool,
    pub splash_start: Instant,
    pub tick_count: u32,
    pub quit_selected: usize,

    // Mouse support
    pub tab_areas: Vec<(u16, u16, u16)>, // (x_start, x_end, tab_index)

    // Command Palette
    pub palette_input: String,
    pub palette_filtered: Vec<PaletteCommand>,
    pub palette_selected: usize,
}

impl App {
    pub fn new(
        config: ProjectConfig,
        manager: ServiceManager,
        manager_rx: mpsc::UnboundedReceiver<ManagerEvent>,
        events: EventHandler,
    ) -> Self {
        let mut tabs = vec!["All".to_string()];
        for name in manager.service_names() {
            tabs.push(name.to_string());
        }
        Self {
            config,
            manager,
            manager_rx,
            events,
            layer: Layer::Dashboard,
            active_tab: 0,
            tabs,
            log_buffer: LogBuffer::new(5000),
            log_scroll: 0,
            follow_logs: true,
            toast: None,
            should_quit: false,
            size: (80, 24),
            started: false,
            splash_done: false,
            splash_start: Instant::now(),
            tick_count: 0,
            quit_selected: 1,
            tab_areas: Vec::new(),
            palette_input: String::new(),
            palette_filtered: Vec::new(),
            palette_selected: 0,
        }
    }

    pub fn compute_tab_areas(&mut self) {
        let mut areas = Vec::new();
        let mut x: u16 = 1; // left padding
        for (i, tab) in self.tabs.iter().enumerate() {
            let width = (tab.len() + 2) as u16; // " tab " padding
            areas.push((x, x + width, i as u16));
            x += width + 2; // gap for dot + space
        }
        self.tab_areas = areas;
    }

    fn palette_commands() -> Vec<PaletteCommand> {
        vec![
            PaletteCommand {
                name: "Restart All".into(),
                description: "Restart all services".into(),
                shortcut: Some("r".into()),
                action: Message::RestartAll,
            },
            PaletteCommand {
                name: "Quit".into(),
                description: "Stop all and exit".into(),
                shortcut: Some("q".into()),
                action: Message::ShowQuit,
            },
            PaletteCommand {
                name: "Help".into(),
                description: "Show shortcuts".into(),
                shortcut: Some("?".into()),
                action: Message::ShowHelp,
            },
            PaletteCommand {
                name: "Scroll to Bottom".into(),
                description: "Jump to latest logs".into(),
                shortcut: Some("G".into()),
                action: Message::ScrollToBottom,
            },
            PaletteCommand {
                name: "Scroll to Top".into(),
                description: "Jump to first log".into(),
                shortcut: Some("g".into()),
                action: Message::ScrollToTop,
            },
            PaletteCommand {
                name: "Copy Logs".into(),
                description: "Copy recent logs to clipboard".into(),
                shortcut: Some("L".into()),
                action: Message::CopyLogs,
            },
            PaletteCommand {
                name: "Copy for AI".into(),
                description: "Copy logs formatted for AI prompt".into(),
                shortcut: Some("P".into()),
                action: Message::CopyLogsForAI,
            },
            PaletteCommand {
                name: "Copy Errors".into(),
                description: "Copy only errors".into(),
                shortcut: Some("E".into()),
                action: Message::CopyErrors,
            },
        ]
    }

    fn filter_palette(&self) -> Vec<PaletteCommand> {
        let query = self.palette_input.to_lowercase();
        if query.is_empty() {
            return Self::palette_commands();
        }
        Self::palette_commands()
            .into_iter()
            .filter(|cmd| {
                cmd.name.to_lowercase().contains(&query)
                    || cmd.description.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> anyhow::Result<()>
    where
        B::Error: Send + Sync + 'static,
    {
        // Start services
        self.manager.start_all().await;
        self.started = true;

        loop {
            // Compute tab areas before render (for mouse hit testing)
            self.compute_tab_areas();

            // Render
            terminal.draw(|frame| self.view(frame))?;

            // Drain manager events (non-blocking)
            while let Ok(event) = self.manager_rx.try_recv() {
                match event {
                    ManagerEvent::StateChanged { name, state } => {
                        self.update(Message::ServiceStateChanged { name, state });
                    }
                    ManagerEvent::LogLine { name, line } => {
                        self.update(Message::LogLine { name, line });
                    }
                    ManagerEvent::AllStarted => {}
                    ManagerEvent::AllStopped => {}
                }
            }

            // Wait for next event
            if let Some(event) = self.events.next().await {
                let msg = self.map_event(event);
                if let Some(m) = msg {
                    self.update(m);
                }
            }

            // Periodic health check every 50 ticks (5 seconds)
            if self.tick_count.is_multiple_of(50) && self.tick_count > 0 && self.started {
                self.manager.check_health().await;
            }

            if self.should_quit {
                self.manager.stop_all().await;
                break;
            }
        }
        Ok(())
    }

    fn map_event(&self, event: Event) -> Option<Message> {
        match event {
            Event::Tick => Some(Message::Tick),
            Event::Resize(w, h) => Some(Message::Resize(w, h)),
            Event::Key(key) => self.map_key(key),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => Some(Message::MouseClick {
                    x: mouse.column,
                    y: mouse.row,
                }),
                MouseEventKind::ScrollDown => Some(Message::MouseScroll { down: true }),
                MouseEventKind::ScrollUp => Some(Message::MouseScroll { down: false }),
                _ => None,
            },
        }
    }

    fn map_key(&self, key: crossterm::event::KeyEvent) -> Option<Message> {
        match self.layer {
            Layer::Help => match key.code {
                KeyCode::Esc | KeyCode::Char('?') => Some(Message::HideHelp),
                _ => None,
            },
            Layer::Quit => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    if self.quit_selected == 1 {
                        Some(Message::Quit)
                    } else {
                        Some(Message::HideQuit)
                    }
                }
                KeyCode::Esc | KeyCode::Char('n') => Some(Message::HideQuit),
                KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
                    Some(Message::ToggleQuitSelection)
                }
                _ => None,
            },
            Layer::CommandPalette => match key.code {
                KeyCode::Esc => Some(Message::HidePalette),
                KeyCode::Enter => Some(Message::PaletteSelect),
                KeyCode::Backspace => Some(Message::PaletteBackspace),
                KeyCode::Up => Some(Message::PaletteUp),
                KeyCode::Down => Some(Message::PaletteDown),
                KeyCode::Char(c) => Some(Message::PaletteInput(c)),
                _ => None,
            },
            Layer::Dashboard => match key.code {
                KeyCode::Char('q') => Some(Message::ShowQuit),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(Message::Quit)
                }
                KeyCode::Char('?') => Some(Message::ShowHelp),
                KeyCode::Char('r') => Some(Message::RestartAll),
                KeyCode::Char(':') => Some(Message::ShowPalette),
                KeyCode::Char('L') => Some(Message::CopyLogs),
                KeyCode::Char('P') => Some(Message::CopyLogsForAI),
                KeyCode::Char('E') => Some(Message::CopyErrors),
                KeyCode::Tab => Some(Message::NextTab),
                KeyCode::BackTab => Some(Message::PrevTab),
                KeyCode::Char('0') => Some(Message::SwitchTab(0)),
                KeyCode::Char('1') => Some(Message::SwitchTab(1)),
                KeyCode::Char('2') => Some(Message::SwitchTab(2)),
                KeyCode::Char('3') => Some(Message::SwitchTab(3)),
                KeyCode::Char('4') => Some(Message::SwitchTab(4)),
                KeyCode::Char('5') => Some(Message::SwitchTab(5)),
                KeyCode::Char('6') => Some(Message::SwitchTab(6)),
                KeyCode::Char('j') | KeyCode::Down => Some(Message::ScrollDown),
                KeyCode::Char('k') | KeyCode::Up => Some(Message::ScrollUp),
                KeyCode::Char('G') => Some(Message::ScrollToBottom),
                KeyCode::Char('g') => Some(Message::ScrollToTop),
                _ => None,
            },
        }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::Quit => self.should_quit = true,
            Message::Tick => {
                self.tick_count += 1;
                // Auto-dismiss toast
                if let Some((_, _, created)) = &self.toast {
                    if created.elapsed() > Duration::from_secs(3) {
                        self.toast = None;
                    }
                }
                // Splash done
                if !self.splash_done
                    && self.splash_start.elapsed() > Duration::from_millis(1500)
                {
                    self.splash_done = true;
                }
            }
            Message::Resize(w, h) => self.size = (w, h),
            Message::SwitchTab(idx) => {
                if idx < self.tabs.len() {
                    self.active_tab = idx;
                    self.follow_logs = true;
                }
            }
            Message::NextTab => {
                self.active_tab = (self.active_tab + 1) % self.tabs.len();
                self.follow_logs = true;
            }
            Message::PrevTab => {
                self.active_tab =
                    (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
                self.follow_logs = true;
            }
            Message::ServiceStateChanged { name, state } => {
                self.manager.update_state(&name, state);
            }
            Message::LogLine { name, line } => {
                self.log_buffer.push(name, line, LogStream::Stdout);
                if self.follow_logs {
                    self.log_scroll = self.log_buffer.len().saturating_sub(1);
                }
            }
            Message::RestartAll => {
                self.toast = Some((
                    "Restarting...".to_string(),
                    ToastVariant::Info,
                    Instant::now(),
                ));
            }
            Message::ShowHelp => self.layer = Layer::Help,
            Message::HideHelp => self.layer = Layer::Dashboard,
            Message::ShowQuit => {
                self.layer = Layer::Quit;
                self.quit_selected = 1;
            }
            Message::HideQuit => self.layer = Layer::Dashboard,
            Message::ToggleQuitSelection => {
                self.quit_selected = 1 - self.quit_selected;
            }
            Message::Toast(msg, variant) => {
                self.toast = Some((msg, variant, Instant::now()));
            }
            Message::DismissToast => self.toast = None,
            Message::ScrollDown => {
                self.follow_logs = false;
                self.log_scroll = self
                    .log_scroll
                    .saturating_add(1)
                    .min(self.log_buffer.len().saturating_sub(1));
            }
            Message::ScrollUp => {
                self.follow_logs = false;
                self.log_scroll = self.log_scroll.saturating_sub(1);
            }
            Message::ScrollToBottom => {
                self.follow_logs = true;
                self.log_scroll = self.log_buffer.len().saturating_sub(1);
            }
            Message::ScrollToTop => {
                self.follow_logs = false;
                self.log_scroll = 0;
            }
            Message::HealthCheckDone => {}
            // Mouse
            Message::MouseClick { x, y } => {
                // Header area (row 0-1) = tabs
                if y < 2 {
                    for (start, end, idx) in &self.tab_areas {
                        if x >= *start && x < *end {
                            self.active_tab = *idx as usize;
                            self.follow_logs = true;
                            break;
                        }
                    }
                }
            }
            Message::MouseScroll { down } => {
                if down {
                    self.update(Message::ScrollDown);
                } else {
                    self.update(Message::ScrollUp);
                }
            }
            // Command Palette
            Message::ShowPalette => {
                self.layer = Layer::CommandPalette;
                self.palette_input.clear();
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
            }
            Message::HidePalette => self.layer = Layer::Dashboard,
            Message::PaletteInput(c) => {
                self.palette_input.push(c);
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
            }
            Message::PaletteBackspace => {
                self.palette_input.pop();
                self.palette_filtered = self.filter_palette();
                self.palette_selected = 0;
            }
            Message::PaletteUp => {
                self.palette_selected = self.palette_selected.saturating_sub(1);
            }
            Message::PaletteDown => {
                if self.palette_selected < self.palette_filtered.len().saturating_sub(1) {
                    self.palette_selected += 1;
                }
            }
            Message::PaletteSelect => {
                if let Some(cmd) = self.palette_filtered.get(self.palette_selected).cloned() {
                    self.layer = Layer::Dashboard;
                    self.update(cmd.action);
                }
            }
            Message::CopyLogs => {
                let entries: Vec<_> = self.log_buffer.since(180);
                let ctx = ClipboardContext {
                    project_name: self.config.name.clone(),
                    services: self
                        .manager
                        .service_names()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    filter: if self.active_tab > 0 {
                        self.tabs.get(self.active_tab).cloned()
                    } else {
                        None
                    },
                };
                let text = logs::format_for_clipboard(&entries, &ctx);
                match logs::copy_to_clipboard(&text) {
                    Ok(_) => {
                        self.toast = Some((
                            "✓ Logs copied (3min)".to_string(),
                            ToastVariant::Success,
                            Instant::now(),
                        ));
                    }
                    Err(_) => {
                        self.toast = Some((
                            "✗ Clipboard failed".to_string(),
                            ToastVariant::Error,
                            Instant::now(),
                        ));
                    }
                }
            }
            Message::CopyLogsForAI => {
                let entries: Vec<_> = self.log_buffer.since(300);
                let ctx = ClipboardContext {
                    project_name: self.config.name.clone(),
                    services: self
                        .manager
                        .service_names()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    filter: if self.active_tab > 0 {
                        self.tabs.get(self.active_tab).cloned()
                    } else {
                        None
                    },
                };
                let text = logs::format_for_ai_prompt(&entries, &ctx);
                match logs::copy_to_clipboard(&text) {
                    Ok(_) => {
                        self.toast = Some((
                            "✓ AI prompt copied".to_string(),
                            ToastVariant::Success,
                            Instant::now(),
                        ));
                    }
                    Err(_) => {
                        self.toast = Some((
                            "✗ Clipboard failed".to_string(),
                            ToastVariant::Error,
                            Instant::now(),
                        ));
                    }
                }
            }
            Message::CopyErrors => {
                let entries = self.log_buffer.errors();
                if entries.is_empty() {
                    self.toast = Some((
                        "No errors found".to_string(),
                        ToastVariant::Info,
                        Instant::now(),
                    ));
                    return;
                }
                let ctx = ClipboardContext {
                    project_name: self.config.name.clone(),
                    services: self
                        .manager
                        .service_names()
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    filter: Some("errors only".to_string()),
                };
                let text = logs::format_for_clipboard(&entries, &ctx);
                match logs::copy_to_clipboard(&text) {
                    Ok(_) => {
                        self.toast = Some((
                            format!("✓ {} errors copied", entries.len()),
                            ToastVariant::Success,
                            Instant::now(),
                        ));
                    }
                    Err(_) => {
                        self.toast = Some((
                            "✗ Clipboard failed".to_string(),
                            ToastVariant::Error,
                            Instant::now(),
                        ));
                    }
                }
            }
        }
    }

    pub fn view(&self, frame: &mut Frame) {
        if !self.splash_done {
            views::splash::render(frame, self);
        } else {
            views::dashboard::render(frame, self);
        }

        // Overlays
        match self.layer {
            Layer::Help => views::help::render(frame),
            Layer::Quit => views::quit::render(frame, self.quit_selected),
            Layer::CommandPalette => {
                widgets::command_palette::render(
                    frame,
                    &self.palette_input,
                    &self.palette_filtered,
                    self.palette_selected,
                );
            }
            Layer::Dashboard => {}
        }

        // Toast
        if let Some((ref msg, ref variant, _)) = self.toast {
            widgets::toast::render(frame, msg, variant, self.tick_count as usize);
        }
    }

    pub fn filtered_logs(&self) -> Vec<&LogEntry> {
        if self.active_tab == 0 {
            self.log_buffer.all().iter().collect()
        } else if let Some(name) = self.tabs.get(self.active_tab) {
            self.log_buffer.filter_service(name)
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProjectConfig, ServiceDef};
    use crate::event::EventHandler;
    use crate::runtime::manager::ServiceManager;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    fn test_config() -> ProjectConfig {
        ProjectConfig {
            name: "test".to_string(),
            tunnel: None,
            services: {
                let mut m = HashMap::new();
                m.insert(
                    "web".to_string(),
                    ServiceDef {
                        command: "echo hello".to_string(),
                        port: Some(3000),
                        health: None,
                        critical: None,
                        depends_on: None,
                        enabled: None,
                    },
                );
                m
            },
            infra: None,
        }
    }

    fn make_app() -> App {
        let (tx, rx) = mpsc::unbounded_channel();
        let cfg = test_config();
        let mgr = ServiceManager::new(PathBuf::from("/tmp"), &cfg, tx);
        let (_, events) = EventHandler::test_channel();
        App::new(cfg, mgr, rx, events)
    }

    #[test]
    fn test_update_switch_tab() {
        let mut app = make_app();
        assert_eq!(app.active_tab, 0);
        app.update(Message::SwitchTab(1));
        assert_eq!(app.active_tab, 1);
    }

    #[test]
    fn test_update_next_tab_wraps() {
        let mut app = make_app();
        let n = app.tabs.len();
        for _ in 0..n {
            app.update(Message::NextTab);
        }
        assert_eq!(app.active_tab, 0);
    }

    #[test]
    fn test_update_log_line() {
        let mut app = make_app();
        app.update(Message::LogLine {
            name: "web".to_string(),
            line: "listening".to_string(),
        });
        assert_eq!(app.log_buffer.len(), 1);
        assert_eq!(app.log_buffer.all().back().unwrap().content, "listening");
    }

    #[test]
    fn test_update_toast_autodismiss() {
        let mut app = make_app();
        app.update(Message::Toast("hi".to_string(), ToastVariant::Info));
        assert!(app.toast.is_some());
        app.update(Message::DismissToast);
        assert!(app.toast.is_none());
    }

    #[test]
    fn test_layer_transitions() {
        let mut app = make_app();
        assert_eq!(app.layer, Layer::Dashboard);
        app.update(Message::ShowHelp);
        assert_eq!(app.layer, Layer::Help);
        app.update(Message::HideHelp);
        assert_eq!(app.layer, Layer::Dashboard);
    }

    #[test]
    fn test_filtered_logs() {
        let mut app = make_app();
        app.update(Message::LogLine {
            name: "web".to_string(),
            line: "line1".to_string(),
        });
        app.update(Message::LogLine {
            name: "api".to_string(),
            line: "line2".to_string(),
        });
        app.update(Message::LogLine {
            name: "web".to_string(),
            line: "line3".to_string(),
        });

        // Tab 0 = All
        assert_eq!(app.filtered_logs().len(), 3);

        // Tab 1 = web
        app.active_tab = 1;
        let filtered = app.filtered_logs();
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].content, "line1");
    }

    #[test]
    fn test_scroll() {
        let mut app = make_app();
        for i in 0..10 {
            app.update(Message::LogLine {
                name: "web".to_string(),
                line: format!("line {i}"),
            });
        }

        assert!(app.follow_logs);
        app.update(Message::ScrollUp);
        assert!(!app.follow_logs);
        app.update(Message::ScrollToBottom);
        assert!(app.follow_logs);
    }

    #[test]
    fn test_quit_flow() {
        let mut app = make_app();
        assert_eq!(app.layer, Layer::Dashboard);

        app.update(Message::ShowQuit);
        assert_eq!(app.layer, Layer::Quit);
        assert_eq!(app.quit_selected, 1);

        app.update(Message::ToggleQuitSelection);
        assert_eq!(app.quit_selected, 0);

        app.update(Message::HideQuit);
        assert_eq!(app.layer, Layer::Dashboard);
    }

    #[test]
    fn test_tick_count_increments() {
        let mut app = make_app();
        assert_eq!(app.tick_count, 0);
        app.update(Message::Tick);
        app.update(Message::Tick);
        assert_eq!(app.tick_count, 2);
    }

    #[test]
    fn test_quit_confirm_only_when_quit_selected() {
        let mut app = make_app();
        app.update(Message::ShowQuit);

        // Toggle to Cancel (0)
        app.update(Message::ToggleQuitSelection);
        assert_eq!(app.quit_selected, 0);

        assert!(!app.should_quit);

        // Toggle back to Quit (1)
        app.update(Message::ToggleQuitSelection);
        assert_eq!(app.quit_selected, 1);
    }

    #[test]
    fn test_palette_filter() {
        let mut app = make_app();

        app.update(Message::ShowPalette);
        assert_eq!(app.layer, Layer::CommandPalette);
        assert!(!app.palette_filtered.is_empty());

        app.update(Message::PaletteInput('r'));
        app.update(Message::PaletteInput('e'));
        // Should filter to "Restart All"
        assert!(app.palette_filtered.iter().any(|c| c.name.contains("Restart")));
    }

    #[test]
    fn test_palette_select_executes() {
        let mut app = make_app();

        app.update(Message::ShowPalette);
        // First command should be available
        app.update(Message::PaletteSelect);
        assert_eq!(app.layer, Layer::Dashboard); // Palette closes after select
    }

    #[test]
    fn test_palette_navigation() {
        let mut app = make_app();

        app.update(Message::ShowPalette);
        assert_eq!(app.palette_selected, 0);

        app.update(Message::PaletteDown);
        assert_eq!(app.palette_selected, 1);

        app.update(Message::PaletteDown);
        assert_eq!(app.palette_selected, 2);

        app.update(Message::PaletteUp);
        assert_eq!(app.palette_selected, 1);

        // Can't go below 0
        app.update(Message::PaletteUp);
        app.update(Message::PaletteUp);
        assert_eq!(app.palette_selected, 0);
    }

    #[test]
    fn test_palette_backspace() {
        let mut app = make_app();

        app.update(Message::ShowPalette);
        app.update(Message::PaletteInput('r'));
        app.update(Message::PaletteInput('e'));
        assert_eq!(app.palette_input, "re");

        app.update(Message::PaletteBackspace);
        assert_eq!(app.palette_input, "r");

        app.update(Message::PaletteBackspace);
        assert_eq!(app.palette_input, "");
    }

    #[test]
    fn test_palette_hide() {
        let mut app = make_app();

        app.update(Message::ShowPalette);
        assert_eq!(app.layer, Layer::CommandPalette);

        app.update(Message::HidePalette);
        assert_eq!(app.layer, Layer::Dashboard);
    }

    #[test]
    fn test_mouse_click_tab() {
        let mut app = make_app();
        app.compute_tab_areas();

        // Click on second tab area (if it exists)
        if app.tab_areas.len() > 1 {
            let (start, _, _) = app.tab_areas[1];
            app.update(Message::MouseClick { x: start, y: 0 });
            assert_eq!(app.active_tab, 1);
        }
    }

    #[test]
    fn test_mouse_scroll() {
        let mut app = make_app();

        app.update(Message::LogLine {
            name: "web".to_string(),
            line: "line1".to_string(),
        });
        app.update(Message::LogLine {
            name: "web".to_string(),
            line: "line2".to_string(),
        });

        app.update(Message::MouseScroll { down: true });
        assert!(!app.follow_logs);
    }

    #[test]
    fn test_compute_tab_areas() {
        let mut app = make_app();
        app.compute_tab_areas();

        // Should have areas for "All" and "web"
        assert_eq!(app.tab_areas.len(), 2);
        // First area starts at x=1
        assert_eq!(app.tab_areas[0].0, 1);
        // " All " = 5 chars wide
        assert_eq!(app.tab_areas[0].1, 6);
    }

    #[test]
    fn test_copy_logs_no_crash() {
        let mut app = make_app();

        app.update(Message::LogLine {
            name: "web".to_string(),
            line: "started".to_string(),
        });
        app.update(Message::LogLine {
            name: "web".to_string(),
            line: "Error: crash".to_string(),
        });

        // These should not crash even if clipboard isn't available
        app.update(Message::CopyLogs);
        app.update(Message::CopyLogsForAI);
        app.update(Message::CopyErrors);
    }

    #[test]
    fn test_copy_errors_empty() {
        let mut app = make_app();

        app.update(Message::LogLine {
            name: "web".to_string(),
            line: "all good".to_string(),
        });
        app.update(Message::CopyErrors);
        // Should show "No errors found" toast
        assert!(app.toast.is_some());
        assert!(app.toast.as_ref().unwrap().0.contains("No errors"));
    }
}
