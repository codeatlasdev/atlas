use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;

use super::project::{InfraConfig, ProjectConfig, ServiceDef, TunnelConfig};

const CONFIG_FILE: &str = "atlas.yaml";
const LOCAL_CONFIG_FILE: &str = "atlas.local.yaml";

pub fn load(root_dir: &Path) -> anyhow::Result<ProjectConfig> {
    let config_path = root_dir.join(CONFIG_FILE);
    if !config_path.exists() {
        bail!("No {} found in {}", CONFIG_FILE, root_dir.display());
    }

    let base_content =
        std::fs::read_to_string(&config_path).context("Failed to read atlas.yaml")?;
    let mut config: ProjectConfig =
        serde_yaml::from_str(&base_content).context("Failed to parse atlas.yaml")?;

    let local_path = root_dir.join(LOCAL_CONFIG_FILE);
    if local_path.exists() {
        let local_content =
            std::fs::read_to_string(&local_path).context("Failed to read atlas.local.yaml")?;
        let local: LocalOverride =
            serde_yaml::from_str(&local_content).context("Failed to parse atlas.local.yaml")?;
        merge_local(&mut config, local);
    }

    apply_defaults(&mut config);
    Ok(config)
}

pub fn find_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(CONFIG_FILE).exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

#[derive(Debug, Deserialize)]
struct LocalOverride {
    name: Option<String>,
    tunnel: Option<LocalTunnelOverride>,
    services: Option<std::collections::HashMap<String, LocalServiceOverride>>,
    infra: Option<InfraConfig>,
}

#[derive(Debug, Deserialize)]
struct LocalTunnelOverride {
    enabled: Option<bool>,
    local_port: Option<u16>,
    remote_host: Option<String>,
    remote_port: Option<u16>,
    ssh_host: Option<String>,
    keepalive_interval: Option<u16>,
    keepalive_max: Option<u8>,
    reconnect_cooldown: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct LocalServiceOverride {
    command: Option<String>,
    port: Option<u16>,
    health: Option<String>,
    critical: Option<bool>,
    depends_on: Option<Vec<String>>,
    enabled: Option<bool>,
}

fn merge_local(config: &mut ProjectConfig, local: LocalOverride) {
    if let Some(name) = local.name {
        config.name = name;
    }

    if let Some(tunnel_override) = local.tunnel {
        if let Some(ref mut tunnel) = config.tunnel {
            merge_tunnel(tunnel, tunnel_override);
        }
    }

    if let Some(service_overrides) = local.services {
        for (name, override_def) in service_overrides {
            if let Some(service) = config.services.get_mut(&name) {
                merge_service(service, override_def);
            }
        }
    }

    if let Some(infra) = local.infra {
        config.infra = Some(infra);
    }
}

fn merge_tunnel(tunnel: &mut TunnelConfig, o: LocalTunnelOverride) {
    if let Some(v) = o.enabled {
        tunnel.enabled = Some(v);
    }
    if let Some(v) = o.local_port {
        tunnel.local_port = v;
    }
    if let Some(v) = o.remote_host {
        tunnel.remote_host = v;
    }
    if let Some(v) = o.remote_port {
        tunnel.remote_port = v;
    }
    if let Some(v) = o.ssh_host {
        tunnel.ssh_host = v;
    }
    if let Some(v) = o.keepalive_interval {
        tunnel.keepalive_interval = v;
    }
    if let Some(v) = o.keepalive_max {
        tunnel.keepalive_max = v;
    }
    if let Some(v) = o.reconnect_cooldown {
        tunnel.reconnect_cooldown = v;
    }
}

fn merge_service(service: &mut ServiceDef, o: LocalServiceOverride) {
    if let Some(v) = o.command {
        service.command = v;
    }
    if let Some(v) = o.port {
        service.port = Some(v);
    }
    if let Some(v) = o.health {
        service.health = Some(v);
    }
    if let Some(v) = o.critical {
        service.critical = Some(v);
    }
    if let Some(v) = o.depends_on {
        service.depends_on = Some(v);
    }
    if let Some(v) = o.enabled {
        service.enabled = Some(v);
    }
}

fn apply_defaults(config: &mut ProjectConfig) {
    if let Some(ref mut tunnel) = config.tunnel {
        if tunnel.keepalive_interval == 0 {
            tunnel.keepalive_interval = 15;
        }
        if tunnel.keepalive_max == 0 {
            tunnel.keepalive_max = 3;
        }
        if tunnel.reconnect_cooldown == 0 {
            tunnel.reconnect_cooldown = 30;
        }
    }
}
