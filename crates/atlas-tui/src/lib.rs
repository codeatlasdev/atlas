pub mod config;
pub mod event;
pub mod headless;
pub mod runtime;
pub mod theme;
pub mod tui;

use std::path::Path;

use anyhow::Result;

/// Entry point: run the atlas dev TUI
pub async fn run(root_dir: &Path) -> Result<()> {
    tui::run(root_dir).await
}

/// Entry point: run without TUI (headless mode for CI/scripts)
pub async fn run_headless(root_dir: &Path) -> Result<()> {
    headless::run(root_dir).await
}
