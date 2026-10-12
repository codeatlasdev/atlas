use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use schemars::JsonSchema;
use serde::Deserialize;

use super::project::{
    ActionConfig, AndroidConfig, DependsOn, InfraConfig, IosConfig, PreCommandWhen, ProjectConfig,
    RestartPolicy, ServiceDef, TunnelConfig,
};
use super::validate;

const CONFIG_FILE: &str = "atlas.yaml";
const LOCAL_CONFIG_FILE: &str = "atlas.local.yaml";

pub fn load(root_dir: &Path) -> anyhow::Result<ProjectConfig> {
    let (config, errors) = load_checked(root_dir)?;
    if !errors.is_empty() {
        bail!("{}", validate::render_errors(&errors, CONFIG_FILE));
    }
    Ok(config)
}

/// Parse atlas.yaml (+ atlas.local.yaml) and run every validation pass.
/// Hard failures (missing/unreadable file, malformed YAML, wrong types) are
/// `Err`; semantic problems (unknown keys, bad dependencies, duplicate ports…)
/// come back as the error list so `atlas config check` can print all of them.
pub fn load_checked(
    root_dir: &Path,
) -> anyhow::Result<(ProjectConfig, Vec<validate::ConfigError>)> {
    let config_path = root_dir.join(CONFIG_FILE);
    if !config_path.exists() {
        bail!("No {} found in {}", CONFIG_FILE, root_dir.display());
    }

    let mut errors = Vec::new();

    let base_content =
        std::fs::read_to_string(&config_path).context("Failed to read atlas.yaml")?;
    let base_value: serde_yaml::Value =
        serde_yaml::from_str(&base_content).context("Failed to parse atlas.yaml")?;
    errors.extend(validate::unknown_keys(
        &base_value,
        validate::Target::Project,
        CONFIG_FILE,
    ));
    let mut config: ProjectConfig =
        serde_yaml::from_value(base_value).map_err(|e| anyhow::anyhow!("{CONFIG_FILE}: {e}"))?;

    let local_path = root_dir.join(LOCAL_CONFIG_FILE);
    if local_path.exists() {
        let local_content =
            std::fs::read_to_string(&local_path).context("Failed to read atlas.local.yaml")?;
        let local_value: serde_yaml::Value =
            serde_yaml::from_str(&local_content).context("Failed to parse atlas.local.yaml")?;
        errors.extend(validate::unknown_keys(
            &local_value,
            validate::Target::Local,
            LOCAL_CONFIG_FILE,
        ));
        let local: LocalOverride = serde_yaml::from_value(local_value)
            .map_err(|e| anyhow::anyhow!("{LOCAL_CONFIG_FILE}: {e}"))?;
        merge_local(&mut config, local);
    }

    apply_defaults(&mut config);
    errors.extend(validate::semantic(&config));
    if let Some(n) = std::env::var(PORT_OFFSET_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|n| *n > 0)
    {
        apply_port_offset(&mut config, n);
    }
    Ok((config, errors))
}

/// Env var set by Atlas Desktop for a worktree's stack (`100`, `200`, …).
pub const PORT_OFFSET_ENV: &str = "ATLAS_PORT_OFFSET";

/// Shift every port by `n`: service ports, tunnel local port, `env` values equal
/// to some service's original port, and `:<port>` in `open_url`/`health`.
pub fn apply_port_offset(config: &mut ProjectConfig, n: u16) {
    let ports: Vec<u16> = config.services.values().filter_map(|s| s.port).collect();
    let shift = |p: u16| p.saturating_add(n);
    for svc in config.services.values_mut() {
        svc.port = svc.port.map(shift);
        for v in svc.env.values_mut() {
            if let Ok(p) = v.trim().parse::<u16>()
                && ports.contains(&p)
            {
                *v = shift(p).to_string();
            }
        }
        for url in [svc.open_url.as_mut(), svc.health.as_mut()]
            .into_iter()
            .flatten()
        {
            for p in &ports {
                let from = format!(":{p}");
                if let Some(i) = url.find(&from) {
                    let end = i + from.len();
                    if !url[end..].starts_with(|c: char| c.is_ascii_digit()) {
                        url.replace_range(i..end, &format!(":{}", shift(*p)));
                        break;
                    }
                }
            }
        }
    }
    if let Some(t) = config.tunnel.as_mut() {
        t.local_port = shift(t.local_port);
    }
    config.port_offset = n;
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

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub(super) struct LocalOverride {
    name: Option<String>,
    tunnel: Option<LocalTunnelOverride>,
    services: Option<std::collections::HashMap<String, LocalServiceOverride>>,
    infra: Option<InfraConfig>,
    pre_command: Option<String>,
    pre_command_when: Option<PreCommandWhen>,
    actions: Option<Vec<ActionConfig>>,
    mobile: Option<LocalMobileOverride>,
    profiles: Option<std::collections::BTreeMap<String, Vec<String>>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct LocalMobileOverride {
    ios: Option<IosConfig>,
    android: Option<AndroidConfig>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
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

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
struct LocalServiceOverride {
    command: Option<String>,
    port: Option<u16>,
    health: Option<String>,
    critical: Option<bool>,
    depends_on: Option<DependsOn>,
    enabled: Option<bool>,
    restart: Option<RestartPolicy>,
    max_restarts: Option<u32>,
    ready_log: Option<String>,
    oneshot: Option<bool>,
    start_timeout: Option<u64>,
    autostart: Option<bool>,
}

fn merge_local(config: &mut ProjectConfig, local: LocalOverride) {
    if let Some(name) = local.name {
        config.name = name;
    }

    if let Some(tunnel_override) = local.tunnel
        && let Some(tunnel) = &mut config.tunnel
    {
        merge_tunnel(tunnel, tunnel_override);
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

    // pre_command: empty string disables it, non-empty overrides, None keeps base
    match &local.pre_command {
        Some(s) if s.is_empty() => config.pre_command = None,
        Some(s) => config.pre_command = Some(s.clone()),
        None => {}
    }
    if let Some(when) = local.pre_command_when {
        config.pre_command_when = Some(when);
    }

    // actions: merge by key — new keys appended, matching keys overridden
    if let Some(overrides) = local.actions {
        for action in overrides {
            if let Some(existing) = config.actions.iter_mut().find(|a| a.key == action.key) {
                *existing = action;
            } else {
                config.actions.push(action);
            }
        }
    }

    // profiles: merge by name — same name replaces the list, new name is added
    if let Some(profiles) = local.profiles {
        config.profiles.extend(profiles);
    }

    // mobile: merge ios/android sections from atlas.local.yaml
    if let Some(mobile) = local.mobile {
        if let Some(ios) = mobile.ios {
            config.mobile.ios = Some(ios);
        }
        if let Some(android) = mobile.android {
            config.mobile.android = Some(android);
        }
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
        service.command = Some(v);
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
    if let Some(v) = o.restart {
        service.restart = v;
    }
    if let Some(v) = o.max_restarts {
        service.max_restarts = Some(v);
    }
    if let Some(v) = o.ready_log {
        service.ready_log = Some(v);
    }
    if let Some(v) = o.oneshot {
        service.oneshot = v;
    }
    if let Some(v) = o.start_timeout {
        service.start_timeout = Some(v);
    }
    if let Some(v) = o.autostart {
        service.autostart = Some(v);
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
