pub mod app;
pub mod views;
pub mod widgets;

use std::io;
use std::path::Path;

use anyhow::Result;
use crossterm::{
    execute,
    event::{DisableMouseCapture, EnableMouseCapture},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::config;
use crate::event::EventHandler;
use crate::runtime::lock::SingletonLock;
use crate::runtime::manager::ServiceManager;

use self::app::App;

pub async fn run(root_dir: &Path) -> Result<()> {
    // Load config
    let cfg = config::load(root_dir)?;

    // Acquire singleton lock
    let log_dir = root_dir.join(".logs");
    std::fs::create_dir_all(&log_dir)?;
    let _lock = SingletonLock::acquire(&log_dir.join(".tui.lock"))?;

    // Setup manager
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let manager = ServiceManager::new(root_dir.to_path_buf(), &cfg, event_tx);

    // Setup terminal
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let events = EventHandler::new(std::time::Duration::from_millis(100));
    let mut app = App::new(cfg, manager, event_rx, events);

    // Run
    let result = app.run(&mut terminal).await;

    // Restore terminal
    terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}
