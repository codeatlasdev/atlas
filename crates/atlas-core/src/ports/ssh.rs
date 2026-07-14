use async_trait::async_trait;

use crate::Result;

#[async_trait]
pub trait SshPort: Send + Sync {
    async fn connect(&self, host: &str, port: u16, user: &str) -> Result<()>;
    async fn disconnect(&self) -> Result<()>;
    async fn execute(&self, command: &str) -> Result<CommandOutput>;
    async fn is_connected(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
