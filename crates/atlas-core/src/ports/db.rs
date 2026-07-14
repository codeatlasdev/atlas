use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::server::Server;
use crate::domain::service::SystemdService;
use crate::domain::session::Session;
use crate::Result;

#[async_trait]
pub trait ServerRepository: Send + Sync {
    async fn get_all(&self) -> Result<Vec<Server>>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Server>>;
    async fn create(&self, server: &Server) -> Result<()>;
    async fn update(&self, server: &Server) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
}

#[async_trait]
pub trait ServiceRepository: Send + Sync {
    async fn get_by_server(&self, server_id: Uuid) -> Result<Vec<SystemdService>>;
    async fn upsert(&self, service: &SystemdService) -> Result<()>;
    async fn delete(&self, id: Uuid) -> Result<()>;
}

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn get_active(&self) -> Result<Vec<Session>>;
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Session>>;
    async fn create(&self, session: &Session) -> Result<()>;
    async fn end_session(&self, id: Uuid) -> Result<()>;
}

#[async_trait]
pub trait ConfigRepository: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn set(&self, key: &str, value: &str) -> Result<()>;
    async fn delete(&self, key: &str) -> Result<()>;
}
