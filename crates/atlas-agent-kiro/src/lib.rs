#![allow(unused)]

pub mod acp;
pub mod detect;

use std::path::PathBuf;

use async_trait::async_trait;
use atlas_agent::{
    ActivityDetector, AgentAdapter, AuthStatus, LaunchConfig, PermissionMode, PromptDelivery,
};

use crate::detect::KiroActivityDetector;

pub struct KiroAdapter;

impl KiroAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KiroAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentAdapter for KiroAdapter {
    fn name(&self) -> &str {
        "kiro"
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    async fn resolve_binary(&self) -> Option<PathBuf> {
        which::which("kiro-cli").ok()
    }

    async fn check_auth(&self) -> AuthStatus {
        let output = tokio::process::Command::new("kiro-cli")
            .arg("whoami")
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => AuthStatus::Authorized,
            Ok(_) => AuthStatus::Unauthorized,
            Err(_) => AuthStatus::Unknown,
        }
    }

    fn launch_command(&self, config: &LaunchConfig) -> Vec<String> {
        let perm_flags = self.permission_flags(&config.permission);

        match config.permission {
            PermissionMode::Autonomous => {
                let mut cmd = vec![
                    "kiro-cli".to_string(),
                    "chat".to_string(),
                    "--no-interactive".to_string(),
                ];
                cmd.extend(perm_flags);
                cmd.push(config.prompt.clone());
                cmd
            }
            _ => {
                let mut cmd = vec![
                    "kiro-cli".to_string(),
                    "chat".to_string(),
                    "--wrap".to_string(),
                    "never".to_string(),
                ];
                cmd.extend(perm_flags);
                cmd.push(config.prompt.clone());
                cmd
            }
        }
    }

    fn prompt_delivery(&self) -> PromptDelivery {
        PromptDelivery::Acp
    }

    fn activity_detector(&self) -> Option<Box<dyn ActivityDetector>> {
        Some(Box::new(KiroActivityDetector::new()))
    }

    fn permission_flags(&self, mode: &PermissionMode) -> Vec<String> {
        match mode {
            PermissionMode::Autonomous => vec!["--trust-all-tools".to_string()],
            PermissionMode::Supervised => vec![],
            PermissionMode::ReadOnly => {
                vec!["--trust-tools".to_string(), "read,grep".to_string()]
            }
        }
    }
}
