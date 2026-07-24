use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub tunnel: Option<TunnelConfig>,
    pub services: HashMap<String, ServiceDef>,
    pub infra: Option<InfraConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TunnelConfig {
    pub enabled: Option<bool>,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    pub ssh_host: String,
    #[serde(default = "default_keepalive_interval")]
    pub keepalive_interval: u16,
    #[serde(default = "default_keepalive_max")]
    pub keepalive_max: u8,
    #[serde(default = "default_reconnect_cooldown")]
    pub reconnect_cooldown: u16,
}

impl TunnelConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

fn default_keepalive_interval() -> u16 {
    15
}

fn default_keepalive_max() -> u8 {
    3
}

fn default_reconnect_cooldown() -> u16 {
    30
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceDef {
    pub command: String,
    pub port: Option<u16>,
    pub health: Option<String>,
    pub critical: Option<bool>,
    pub depends_on: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

impl ServiceDef {
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct InfraConfig {
    pub compose_file: String,
}
