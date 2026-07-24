use std::process::Command;

use atlas_core::ports::ssh::CommandOutput;
use atlas_core::{AtlasError, Result};

/// SSH client that wraps the system `ssh` binary.
/// Uses ~/.ssh/config for host aliases, keys, and options.
#[derive(Debug, Clone)]
pub struct SshClient {
    host: String,
}

impl SshClient {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.to_string(),
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    /// Execute a command on the remote host via system ssh.
    /// Uses spawn_blocking to avoid blocking the tokio runtime.
    pub async fn exec(&self, command: &str) -> Result<CommandOutput> {
        let host = self.host.clone();
        let cmd = command.to_string();

        let output = tokio::task::spawn_blocking(move || {
            Command::new("ssh")
                .args(["-o", "StrictHostKeyChecking=accept-new"])
                .args(["-o", "ConnectTimeout=10"])
                .args(["-o", "BatchMode=yes"])
                .arg(&host)
                .arg(&cmd)
                .output()
        })
        .await
        .map_err(|e| AtlasError::Ssh(format!("task join error: {e}")))?
        .map_err(|e| AtlasError::Ssh(format!("ssh command failed: {e}")))?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Quick connectivity check — runs `echo ok` and verifies output.
    pub async fn is_reachable(&self) -> bool {
        match self.exec("echo ok").await {
            Ok(output) => output.exit_code == 0 && output.stdout.trim() == "ok",
            Err(_) => false,
        }
    }
}
