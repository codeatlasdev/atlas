#![allow(unused)]

use std::path::PathBuf;

use async_trait::async_trait;

use atlas_agent::{
    ActivityDetector, ActivityState, AgentAdapter, AuthStatus, LaunchConfig, PermissionMode,
    PromptDelivery,
};

use crate::manifest::PluginManifest;

pub struct PluginAgentAdapter {
    manifest: PluginManifest,
}

impl PluginAgentAdapter {
    pub fn new(manifest: PluginManifest) -> Self {
        Self { manifest }
    }

    fn resolve_template(&self, template: &[String], config: &LaunchConfig) -> Vec<String> {
        template
            .iter()
            .map(|part| {
                part.replace("{binary}", &self.manifest.binary)
                    .replace("{prompt}", &config.prompt)
            })
            .collect()
    }
}

#[async_trait]
impl AgentAdapter for PluginAgentAdapter {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn version(&self) -> &str {
        &self.manifest.version
    }

    async fn resolve_binary(&self) -> Option<PathBuf> {
        which::which(&self.manifest.binary).ok()
    }

    async fn check_auth(&self) -> AuthStatus {
        AuthStatus::Unknown
    }

    fn launch_command(&self, config: &LaunchConfig) -> Vec<String> {
        let template = if self.manifest.modes.contains(&"acp".to_string())
            && config.permission == PermissionMode::Autonomous
        {
            &self.manifest.launch.acp
        } else if self.manifest.modes.contains(&"headless".to_string()) {
            &self.manifest.launch.headless
        } else {
            &self.manifest.launch.interactive
        };
        self.resolve_template(template, config)
    }

    fn prompt_delivery(&self) -> PromptDelivery {
        match self.manifest.prompt_delivery.as_str() {
            "after_start" => PromptDelivery::AfterStart,
            "acp" => PromptDelivery::Acp,
            _ => PromptDelivery::InCommand,
        }
    }

    fn activity_detector(&self) -> Option<Box<dyn ActivityDetector>> {
        None
    }

    fn permission_flags(&self, mode: &PermissionMode) -> Vec<String> {
        match mode {
            PermissionMode::Autonomous => self.manifest.permissions.autonomous.clone(),
            PermissionMode::Supervised => self.manifest.permissions.supervised.clone(),
            PermissionMode::ReadOnly => self.manifest.permissions.read_only.clone(),
        }
    }
}
