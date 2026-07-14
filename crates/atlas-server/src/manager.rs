use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

use atlas_core::domain::server::ServiceInfo;
use atlas_core::domain::service::ServiceState;
use atlas_core::ports::server_manager::ServerManager;
use atlas_core::ports::ssh::SshPort;
use atlas_core::Result;

use crate::systemd;

pub struct ServerManagerImpl {
    ssh: Arc<dyn SshPort>,
}

impl ServerManagerImpl {
    pub fn new(ssh: Arc<dyn SshPort>) -> Self {
        Self { ssh }
    }
}

#[async_trait]
impl ServerManager for ServerManagerImpl {
    async fn list_services(&self, _server_id: Uuid) -> Result<Vec<ServiceInfo>> {
        let output = self.ssh.execute("systemctl list-units --type=service --all --no-pager --plain").await?;
        let services = systemd::parse_list_units(&output.stdout);
        Ok(services)
    }

    async fn service_status(&self, _server_id: Uuid, unit_name: &str) -> Result<ServiceState> {
        let output = self
            .ssh
            .execute(&format!("systemctl is-active {unit_name}"))
            .await?;
        Ok(systemd::parse_state(&output.stdout))
    }

    async fn restart_service(&self, _server_id: Uuid, unit_name: &str) -> Result<()> {
        self.ssh
            .execute(&format!("sudo systemctl restart {unit_name}"))
            .await?;
        Ok(())
    }

    async fn stop_service(&self, _server_id: Uuid, unit_name: &str) -> Result<()> {
        self.ssh
            .execute(&format!("sudo systemctl stop {unit_name}"))
            .await?;
        Ok(())
    }

    async fn service_logs(&self, _server_id: Uuid, unit_name: &str, lines: u32) -> Result<String> {
        let output = self
            .ssh
            .execute(&format!("journalctl -u {unit_name} -n {lines} --no-pager"))
            .await?;
        Ok(output.stdout)
    }

    async fn deploy(&self, _server_id: Uuid, service_name: &str) -> Result<String> {
        crate::deploy::run_deploy(&*self.ssh, service_name).await
    }
}
