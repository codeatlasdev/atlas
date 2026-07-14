use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use atlas_core::domain::server::ServiceInfo;
use atlas_core::Result;
use atlas_ssh::SshClient;

use crate::systemd;

/// Manages SSH connections to multiple servers.
pub struct ServerManager {
    clients: Arc<RwLock<HashMap<String, SshClient>>>,
}

impl ServerManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register or update an SSH connection for a server.
    /// `host` should match ~/.ssh/config Host alias or be user@ip.
    pub async fn connect(&self, server_id: &str, host: &str) {
        let client = SshClient::new(host);
        let mut clients = self.clients.write().await;
        clients.insert(server_id.to_string(), client);
    }

    /// Remove a server's SSH client.
    pub async fn disconnect(&self, server_id: &str) {
        let mut clients = self.clients.write().await;
        clients.remove(server_id);
    }

    /// Get a clone of the SSH client for a server.
    async fn get_client(&self, server_id: &str) -> Result<SshClient> {
        let clients = self.clients.read().await;
        clients
            .get(server_id)
            .cloned()
            .ok_or_else(|| atlas_core::AtlasError::NotFound(format!("server not connected: {server_id}")))
    }

    /// Check if a server is reachable via SSH.
    pub async fn check_status(&self, server_id: &str) -> Result<bool> {
        let client = self.get_client(server_id).await?;
        Ok(client.is_reachable().await)
    }

    /// List running services on a server.
    pub async fn list_services(&self, server_id: &str) -> Result<Vec<ServiceInfo>> {
        let client = self.get_client(server_id).await?;
        systemd::list_services(&client).await
    }

    /// Get detailed status of a service.
    pub async fn service_status(&self, server_id: &str, unit: &str) -> Result<systemd::ServiceStatus> {
        let client = self.get_client(server_id).await?;
        systemd::service_status(&client, unit).await
    }

    /// Restart a service.
    pub async fn restart_service(&self, server_id: &str, unit: &str) -> Result<()> {
        let client = self.get_client(server_id).await?;
        systemd::restart_service(&client, unit).await
    }

    /// Stop a service.
    pub async fn stop_service(&self, server_id: &str, unit: &str) -> Result<()> {
        let client = self.get_client(server_id).await?;
        systemd::stop_service(&client, unit).await
    }

    /// Get recent logs for a service.
    pub async fn service_logs(&self, server_id: &str, unit: &str, lines: u32) -> Result<String> {
        let client = self.get_client(server_id).await?;
        systemd::service_logs(&client, unit, lines).await
    }

    /// Execute an arbitrary command on a server.
    pub async fn exec(&self, server_id: &str, command: &str) -> Result<atlas_core::ports::ssh::CommandOutput> {
        let client = self.get_client(server_id).await?;
        client.exec(command).await
    }
}

impl Default for ServerManager {
    fn default() -> Self {
        Self::new()
    }
}
