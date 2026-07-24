use std::path::Path;

use atlas_core::ports::ssh::CommandOutput;
use atlas_core::{AtlasError, Result};

pub struct ManagedSession {
    _host: String,
    _port: u16,
    _user: String,
}

impl ManagedSession {
    pub async fn connect(
        host: &str,
        port: u16,
        user: &str,
        _key_path: Option<&Path>,
    ) -> Result<Self> {
        // TODO: implement actual russh connection
        // This will use russh::client::connect and handle key authentication
        tracing::info!(host, port, user, "establishing SSH connection");

        Err(AtlasError::Ssh(format!(
            "SSH connection to {user}@{host}:{port} not yet implemented"
        )))
    }

    pub async fn execute(&self, command: &str) -> Result<CommandOutput> {
        // TODO: implement command execution via russh channel
        tracing::debug!(command, "executing remote command");

        Err(AtlasError::Ssh(format!(
            "command execution not yet implemented: {command}"
        )))
    }

    pub async fn close(self) -> Result<()> {
        tracing::info!("closing SSH session");
        Ok(())
    }
}
