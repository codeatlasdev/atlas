use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{AtlasError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub org: Option<String>,
    #[serde(default)]
    pub server: Option<ServerConfig>,
    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,
    #[serde(default)]
    pub deploy: Option<DeployConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub user: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub command: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub env_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    pub strategy: String,
    #[serde(default)]
    pub domain: Option<String>,
}

pub fn load_project(path: &Path) -> Result<ProjectConfig> {
    let yaml_path = path.join("atlas.yaml");
    let content = std::fs::read_to_string(&yaml_path).map_err(|e| {
        AtlasError::InvalidInput(format!(
            "failed to read atlas.yaml at {}: {e}",
            yaml_path.display()
        ))
    })?;

    serde_yaml::from_str(&content).map_err(|e| {
        AtlasError::InvalidInput(format!("failed to parse atlas.yaml: {e}"))
    })
}
