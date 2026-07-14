//! ACP Client Handler backed by the daemon's real infrastructure.
//!
//! When an agent running in ACP mode requests filesystem or terminal operations,
//! this handler executes them using the daemon's PtyManager and direct filesystem access.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use atlas_agent::AcpClientHandler;
use atlas_terminal::{PtyManager, SessionConfig};
use tracing::debug;

/// Handler that delegates agent→client requests to the daemon's real services.
pub struct DaemonClientHandler {
    cwd: PathBuf,
    pty_manager: Arc<PtyManager>,
}

impl DaemonClientHandler {
    pub fn new(cwd: PathBuf, pty_manager: Arc<PtyManager>) -> Self {
        Self { cwd, pty_manager }
    }
}

#[async_trait]
impl AcpClientHandler for DaemonClientHandler {
    async fn read_file(&self, path: &str) -> Result<String, String> {
        let full_path = resolve_and_validate_path(&self.cwd, path)?;

        debug!(path = %full_path.display(), "ACP: fs/readTextFile");

        tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| format!("read {}: {e}", full_path.display()))
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        let full_path = resolve_and_validate_path(&self.cwd, path)?;

        debug!(path = %full_path.display(), "ACP: fs/writeTextFile");

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("create dir {}: {e}", parent.display()))?;
        }

        tokio::fs::write(&full_path, content)
            .await
            .map_err(|e| format!("write {}: {e}", full_path.display()))
    }

    async fn terminal_create(&self, command: &str, cwd: &str) -> Result<String, String> {
        let cwd_path = if cwd == "." {
            self.cwd.clone()
        } else if cwd.starts_with('/') {
            PathBuf::from(cwd)
        } else {
            self.cwd.join(cwd)
        };

        debug!(command = command, cwd = %cwd_path.display(), "ACP: terminal/create");

        let (shell, args) = parse_command(command);

        let config = SessionConfig {
            shell,
            args,
            rows: 24,
            cols: 120,
            cwd: cwd_path,
            env: Default::default(),
        };

        self.pty_manager
            .create_session(config)
            .await
            .map_err(|e| format!("terminal create: {e}"))
    }

    async fn terminal_input(&self, terminal_id: &str, data: &str) -> Result<(), String> {
        debug!(terminal_id = terminal_id, "ACP: terminal/input");

        self.pty_manager
            .write_input(terminal_id, data.as_bytes())
            .await
            .map_err(|e| format!("terminal input: {e}"))
    }
}

fn parse_command(command: &str) -> (String, Vec<String>) {
    let parts: Vec<&str> = command.split_whitespace().collect();
    match parts.split_first() {
        Some((first, rest)) => (
            first.to_string(),
            rest.iter().map(|s| s.to_string()).collect(),
        ),
        None => ("bash".to_string(), vec![]),
    }
}

/// Resolve a path and validate it stays within the project cwd.
/// Prevents path traversal attacks from agent-requested file operations.
fn resolve_and_validate_path(cwd: &PathBuf, path: &str) -> Result<PathBuf, String> {
    let resolved = if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        cwd.join(path)
    };

    // Normalize path components (resolve .. and .)
    let mut normalized = PathBuf::new();
    for component in resolved.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            other => normalized.push(other),
        }
    }

    // Ensure the resolved path is within the cwd
    if !normalized.starts_with(cwd) {
        return Err(format!(
            "path traversal denied: {} escapes project root {}",
            path,
            cwd.display()
        ));
    }

    Ok(normalized)
}
