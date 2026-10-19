pub mod config;
pub mod control;
pub mod event;
pub mod headless;
pub mod mcp;
pub mod runner;
pub mod runtime;
pub(crate) mod sig;
pub mod status;
pub mod theme;
pub mod tui;
pub mod update_notice;

use std::path::Path;

use anyhow::Result;

pub use config::SelectionRequest;

/// Entry point: run the atlas dev TUI
pub async fn run(root_dir: &Path, select: &SelectionRequest) -> Result<()> {
    tui::run(root_dir, select).await
}

/// Entry point: run without TUI (headless mode for CI/scripts)
pub async fn run_headless(root_dir: &Path, select: &SelectionRequest) -> Result<()> {
    runner::run(root_dir, select, runner::RunKind::Foreground).await
}

/// Entry point: the daemonized runner process (`atlas dev --runner`, spawned by `--detach`).
pub async fn run_runner(root_dir: &Path, select: &SelectionRequest) -> Result<()> {
    runner::run(root_dir, select, runner::RunKind::Background).await
}

/// `atlas dev --detach`: start the runner in the background.
pub async fn detach(root_dir: &Path, select: &SelectionRequest) -> Result<runner::Detached> {
    runner::detach(root_dir, select).await
}

/// Entry point: CI mode — starts services, waits for health, prints JSON, exits 0/1.
pub async fn run_ci(root_dir: &Path, timeout_secs: u64, select: &SelectionRequest) -> Result<()> {
    headless::run_ci(root_dir, timeout_secs, select).await
}
