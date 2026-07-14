use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use atlas_core::ports::ssh::{CommandOutput, SshPort};
use atlas_core::{AtlasError, Result};

use crate::session::ManagedSession;

pub struct SshClient {
    session: Arc<Mutex<Option<ManagedSession>>>,
    key_path: Option<std::path::PathBuf>,
}

impl SshClient {
    pub fn new(key_path: Option<std::path::PathBuf>) -> Self {
        Self {
            session: Arc::new(Mutex::new(None)),
            key_path,
        }
    }
}

#[async_trait]
impl SshPort for SshClient {
    async fn connect(&self, host: &str, port: u16, user: &str) -> Result<()> {
        let managed = ManagedSession::connect(host, port, user, self.key_path.as_deref()).await?;
        let mut session = self.session.lock().await;
        *session = Some(managed);
        Ok(())
    }

    async fn disconnect(&self) -> Result<()> {
        let mut session = self.session.lock().await;
        if let Some(s) = session.take() {
            s.close().await?;
        }
        Ok(())
    }

    async fn execute(&self, command: &str) -> Result<CommandOutput> {
        let session = self.session.lock().await;
        let s = session
            .as_ref()
            .ok_or_else(|| AtlasError::Ssh("not connected".to_string()))?;
        s.execute(command).await
    }

    async fn is_connected(&self) -> bool {
        let session = self.session.lock().await;
        session.is_some()
    }
}
