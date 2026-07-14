#![allow(unused)]

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::activity::ActivityDetector;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptDelivery {
    InCommand,
    AfterStart,
    Acp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionMode {
    Autonomous,
    Supervised,
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStatus {
    Authorized,
    Unauthorized,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    pub prompt: String,
    pub cwd: PathBuf,
    pub permission: PermissionMode,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    async fn resolve_binary(&self) -> Option<PathBuf>;
    async fn check_auth(&self) -> AuthStatus;
    fn launch_command(&self, config: &LaunchConfig) -> Vec<String>;
    fn prompt_delivery(&self) -> PromptDelivery;
    fn activity_detector(&self) -> Option<Box<dyn ActivityDetector>>;
    fn permission_flags(&self, mode: &PermissionMode) -> Vec<String>;
}
