use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::server::ServiceInfo;
use crate::domain::service::ServiceState;
use crate::Result;

#[async_trait]
pub trait ServerManager: Send + Sync {
    async fn list_services(&self, server_id: Uuid) -> Result<Vec<ServiceInfo>>;
    async fn service_status(&self, server_id: Uuid, unit_name: &str) -> Result<ServiceState>;
    async fn restart_service(&self, server_id: Uuid, unit_name: &str) -> Result<()>;
    async fn stop_service(&self, server_id: Uuid, unit_name: &str) -> Result<()>;
    async fn service_logs(
        &self,
        server_id: Uuid,
        unit_name: &str,
        lines: u32,
    ) -> Result<String>;
    async fn deploy(&self, server_id: Uuid, service_name: &str) -> Result<String>;
}
