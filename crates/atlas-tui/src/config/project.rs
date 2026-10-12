use std::collections::{BTreeMap, HashMap};

use schemars::JsonSchema;
use serde::Deserialize;

/// Platform hint for session actions — drives the device picker in the TUI.
#[derive(Debug, Clone, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionPlatform {
    Ios,
    Android,
}

/// How the mobile dev server exposes itself to the device.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum HostMode {
    /// LAN address for a physical device, loopback for a simulator/emulator.
    #[default]
    Auto,
    /// Always advertise the machine's LAN IPv4.
    Lan,
    /// Always advertise `127.0.0.1`.
    Simulator,
}

/// When the `pre_command` (dependency install) runs.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PreCommandWhen {
    /// Every start (default when `pre_command` is set).
    Always,
    /// Only when `node_modules` is missing or a lockfile is newer than it.
    Lockfile,
}

/// Actions in display order for the help overlay and command palette:
/// ungrouped first (config order), then each `group` in order of first
/// appearance, members in config order. An empty group string counts as
/// ungrouped.
pub fn grouped_actions(actions: &[ActionConfig]) -> Vec<(Option<&str>, Vec<&ActionConfig>)> {
    fn group_of(a: &ActionConfig) -> Option<&str> {
        a.group.as_deref().filter(|g| !g.is_empty())
    }
    let mut out: Vec<(Option<&str>, Vec<&ActionConfig>)> = vec![(None, Vec::new())];
    for a in actions {
        match group_of(a) {
            None => out[0].1.push(a),
            Some(g) => match out.iter_mut().find(|(k, _)| *k == Some(g)) {
                Some((_, v)) => v.push(a),
                None => out.push((Some(g), vec![a])),
            },
        }
    }
    out.retain(|(_, v)| !v.is_empty());
    out
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ActionConfig {
    pub key: char,
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub pause_services: Vec<String>,
    #[serde(default)]
    pub open_tunnel: bool,
    /// When true, opens a dedicated log tab with stdout/stderr streamed in real time.
    #[serde(default)]
    pub session: bool,
    /// Key→stdin string mapping for interactive sessions.
    #[serde(default)]
    pub input_map: HashMap<String, String>,
    /// Platform hint — when set, atlas shows a device picker before spawning and
    /// injects mobile config (team ID, bundle ID) from atlas.local.yaml automatically.
    pub platform: Option<SessionPlatform>,
    /// Pin to a specific simulator name or ADB device serial.
    /// When set, skips the interactive device picker.
    /// Example: `device_id: "iPhone 16 Pro"` or `device_id: "emulator-5554"`
    pub device_id: Option<String>,
    /// Host the dev server advertises to the device, exported to the session as
    /// `TAURI_DEV_HOST` and `ATLAS_DEV_HOST` (never overriding a user-set value).
    /// `auto` (default): physical device → LAN IPv4, simulator/emulator → `127.0.0.1`.
    /// `lan`: always the LAN IPv4. `simulator`: always `127.0.0.1`.
    #[serde(default)]
    pub host_mode: HostMode,
    /// Groups actions: help (`?`) and the command palette list ungrouped
    /// actions first, then each group in order of first appearance.
    /// Example: `group: "iOS"` or `group: "Android"`
    pub group: Option<String>,
    /// When set, triggering this action first shows a confirmation modal with
    /// this text (title = action label). Enter/`y` runs it, Esc/`n` cancels.
    /// Example: `confirm: "This wipes the local database. Continue?"`
    pub confirm: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
pub struct MobileConfig {
    pub ios: Option<IosConfig>,
    pub android: Option<AndroidConfig>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
pub struct IosConfig {
    /// Apple Developer Team ID (e.g. A6FCTW4KP6). Set in atlas.local.yaml — never commit.
    pub team_id: Option<String>,
    /// Override bundle identifier. Defaults to the one in tauri.conf.json.
    pub bundle_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
pub struct AndroidConfig {
    /// Path to keystore file for signing. Defaults to ~/.android/debug.keystore.
    pub keystore: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: String,
    pub tunnel: Option<TunnelConfig>,
    pub services: HashMap<String, ServiceDef>,
    pub infra: Option<InfraConfig>,
    #[serde(default)]
    pub pre_command: Option<String>,
    /// `always` (default with an explicit `pre_command`) re-runs it on every
    /// start; `lockfile` only when `node_modules` is missing/stale. The
    /// auto-detected installer (no `pre_command`) always uses `lockfile`.
    #[serde(default)]
    pub pre_command_when: Option<PreCommandWhen>,
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
    /// Mobile-specific config — lives in atlas.local.yaml, never committed.
    #[serde(default)]
    pub mobile: MobileConfig,
    /// Dotenv-style files (paths relative to the project root) loaded in order
    /// (later wins) and injected into every spawned service/action. Missing
    /// files are skipped silently. Precedence for child env, lowest→highest:
    /// inherited env < env_files < per-service `env:` < atlas-injected vars.
    #[serde(default)]
    pub env_files: Vec<String>,
    /// Named service groups for `atlas dev --profile NAME` (repeatable, union):
    /// `profiles: { backend: [server, worker], front: [web] }`. Selecting
    /// services pulls in their transitive `depends_on` too.
    #[serde(default)]
    pub profiles: BTreeMap<String, Vec<String>>,
    /// Release engine for `atlas deploy` (not overridable from atlas.local.yaml).
    #[serde(default)]
    pub deploy: Option<DeployConfig>,
    /// Atlas Desktop worktrees: setup/teardown hooks and files shared with the main checkout.
    #[serde(default)]
    pub worktree: Option<WorktreeConfig>,
    /// Added to every port by `ATLAS_PORT_OFFSET` (parallel worktrees); 0 = none.
    #[serde(skip)]
    #[schemars(skip)]
    pub port_offset: u16,
}

/// `worktree:` — used by Atlas Desktop when it creates/deletes a git worktree.
#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
pub struct WorktreeConfig {
    /// Run (`sh -c`, cwd = new worktree) after creation. Env: ATLAS_ROOT_PATH,
    /// ATLAS_WORKTREE_PATH, ATLAS_WORKSPACE_NAME, ATLAS_PORT_OFFSET.
    pub setup: Option<String>,
    /// Run before the worktree is deleted.
    pub archive: Option<String>,
    /// Gitignored paths (relative) taken from the main checkout: directories are
    /// symlinked, files copied (e.g. `.env`, `node_modules`).
    #[serde(default)]
    pub shared: Vec<String>,
    /// Seconds before `setup` is considered hung (default 600).
    #[serde(default = "default_setup_timeout")]
    pub setup_timeout: u64,
}

fn default_setup_timeout() -> u64 {
    600
}

/// `deploy:` — build locally, ship an artifact, swap atomically on the host.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct DeployConfig {
    /// App name ([a-z0-9-]+). Defaults to the project `name`.
    pub app: Option<String>,
    /// Local build command (`sh -c`, cwd = project root).
    pub build: Option<String>,
    /// Root-relative paths to ship. Omitted: `git archive HEAD`.
    pub artifact: Option<Vec<String>>,
    pub targets: HashMap<String, DeployTarget>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct DeployTarget {
    /// ssh host/alias, or `local`.
    pub host: String,
    /// Absolute deploy dir. Default `/opt/<app>`.
    pub dir: Option<String>,
    /// Releases kept on the host (>= 2). Default 5.
    pub keep: Option<u32>,
    /// Release-relative path -> absolute shared path (symlinked into each release).
    pub link: Option<HashMap<String, String>>,
    /// Run in the new release before the swap.
    pub install: Option<String>,
    /// Run in the new release before the swap (expand-only migrations).
    pub migrate: Option<String>,
    pub hooks: Option<DeployHooks>,
    /// Absolute path touched every 30s while the deploy runs, removed at the end.
    pub pause_marker: Option<String>,
    /// Seconds per service to become healthy. Default 60.
    pub health_timeout: Option<u64>,
    /// Consecutive passing probes required. Default 3.
    pub health_passes: Option<u32>,
    /// Seconds a service must stay up without restarting. Default 15.
    pub soak: Option<u64>,
    /// Minimum free disk on the host, MB. Default 1024.
    pub min_free_mb: Option<u64>,
    /// Restarted strictly in order.
    pub services: Vec<DeployService>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
pub struct DeployHooks {
    pub pre_swap: Option<String>,
    pub post_swap: Option<String>,
    pub post_deploy: Option<String>,
    pub on_failure: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DeployKind {
    Systemd,
    Compose,
    Command,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct DeployService {
    pub name: String,
    pub kind: DeployKind,
    /// http(s) URL probed with curl on the host.
    pub health: Option<String>,
    /// Compose only; release-relative. Default `compose.yml`.
    pub compose_file: Option<String>,
    /// Command only (required).
    pub restart: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
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

#[derive(Debug, Clone, Deserialize, JsonSchema, Default)]
#[schemars(deny_unknown_fields)]
pub struct ServiceDef {
    pub command: Option<String>,
    /// Working directory for this service, relative to project root.
    /// Defaults to project root when absent.
    pub dir: Option<String>,
    pub port: Option<u16>,
    pub health: Option<String>,
    pub critical: Option<bool>,
    /// Services this one waits for. List form (`[db, redis]`) = wait until each
    /// is up (port/health Running, or simply spawned when it has no probe).
    /// Map form picks a condition per dependency:
    /// `{ db: healthy, migrate: completed_successfully, api: log_ready }`.
    pub depends_on: Option<DependsOn>,
    pub enabled: Option<bool>,
    /// Single key to open this service's URL in the browser (e.g. `open_key: w`).
    pub open_key: Option<char>,
    /// Extra environment variables injected only for this service.
    /// Use instead of hardcoding variable names in the TUI.
    /// Example: `env: { PDF_SERVICE_PORT: "3873" }`
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// When true, no dedicated log tab is created for this service.
    /// Logs still appear in the All tab. Use for infra/docker services
    /// that should start silently as dependencies.
    #[serde(default)]
    pub hide_logs: bool,
    /// JSON lock file (path relative to project root) holding `{"pid": N}`.
    /// Before the service starts, a live process from a previous run that
    /// still owns the lock (and lives inside the project root) is terminated
    /// and the file removed. Example: `lock_file: apps/ai/.mastra/dev.lock`
    pub lock_file: Option<String>,
    /// URL opened by `open_key` instead of `http://localhost:<port>`.
    /// Example: `open_url: http://localhost:3001/api-reference`
    pub open_url: Option<String>,
    /// Memory ceiling for the whole process tree (`512M`, `3G`). Over it for
    /// two samples in a row (~10s), the service is restarted with an alert (3 times
    /// per run, then stopped and marked failed): a
    /// guard against dev servers that leak. Example: `max_memory: 4G`
    pub max_memory: Option<String>,
    /// Hint shown under this service's critical-failure alert in the sidebar.
    /// Only relevant with `critical: true`.
    /// Example: `critical_help: "Run `bun run db:up` and retry"`
    pub critical_help: Option<String>,
    /// Restart policy when atlas-spawned child exits on its own:
    /// `no` (default), `on_failure` (non-zero exit / crash), `always`.
    #[serde(default)]
    pub restart: RestartPolicy,
    /// Max consecutive restarts before giving up (Failed). Default 5; `0` = unlimited.
    /// The counter resets after the service stays Running for 60s.
    pub max_restarts: Option<u32>,
    /// Regex matched against each stdout/stderr line. First match marks the
    /// service ready (for `log_ready` dependents, and as its own readiness
    /// signal instead of the port/health probe).
    pub ready_log: Option<String>,
    /// One-shot task (migration, seed): exiting 0 is success, not a failure.
    /// Dependents can wait with `completed_successfully`. Implied when a
    /// dependent uses that condition.
    #[serde(default)]
    pub oneshot: bool,
    /// Seconds a service may stay `starting` (port/health/`ready_log` not yet
    /// satisfied) before atlas warns; with `critical: true` the critical alert
    /// fires too. Default 60. The service is not killed.
    pub start_timeout: Option<u64>,
    /// `false` = not started at boot; start it from the service menu, command
    /// palette or MCP (`atlas_start_service`). Default `true`.
    pub autostart: Option<bool>,
}

/// Default `start_timeout` in seconds.
pub const DEFAULT_START_TIMEOUT_SECS: u64 = 60;

impl ServiceDef {
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    /// Started at boot (default); `autostart: false` waits for a manual start.
    pub fn is_autostart(&self) -> bool {
        self.autostart.unwrap_or(true)
    }

    /// Seconds a service may stay `starting` before the start-timeout warning.
    pub fn start_timeout_secs(&self) -> u64 {
        self.start_timeout.unwrap_or(DEFAULT_START_TIMEOUT_SECS)
    }

    /// True when atlas has nothing to probe: no port, health URL or `ready_log`.
    pub fn has_no_probe(&self) -> bool {
        self.port.is_none()
            && self.health.as_deref().is_none_or(str::is_empty)
            && self.ready_log.is_none()
    }

    /// `(dependency, condition)` pairs in declaration order (map form sorted by name).
    pub fn deps(&self) -> Vec<(String, Condition)> {
        self.depends_on
            .as_ref()
            .map(DependsOn::pairs)
            .unwrap_or_default()
    }
}

/// What a dependent waits for before it is spawned.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// Dependency process has been spawned.
    Started,
    /// Dependency is Running (port/health probe, or `ready_log`). Default for list form.
    #[default]
    Healthy,
    /// Dependency exited with status 0.
    CompletedSuccessfully,
    /// A line matching the dependency's `ready_log` appeared.
    LogReady,
}

impl Condition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Healthy => "healthy",
            Self::CompletedSuccessfully => "completed_successfully",
            Self::LogReady => "log_ready",
        }
    }
}

/// `depends_on:` — list form or `{ service: condition }` map form.
#[derive(Debug, Clone, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(untagged)]
pub enum DependsOn {
    List(Vec<String>),
    Map(BTreeMap<String, Condition>),
}

impl DependsOn {
    pub fn pairs(&self) -> Vec<(String, Condition)> {
        match self {
            Self::List(v) => v.iter().map(|n| (n.clone(), Condition::Healthy)).collect(),
            Self::Map(m) => m.iter().map(|(n, c)| (n.clone(), *c)).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicy {
    /// Never restart (default).
    #[default]
    No,
    /// Restart when the process exits non-zero or is killed.
    OnFailure,
    /// Restart on any exit.
    Always,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct InfraConfig {
    pub compose_file: String,
}
