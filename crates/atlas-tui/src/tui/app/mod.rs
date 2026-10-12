mod chrome;
#[cfg(test)]
mod chrome_tests;
mod control;
mod input;
mod logview;
mod messages;
mod palette;
mod remote;
#[cfg(test)]
mod remote_tests;
mod state;
mod update;
#[cfg(test)]
mod update_tests;

pub use logview::{MatchPos, SearchMode, TabBadge};
use messages::Effect;
pub use messages::{Layer, Message, Operation, PaletteCommand, ServiceMenuAction, ToastVariant};
pub use remote::{Leave, Remote, RemoteEvent, spawn_reader};
pub use state::LevelFilter;
use state::{LogPane, Sessions};

use std::cell::Cell;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::prelude::*;
use tokio::sync::mpsc;

use crate::config::ProjectConfig;
use crate::event::{Event, EventHandler};
use crate::runtime::manager::{ManagerEvent, ServiceManager};
use crate::runtime::service::ServiceState;

use super::clicks::{ClickMap, ScrollbarGeometry};
use super::views;
use super::widgets;

enum NextEvent {
    Control(Option<crate::control::ControlRequest>),
    /// Attached: one line from the runner's stream (None = reader task gone).
    Remote(Option<RemoteEvent>),
    Manager(Option<ManagerEvent>),
    Ui(Option<Event>),
    /// Frame-gap timer elapsed.
    Wake,
}

/// Max manager events applied per loop iteration, and the time slice they may take.
const MANAGER_DRAIN_MAX: usize = 4096;
const MANAGER_DRAIN_BUDGET: Duration = Duration::from_millis(8);
/// Minimum gap between draws (~30 fps) unless an input event just happened.
const MIN_FRAME_GAP: Duration = Duration::from_millis(33);
/// Terminal size assumed until the first resize event.
const INITIAL_SIZE: (u16, u16) = (80, 24);
/// Sidebar event log length.
const EVENT_LOG_CAPACITY: usize = 50;
/// Palette rows assumed before the first draw reports the real count.
const PALETTE_ROWS_GUESS: usize = 10;

// --- App State ---
pub struct App {
    pub config: ProjectConfig,
    pub manager: ServiceManager,
    pub manager_rx: mpsc::UnboundedReceiver<ManagerEvent>,
    pub events: EventHandler,
    /// True when the atlas binary is a symlink (atlas link active) or a debug build.
    pub dev_mode: bool,

    // UI state
    pub layer: Layer,
    pub active_tab: usize,
    pub tabs: Vec<String>,
    /// Log pane: buffer, scroll/follow, view toggles, search.
    pub logs: LogPane,
    /// Active session tabs and mobile build details.
    pub sessions: Sessions,
    effects: VecDeque<Effect>,
    pub dirty: bool,
    pub toast: Option<(String, ToastVariant, Instant)>,
    pub should_quit: bool,
    pub size: (u16, u16),
    pub started: bool,
    /// `pre_command` / dependency install still running: app services wait for it
    /// (like tools/dev's synchronous ensureDeps). Cleared by DepsInstalled/DepsFailed.
    pub deps_pending: bool,
    /// App-service start requested by InfraReady while `deps_pending`.
    held_app_start: Option<Vec<String>>,
    pub splash_done: bool,
    pub splash_start: Instant,
    pub tick_count: u32,
    pub quit_selected: usize,
    /// Whether the tunnel should be killed on exit (toggled with `t`).
    pub tunnel_kill_on_exit: bool,

    // Mouse support
    /// Hit targets recorded by the last `view()`; the single source of truth for clicks.
    pub clicks: ClickMap,
    /// Scrollbar track being dragged (geometry captured at press time).
    pub drag_scrollbar: Option<ScrollbarGeometry>,
    // Command Palette
    pub palette_input: String,
    pub palette_filtered: Vec<PaletteCommand>,
    pub palette_selected: usize,
    /// First visible palette row (scroll window start).
    pub palette_offset: usize,
    /// Rows the palette list drew last frame; scroll handlers keep the selection inside.
    pub palette_rows: Cell<usize>,
    /// `z`: sidebar hidden regardless of width.
    pub zoom: bool,
    /// Help overlay scroll offset and the largest valid one (set while drawing).
    pub help_scroll: usize,
    pub help_max_scroll: Cell<usize>,
    /// Persisted UI memory (`.logs/.atlas-ui.json`).
    pub ui: super::ui_state::UiState,

    // Event log for sidebar
    pub event_log: VecDeque<widgets::events::SideEvent>,

    /// Terminal mouse capture is on (off lets the terminal select text).
    pub mouse_capture: bool,
    /// Terminal needs a full clear + redraw (after resume from suspend).
    pub force_redraw: bool,

    /// Most recent non-session action run — drives the sidebar OPERATIONS block.
    pub operation: Option<Operation>,
    /// Newer atlas version the update modal was already shown for (once per version per session).
    pub update_announced: Option<String>,
    /// The user accepted "update when you quit" (brand/footer show `⬆ update on exit`).
    pub update_on_exit: bool,
    /// Requests from `atlas mcp` over `.logs/atlas.sock` (None = control disabled).
    pub control_rx: Option<mpsc::UnboundedReceiver<crate::control::ControlRequest>>,
    /// Attached client of a runner (None = this App owns its services).
    pub remote: Option<Remote>,
    /// Attached: the runner's event stream.
    pub remote_rx: Option<mpsc::UnboundedReceiver<RemoteEvent>>,
}

impl App {
    pub fn new(
        config: ProjectConfig,
        manager: ServiceManager,
        manager_rx: mpsc::UnboundedReceiver<ManagerEvent>,
        events: EventHandler,
        dev_mode: bool,
    ) -> Self {
        let mut tabs = vec!["All".to_string()];
        for name in manager.app_service_names() {
            tabs.push(name.to_string());
        }
        // Tunnel ssh output gets its own tab — only when it starts at boot.
        // An on-demand tunnel (open_tunnel action) adds the tab when it starts.
        if manager.tunnel_starts_at_boot() {
            tabs.push(crate::runtime::tunnel::LOG_NAME.to_string());
        }
        Self {
            config,
            manager,
            manager_rx,
            events,
            dev_mode,
            layer: Layer::Dashboard,
            active_tab: 0,
            tabs,
            logs: LogPane::default(),
            sessions: Sessions::default(),
            effects: VecDeque::new(),
            dirty: true,
            toast: None,
            should_quit: false,
            size: INITIAL_SIZE,
            started: false,
            deps_pending: false,
            held_app_start: None,
            splash_done: false,
            splash_start: Instant::now(),
            tick_count: 0,
            quit_selected: 1,
            tunnel_kill_on_exit: true,
            clicks: ClickMap::default(),
            drag_scrollbar: None,
            palette_input: String::new(),
            palette_filtered: Vec::new(),
            palette_selected: 0,
            event_log: VecDeque::with_capacity(EVENT_LOG_CAPACITY),
            operation: None,
            update_announced: None,
            update_on_exit: false,
            mouse_capture: true,
            force_redraw: false,
            palette_offset: 0,
            palette_rows: Cell::new(PALETTE_ROWS_GUESS),
            zoom: false,
            help_scroll: 0,
            help_max_scroll: Cell::new(0),
            ui: super::ui_state::UiState::default(),
            control_rx: None,
            remote: None,
            remote_rx: None,
        }
    }

    /// Make this App a client of the runner behind `remote` (see `app/remote.rs`).
    pub fn attach(&mut self, remote: Remote, rx: mpsc::UnboundedReceiver<RemoteEvent>) {
        self.remote = Some(remote);
        self.remote_rx = Some(rx);
        // The runner owns the exit policy; `t` is refused while attached.
        self.started = true;
    }

    /// Sender into the channel `apply_manager_event` drains (OS signals use it).
    pub fn manager_sender(&self) -> mpsc::UnboundedSender<ManagerEvent> {
        self.manager.event_tx.clone()
    }

    pub fn toast_info(&mut self, msg: impl Into<String>) {
        self.set_toast(msg, ToastVariant::Info);
    }

    /// Optional keymap rows that apply to this session.
    pub fn key_ctx(&self) -> super::keymap::KeyCtx {
        super::keymap::KeyCtx {
            has_tunnel: self.manager.has_tunnel(),
            attached: self.remote.is_some(),
        }
    }

    /// Run the app, and ALWAYS stop services afterwards — even when the loop errors.
    pub async fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> anyhow::Result<()>
    where
        B::Error: Send + Sync + 'static,
    {
        let result = self.run_loop(terminal).await;
        // Attached: the services belong to the runner and outlive this client.
        if self.remote.is_none() {
            self.manager.stop_all().await;
        }
        result
    }

    /// Apply manager events until the channel is empty, the budget is spent, or Quit arrives.
    /// Returns (applied, channel_closed).
    fn drain_manager(&mut self) -> (usize, bool) {
        let start = Instant::now();
        let mut applied = 0;
        while applied < MANAGER_DRAIN_MAX {
            match self.manager_rx.try_recv() {
                Ok(event) => {
                    self.apply_manager_event(event);
                    applied += 1;
                    // Quit has priority: don't keep chewing through a log flood.
                    if self.should_quit {
                        break;
                    }
                    // Checking the clock every event is measurable at 100k+/s.
                    if applied.is_multiple_of(64) && start.elapsed() >= MANAGER_DRAIN_BUDGET {
                        break;
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => return (applied, false),
                Err(mpsc::error::TryRecvError::Disconnected) => return (applied, true),
            }
        }
        (applied, false)
    }

    /// Attached: apply the runner's pending stream events, bounded like the manager
    /// drain so input is never starved by a log flood. Returns whether anything was applied.
    fn drain_remote(&mut self) -> bool {
        let start = Instant::now();
        let mut applied = 0;
        while applied < MANAGER_DRAIN_MAX {
            let Some(rx) = self.remote_rx.as_mut() else {
                break;
            };
            match rx.try_recv() {
                Ok(ev) => {
                    self.handle_remote_event(ev);
                    applied += 1;
                    if self.should_quit {
                        break;
                    }
                    if applied.is_multiple_of(64) && start.elapsed() >= MANAGER_DRAIN_BUDGET {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let errors = self.drain_remote_errors();
        applied > 0 || errors
    }

    /// Handle every pending UI event. Consecutive ticks collapse into one.
    /// Returns (saw_input, channel_closed).
    fn drain_ui(&mut self) -> (bool, bool) {
        let mut saw_input = false;
        let mut tick_seen = false;
        // Bound the pass so a paste/mouse storm can't starve the manager drain.
        for _ in 0..MANAGER_DRAIN_MAX {
            match self.events.try_next() {
                Ok(event) => {
                    if matches!(event, Event::Tick) {
                        if std::mem::replace(&mut tick_seen, true) {
                            continue;
                        }
                    } else {
                        saw_input = true;
                    }
                    self.handle_ui_event(event);
                    if self.should_quit {
                        break;
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => return (saw_input, false),
                Err(mpsc::error::TryRecvError::Disconnected) => return (saw_input, true),
            }
        }
        (saw_input, false)
    }

    fn handle_ui_event(&mut self, event: Event) {
        // Tick only marks dirty when there's active animation —
        // services starting/stopping, splash, or toast visible.
        // All other events always mark dirty.
        let is_tick = matches!(event, Event::Tick);
        if let Some(message) = self.map_event(event) {
            self.update(message);
        }
        if is_tick {
            // A sliding time range ("last 5 min") ages lines out on its own:
            // redraw about once a second (the index throttles the same way).
            let sliding = matches!(
                self.logs.time_range,
                crate::runtime::timerange::TimeRange::LastMinutes(_)
            ) && self.tick_count.is_multiple_of(10);
            let animated = self.toast.is_some()
                || !self.splash_done
                || self.operation.is_some()
                || sliding
                || self.manager.any_service_transitioning();
            if animated {
                self.dirty = true;
            }
        } else {
            self.dirty = true;
        }
    }

    async fn run_loop<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> anyhow::Result<()>
    where
        B::Error: Send + Sync + 'static,
    {
        terminal.draw(|frame| self.view(frame))?;
        let mut last_draw = Instant::now();
        if self.remote.is_none() {
            self.manager.start_all().await;
            // start_all is now fire-and-forget: AllStarted event sets self.started
        }

        let mut manager_closed = false;
        let mut ui_closed = false;

        loop {
            // Manager first so Quit (SIGTERM/SIGHUP) beats a log flood; then input.
            if !manager_closed {
                let (applied, closed) = self.drain_manager();
                manager_closed = closed;
                if applied > 0 {
                    self.dirty = true;
                }
            }
            if self.drain_remote() {
                self.dirty = true;
            }
            let mut saw_input = false;
            if !ui_closed && !self.should_quit {
                let (input, closed) = self.drain_ui();
                saw_input = input;
                ui_closed = closed;
            }

            self.execute_effects().await;

            if self.should_quit || (manager_closed && ui_closed) {
                break;
            }

            if self.force_redraw {
                self.force_redraw = false;
                terminal.clear()?;
                self.dirty = true;
            }

            // Render only when something changed, capped at ~30 fps unless the
            // user just pressed something (input stays instant).
            if self.dirty && (saw_input || last_draw.elapsed() >= MIN_FRAME_GAP) {
                self.dirty = false;
                terminal.draw(|frame| self.view(frame))?;
                last_draw = Instant::now();
            }

            // Backlog left (budget hit): keep going, but yield so tasks/signals run.
            if (!manager_closed && !self.manager_rx.is_empty())
                || self.remote_rx.as_ref().is_some_and(|r| !r.is_empty())
            {
                tokio::task::yield_now().await;
                continue;
            }

            // Idle: sleep until something arrives. A pending (frame-capped) draw
            // wakes us when the gap elapses.
            let draw_wait = self
                .dirty
                .then(|| MIN_FRAME_GAP.saturating_sub(last_draw.elapsed()));
            let manager_rx = &mut self.manager_rx;
            let events = &mut self.events;
            let control_rx = &mut self.control_rx;
            let remote_rx = &mut self.remote_rx;
            let next = tokio::select! {
                ev = async { remote_rx.as_mut().unwrap().recv().await }, if remote_rx.is_some() => NextEvent::Remote(ev),
                manager = manager_rx.recv(), if !manager_closed => NextEvent::Manager(manager),
                ui = events.next(), if !ui_closed => NextEvent::Ui(ui),
                req = async { control_rx.as_mut().unwrap().recv().await }, if control_rx.is_some() => NextEvent::Control(req),
                _ = tokio::time::sleep(draw_wait.unwrap_or_default()), if draw_wait.is_some() => NextEvent::Wake,
            };
            match next {
                NextEvent::Control(Some(req)) => self.handle_control(req),
                NextEvent::Control(None) => self.control_rx = None,
                NextEvent::Remote(Some(ev)) => self.handle_remote_event(ev),
                NextEvent::Remote(None) => {
                    self.remote_rx = None;
                    self.handle_remote_event(RemoteEvent::Closed);
                }
                NextEvent::Manager(Some(event)) => {
                    self.apply_manager_event(event);
                    self.dirty = true;
                }
                NextEvent::Manager(None) => manager_closed = true,
                NextEvent::Ui(Some(event)) => {
                    let is_input = !matches!(event, Event::Tick);
                    self.handle_ui_event(event);
                    if is_input {
                        // Input renders immediately on the next pass regardless of frame gap.
                        last_draw = Instant::now() - MIN_FRAME_GAP;
                    }
                }
                NextEvent::Ui(None) => ui_closed = true,
                NextEvent::Wake => {}
            }
        }
        Ok(())
    }

    pub fn view(&self, frame: &mut Frame) {
        // Widgets re-record every hit target they draw; nothing persists across frames.
        self.clicks.clear();
        if !self.splash_done {
            views::splash::render(frame, self);
        } else {
            views::dashboard::render(frame, self);
        }

        // Overlays
        match &self.layer {
            Layer::Help => {
                let open_svcs = self.manager.open_services();
                views::help::render(
                    frame,
                    &self.config.actions,
                    &open_svcs,
                    self.key_ctx(),
                    self.help_scroll,
                    &self.help_max_scroll,
                    &self.clicks,
                );
            }
            Layer::Quit => {
                let mode = if self.remote.is_some() {
                    views::quit::Mode::Attached
                } else {
                    views::quit::Mode::Owner
                };
                views::quit::render(frame, self.quit_selected, mode, &self.clicks)
            }
            Layer::CommandPalette => {
                widgets::command_palette::render(
                    frame,
                    &self.palette_input,
                    &self.palette_filtered,
                    self.palette_selected,
                    self.palette_offset,
                    &self.palette_rows,
                    &self.clicks,
                );
            }
            Layer::ServiceMenu(name) => {
                let state = self
                    .manager
                    .services()
                    .iter()
                    .find(|s| s.name == *name)
                    .map(|s| s.state.clone())
                    .unwrap_or(ServiceState::Stopped);
                views::service_menu::render(frame, name, &state, &self.clicks);
            }
            Layer::DevicePicker {
                action_key: _,
                platform,
                devices,
                selected,
                detecting,
            } => {
                views::device_picker::render(
                    frame,
                    platform,
                    devices,
                    *selected,
                    *detecting,
                    self.tick_count as usize,
                    &self.clicks,
                );
            }
            Layer::Confirm { title, body, .. } => {
                views::confirm::render(frame, title, body, &self.clicks)
            }
            Layer::Dashboard | Layer::SessionAttached(_) => {}
        }

        // Toast
        if let Some((msg, variant, instant)) = &self.toast {
            let age_ms = instant.elapsed().as_millis() as u64;
            widgets::toast::render(
                frame,
                msg,
                variant,
                self.tick_count as usize,
                age_ms,
                &self.clicks,
            );
        }
    }

    /// Move the log view by `delta` wrapped rows (positive = older).
    /// Reaching the bottom re-enables follow.
    fn scroll_logs(&mut self, delta: isize) {
        let max = self.logs.log_viewport.get().max_scroll;
        let cur = self.logs.log_scroll.min(max) as isize;
        self.logs.log_scroll = cur.saturating_add(delta).clamp(0, max as isize) as usize;
        self.set_follow(self.logs.log_scroll == 0);
    }

    fn log_page(&self) -> isize {
        self.logs.log_viewport.get().height.saturating_sub(1).max(1) as isize
    }

    /// Single place follow flips: remembers where a pause began (for "N new").
    fn set_follow(&mut self, follow: bool) {
        if !follow && self.logs.follow_logs {
            self.logs.paused_since_seq = Some(self.logs.log_buffer.next_seq());
        }
        if follow {
            self.logs.paused_since_seq = None;
        }
        self.logs.follow_logs = follow;
    }

    /// Jump to the newest line and follow. Also the "tab opened" hook: the
    /// active tab counts as viewed, clearing its activity / error badge.
    fn reset_log_view(&mut self) {
        self.logs.log_scroll = 0;
        self.logs.log_hscroll = 0;
        self.logs.find_current = usize::MAX;
        self.set_follow(true);
        self.mark_tab_seen();
    }
}

#[cfg(test)]
mod tests;
