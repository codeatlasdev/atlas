mod loader;
mod project;
mod selection;
mod validate;

pub use loader::{PORT_OFFSET_ENV, apply_port_offset, find_root, load, load_checked};
pub use project::{
    ActionConfig, AndroidConfig, Condition, DEFAULT_START_TIMEOUT_SECS, DependsOn, DeployConfig,
    DeployHooks, DeployKind, DeployService, DeployTarget, HostMode, InfraConfig, IosConfig,
    MobileConfig, PreCommandWhen, ProjectConfig, RestartPolicy, ServiceDef, SessionPlatform,
    TunnelConfig, grouped_actions,
};
pub use selection::{Selection, SelectionRequest, resolve as resolve_selection};
pub use validate::{BUILTIN_KEYS, ConfigError, json_schema, render_errors, warnings};

#[cfg(test)]
mod tests;
