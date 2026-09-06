use std::path::Path;

use anyhow::Result;
use tokio::signal;
use tokio::sync::mpsc;

use crate::config;
use crate::runtime::manager::{ManagerEvent, ServiceManager};
use crate::runtime::service::ServiceState;

pub async fn run(root_dir: &Path) -> Result<()> {
    let cfg = config::load(root_dir)?;

    println!("\x1b[1;34m●\x1b[0m atlas dev (headless)");
    println!("  project: {}", cfg.name);
    println!("  services: {}", cfg.services.len());
    println!();

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut manager = ServiceManager::new(root_dir.to_path_buf(), &cfg, event_tx);

    manager.start_all().await;

    println!("\x1b[1;32m✓\x1b[0m All services started. Press Ctrl+C to stop.");
    println!();

    // Forward log lines to stdout
    let log_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                ManagerEvent::LogLine { name, line, .. } => {
                    let prefix = if name.len() > 5 { &name[..5] } else { &name };
                    println!("[{prefix:<5}] {line}");
                }
                ManagerEvent::StateChanged { name, state } => {
                    let icon = match state {
                        ServiceState::Running => "\x1b[32m●\x1b[0m",
                        ServiceState::Starting => "\x1b[33m◌\x1b[0m",
                        ServiceState::Failed => "\x1b[31m✗\x1b[0m",
                        ServiceState::Stopped => "\x1b[90m○\x1b[0m",
                    };
                    eprintln!("{icon} {name}: {state:?}");
                }
                _ => {}
            }
        }
    });

    // Wait for Ctrl+C
    signal::ctrl_c().await?;

    println!();
    println!("\x1b[33m●\x1b[0m Shutting down...");

    manager.stop_all().await;
    log_task.abort();

    println!("\x1b[32m✓\x1b[0m Done.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headless_requires_config() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(config::load(dir.path()).is_err());
    }
}
