#![allow(unused)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub binary: String,
    pub modes: Vec<String>,
    pub prompt_delivery: String,
    pub activity_detection: ActivityDetectionConfig,
    pub permissions: PermissionsConfig,
    pub launch: LaunchTemplateConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityDetectionConfig {
    pub strategy: String,
    pub file_pattern: Option<String>,
    pub ready_pattern: Option<String>,
    pub active_pattern: Option<String>,
    pub idle_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsConfig {
    pub autonomous: Vec<String>,
    pub supervised: Vec<String>,
    pub read_only: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchTemplateConfig {
    pub interactive: Vec<String>,
    pub headless: Vec<String>,
    pub acp: Vec<String>,
}
