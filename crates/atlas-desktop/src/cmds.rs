//! `#[tauri::command]`s. Every error is a human-readable string (shown in a toast).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use atlas_tui::config::{self, ConfigError};
use atlas_tui::runtime::{self, deps};
use atlas_tui::{control, runner, status};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::live::LiveHub;
use crate::registry::{self, Entry};

#[derive(Serialize)]
pub struct ProjectView {
    id: String,
    root: String,
    name: String,
    pinned: bool,
    running: bool,
    mode: Option<String>,
    services_total: usize,
    services_healthy: usize,
    error: Option<String>,
    profiles: Vec<String>,
    parent: Option<String>,
    port_offset: u16,
    branch: Option<String>,
    /// `ssh:<alias>` for SSH checkouts.
    host: Option<String>,
    /// User-chosen icon and color (`None` = automatic).
    identity: Option<registry::Identity>,
}

#[derive(Serialize)]
pub struct Problem {
    path: String,
    message: String,
}

#[derive(Serialize)]
pub struct ConfigText {
    text: String,
    path: String,
}

fn view(e: &Entry) -> ProjectView {
    let root = Path::new(&e.root);
    let fallback = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| e.root.clone());
    if e.is_remote() {
        // Never read a remote path on this machine; details come via hostd.
        return ProjectView {
            id: e.id.clone(),
            root: e.root.clone(),
            name: fallback,
            pinned: e.pinned,
            running: false,
            mode: None,
            services_total: 0,
            services_healthy: 0,
            error: None,
            profiles: Vec::new(),
            parent: e.parent.clone(),
            port_offset: e.port_offset,
            branch: e.branch.clone(),
            host: e.host.clone(),
            identity: e.identity.clone(),
        };
    }
    let (name, error, total, profiles) = match config::load_checked(root) {
        Ok((cfg, errs)) => (
            cfg.name.clone(),
            (!errs.is_empty()).then(|| config::render_errors(&errs, "atlas.yaml")),
            cfg.services.len(),
            cfg.profiles.keys().cloned().collect(),
        ),
        Err(err) => (fallback, Some(format!("{err:#}")), 0, Vec::new()),
    };
    let snap = status::live_snapshot(root).ok();
    ProjectView {
        id: e.id.clone(),
        root: e.root.clone(),
        name,
        pinned: e.pinned,
        running: snap.is_some(),
        mode: snap.as_ref().map(|s| s.mode.clone()),
        services_total: snap.as_ref().map_or(total, |s| s.services.len()),
        services_healthy: snap.as_ref().map_or(0, |s| {
            s.services.iter().filter(|r| r.state == "running").count()
        }),
        error,
        profiles,
        parent: e.parent.clone(),
        port_offset: e.port_offset,
        branch: e.branch.clone(),
        host: None,
        identity: e.identity.clone(),
    }
}

fn find(id: &str) -> Result<Entry, String> {
    registry::load()?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown project {id}"))
}

/// Root of a checkout on this machine. SSH checkouts are refused here: commands
/// that read local files or talk to a local runner socket don't apply to them.
fn root_of(id: &str) -> Result<PathBuf, String> {
    let e = find(id)?;
    if e.is_remote() {
        return Err(REMOTE_UNSUPPORTED.into());
    }
    Ok(PathBuf::from(e.root))
}

const REMOTE_UNSUPPORTED: &str =
    "not available for SSH checkouts yet (use a terminal in the worktree)";

/// atlas.yaml of an entry, ports shifted by its worktree offset.
fn load_for(id: &str) -> Result<(PathBuf, config::ProjectConfig), String> {
    let e = find(id)?;
    if e.is_remote() {
        return Err(REMOTE_UNSUPPORTED.into());
    }
    let root = PathBuf::from(&e.root);
    let mut cfg = config::load(&root).map_err(|e| format!("{e:#}"))?;
    if e.port_offset > 0 {
        config::apply_port_offset(&mut cfg, e.port_offset);
    }
    Ok((root, cfg))
}

#[tauri::command]
pub fn projects_list(app: AppHandle, hub: State<'_, LiveHub>) -> Result<Vec<ProjectView>, String> {
    let entries = registry::load()?;
    for e in entries.iter().filter(|e| !e.is_remote()) {
        hub.ensure(&app, &e.id, Path::new(&e.root));
    }
    Ok(entries.iter().map(view).collect())
}

/// Restart a project's subscription so the frontend gets a fresh `snapshot`
/// (after a webview reload, the long-lived task would otherwise only send deltas).
#[tauri::command]
pub fn live_resync(app: AppHandle, hub: State<'_, LiveHub>, id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    hub.remove(&id);
    hub.ensure(&app, &id, &root);
    Ok(())
}

#[tauri::command]
pub fn projects_add(
    app: AppHandle,
    hub: State<'_, LiveHub>,
    root: String,
) -> Result<ProjectView, String> {
    let root = config::find_root(Path::new(&root)).ok_or(format!("no atlas.yaml in {root}"))?;
    let root = root.canonicalize().unwrap_or(root);
    let mut entries = registry::load()?;
    let id = registry::id_for(&root);
    if let Some(e) = entries.iter().find(|e| e.id == id) {
        return Ok(view(e));
    }
    let entry = Entry {
        id: id.clone(),
        root: root.to_string_lossy().into_owned(),
        pinned: false,
        added: chrono::Utc::now().to_rfc3339(),
        parent: None,
        port_offset: 0,
        branch: None,
        host: None,
        identity: None,
    };
    entries.push(entry.clone());
    registry::save(&entries)?;
    hub.ensure(&app, &id, &root);
    let _ = app.emit("projects://changed", ());
    Ok(view(&entry))
}

#[tauri::command]
pub fn projects_remove(app: AppHandle, hub: State<'_, LiveHub>, id: String) -> Result<(), String> {
    let mut entries = registry::load()?;
    entries.retain(|e| e.id != id);
    registry::save(&entries)?;
    hub.remove(&id);
    let _ = app.emit("projects://changed", ());
    Ok(())
}

#[tauri::command]
pub fn projects_pin(app: AppHandle, id: String, pinned: bool) -> Result<(), String> {
    let mut entries = registry::load()?;
    let e = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(format!("unknown project {id}"))?;
    e.pinned = pinned;
    registry::save(&entries)?;
    let _ = app.emit("projects://changed", ());
    Ok(())
}

/// Set (or, with `None`/an empty identity, clear) the icon and color of a project.
#[tauri::command]
pub fn projects_set_identity(
    app: AppHandle,
    id: String,
    identity: Option<registry::Identity>,
) -> Result<(), String> {
    let identity = identity
        .map(registry::Identity::validated)
        .transpose()?
        .flatten();
    let mut entries = registry::load()?;
    let e = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(format!("unknown project {id}"))?;
    e.identity = identity;
    registry::save(&entries)?;
    let _ = app.emit("projects://changed", ());
    Ok(())
}

#[tauri::command]
pub async fn projects_discover() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = dirs::home_dir().ok_or("cannot locate home directory")?;
        let known: Vec<String> = registry::load()?.into_iter().map(|e| e.root).collect();
        let mut found = Vec::new();
        for dir in ["Projetos", "Projects", "code", "dev"] {
            scan(&home.join(dir), 0, &mut found);
        }
        found.retain(|p| !known.contains(p));
        found.sort();
        found.dedup();
        Ok(found)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn scan(dir: &Path, depth: u32, out: &mut Vec<String>) {
    if dir.join("atlas.yaml").is_file() {
        out.push(dir.to_string_lossy().into_owned());
    }
    if depth >= 3 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        if ent.file_type().is_ok_and(|t| t.is_dir()) {
            scan(&ent.path(), depth + 1, out);
        }
    }
}

/// `$ATLAS_BIN`, then `~/.atlas/bin/atlas`, then `atlas` on the shell's PATH.
pub(crate) fn atlas_bin() -> Result<PathBuf, String> {
    let env = std::env::var_os("ATLAS_BIN").map(PathBuf::from);
    let home = dirs::home_dir().map(|h| h.join(".atlas/bin/atlas"));
    [env, home]
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
        .or_else(|| crate::shell_env::which("atlas"))
        .ok_or_else(|| {
            "atlas CLI not found — install: curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash".into()
        })
}

#[tauri::command]
pub async fn stack_start(
    app: AppHandle,
    id: String,
    services: Vec<String>,
    profiles: Vec<String>,
) -> Result<(), String> {
    let e = find(&id)?;
    let root = PathBuf::from(&e.root);
    if e.is_remote() {
        let mut argv = vec![
            "env".to_string(),
            format!("{}={}", config::PORT_OFFSET_ENV, e.port_offset),
        ];
        argv.extend(["atlas", "dev", "--detach", "--dir", &e.root].map(String::from));
        argv.extend(services);
        for p in profiles {
            argv.extend(["--profile".into(), p]);
        }
        let hub = app
            .state::<std::sync::Arc<crate::host::HostHub>>()
            .inner()
            .clone();
        let r = hub
            .call(
                "exec",
                json!({"host": e.host(), "cwd": e.root, "argv": argv, "timeout_ms": 120_000}),
            )
            .await?;
        let _ = app.emit("projects://changed", ());
        return if r["code"] == 0 {
            Ok(())
        } else {
            Err("atlas dev --detach failed on the host".into())
        };
    }
    let mut cmd = tokio::process::Command::new(atlas_bin()?);
    cmd.envs(crate::shell_env::vars_async().await);
    if e.port_offset > 0 {
        cmd.env(config::PORT_OFFSET_ENV, e.port_offset.to_string());
    }
    cmd.args(["dev", "--detach", "--dir"])
        .arg(&root)
        .args(&services);
    for p in &profiles {
        cmd.args(["--profile", p]);
    }
    let out = cmd
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("cannot run atlas: {e}"))?;
    let _ = app.emit("projects://changed", ());
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() {
            format!("atlas dev --detach failed ({})", out.status)
        } else {
            err
        })
    }
}

#[tauri::command]
pub async fn stack_stop(app: AppHandle, id: String) -> Result<(), String> {
    let e = find(&id)?;
    if e.is_remote() {
        let hub = app
            .state::<std::sync::Arc<crate::host::HostHub>>()
            .inner()
            .clone();
        hub.call(
            "exec",
            json!({"host": e.host(), "cwd": e.root, "argv": ["atlas", "down", "--dir", e.root], "timeout_ms": 30_000}),
        )
        .await?;
        let _ = app.emit("projects://changed", ());
        return Ok(());
    }
    let root = root_of(&id)?;
    let res = runner::down(&root, Duration::from_secs(10)).await?;
    let _ = app.emit("projects://changed", ());
    match res {
        runner::DownOutcome::Timeout { pid } => Err(format!("atlas (pid {pid}) is still stopping")),
        _ => Ok(()),
    }
}

async fn call(root: &Path, cmd: &str, args: Value) -> Result<Value, String> {
    let resp = control::call(root, cmd, args).await?;
    if resp.ok {
        Ok(resp.data.unwrap_or(Value::Null))
    } else {
        Err(resp.error.unwrap_or_else(|| format!("{cmd} failed")))
    }
}

#[tauri::command]
pub async fn service_cmd(id: String, cmd: String, service: String) -> Result<Value, String> {
    if !matches!(
        cmd.as_str(),
        "start_service" | "stop_service" | "restart_service"
    ) {
        return Err(format!("unknown service command {cmd}"));
    }
    call(&root_of(&id)?, &cmd, json!({ "service": service })).await
}

/// `{ needs_confirm: text }` when the action has a `confirm:` gate and `confirmed` is false.
#[tauri::command]
pub async fn action_run(id: String, target: String, confirmed: bool) -> Result<Value, String> {
    let root = root_of(&id)?;
    if !confirmed
        && let Ok(cfg) = config::load(&root)
        && let Some(a) = cfg
            .actions
            .iter()
            .find(|a| a.label == target || a.key.to_string() == target)
        && let Some(text) = a.confirm.as_deref().filter(|c| !c.is_empty())
    {
        return Ok(json!({ "needs_confirm": text, "label": a.label }));
    }
    call(
        &root,
        "run_action",
        json!({ "action": target, "confirmed": confirmed }),
    )
    .await
}

/// Services (with ports) and actions from the config, for the overview.
#[tauri::command]
pub fn project_config(id: String) -> Result<Value, String> {
    let (root, cfg) = load_for(&id)?;
    let mut services: Vec<Value> = cfg
        .services
        .iter()
        .map(|(n, s)| {
            let kind = service_kind(n, s);
            json!({
                "name": n, "port": s.port, "enabled": s.is_enabled(),
                "kind": kind, "open_key": s.open_key.map(|k| k.to_string()),
                "target": open_target(kind, s),
                "command": s.command, "dir": s.dir,
                "depends_on": s.deps().into_iter().map(|(d, _)| d).collect::<Vec<_>>(),
            })
        })
        .collect();
    services.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let actions: Vec<Value> = cfg
        .actions
        .iter()
        .map(|a| {
            json!({
                "key": a.key.to_string(), "label": a.label, "group": a.group,
                "confirm": a.confirm, "session": a.session || a.platform.is_some(),
            })
        })
        .collect();
    Ok(
        json!({ "name": cfg.name, "services": services, "actions": actions, "setup": setup_info(&root, &cfg) }),
    )
}

/// The dependency-install step: effective command (auto-detected when no
/// `pre_command`), schedule and whether it is auto-detected. `command` is null
/// when there is nothing to run (`pre_command: ""`, or no `package.json`).
fn setup_info(root: &Path, cfg: &config::ProjectConfig) -> Value {
    let pre = cfg.pre_command.as_deref();
    let command = match pre {
        Some("") => None,
        Some(c) => Some(c.to_string()),
        None => deps::detect_install_command(root).map(str::to_string),
    };
    let when = match deps::effective_when(pre, cfg.pre_command_when) {
        config::PreCommandWhen::Always => "always",
        config::PreCommandWhen::Lockfile => "lockfile",
    };
    json!({ "command": command, "when": when, "auto": pre.is_none() })
}

/// Projects with a `setup_run` in flight (one install per project at a time).
static SETUP_RUNNING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Releases the project's slot in `SETUP_RUNNING` on every exit path.
struct SetupSlot(String);

impl SetupSlot {
    fn take(id: &str) -> Option<Self> {
        SETUP_RUNNING
            .lock()
            .insert(id.to_string())
            .then(|| Self(id.to_string()))
    }
}

impl Drop for SetupSlot {
    fn drop(&mut self) {
        SETUP_RUNNING.lock().remove(&self.0);
    }
}

/// Output lines `setup_status` returns.
const SETUP_OUTPUT_LINES: usize = 200;

/// `{ status: <.logs/setup.json or null>, output: <last 200 lines of setup.log> }`.
#[tauri::command]
pub fn setup_status(id: String) -> Result<Value, String> {
    let dir = runtime::log_dir(&root_of(&id)?);
    let status = std::fs::read_to_string(dir.join(deps::SETUP_STATUS))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let log = std::fs::read_to_string(dir.join(deps::SETUP_LOG)).unwrap_or_default();
    let lines: Vec<&str> = log.lines().collect();
    let output = lines[lines.len().saturating_sub(SETUP_OUTPUT_LINES)..].join("\n");
    Ok(json!({ "status": status, "output": output }))
}

/// "Run now": the dependency install, skipping the `when` gate. Returns
/// `{ ok, reason }` and emits `projects://setup` with the id when it finishes.
#[tauri::command]
pub async fn setup_run(app: AppHandle, id: String) -> Result<Value, String> {
    let (root, cfg) = load_for(&id)?;
    let _slot = SetupSlot::take(&id).ok_or("setup is already running")?;
    // Seen by setup_status/`projects://setup` listeners before the process ends.
    let outcome = deps::ensure_deps(
        &root,
        cfg.pre_command.as_deref(),
        cfg.pre_command_when,
        true,
    )
    .await;
    let _ = app.emit("projects://setup", &id);
    Ok(match outcome {
        deps::DepsOutcome::Failed(reason) => json!({ "ok": false, "reason": reason }),
        deps::DepsOutcome::Installed => json!({ "ok": true }),
        deps::DepsOutcome::UpToDate => json!({ "ok": true, "skipped": true }),
    })
}

/// What a service is, from its config alone: drives the card icon and what "open" does.
/// `web` / `api` open in the browser; `database` / `cache` copy a connection URL;
/// `worker` / `task` have nothing to open.
fn service_kind(name: &str, s: &config::ServiceDef) -> &'static str {
    let n = name.to_ascii_lowercase();
    let cmd = s
        .command
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w) || cmd.contains(w));
    let port = s.port.unwrap_or(0);
    if has(&["redis", "valkey", "memcache", "dragonfly"])
        || (6379..=6399).contains(&port)
        || port == 63790
    {
        return "cache";
    }
    if has(&[
        "postgres", "database", "mysql", "mariadb", "mongo", "supabase",
    ]) || n == "db"
        || matches!(port, 5432 | 3306 | 27017)
        || (54320..=54329).contains(&port)
    {
        return "database";
    }
    if s.port.is_none() && s.open_url.is_none() {
        return if s.oneshot { "task" } else { "worker" };
    }
    let url = s.open_url.as_deref().unwrap_or_default();
    if has(&["api", "server", "backend", "graphql", "grpc"])
        || url.contains("api")
        || url.contains("docs")
    {
        "api"
    } else {
        "web"
    }
}

/// `{ how: "browser" | "copy", url }`, or null when there is nothing to open.
fn open_target(kind: &str, s: &config::ServiceDef) -> Value {
    if let Some(url) = s.open_url.as_deref().filter(|u| !u.is_empty()) {
        return json!({ "how": "browser", "url": url });
    }
    let Some(port) = s.port else {
        return Value::Null;
    };
    match kind {
        "web" | "api" => json!({ "how": "browser", "url": format!("http://localhost:{port}") }),
        "cache" => json!({ "how": "copy", "url": format!("redis://localhost:{port}") }),
        "database" => {
            let scheme = match port {
                3306 => "mysql",
                27017 => "mongodb",
                _ => "postgresql",
            };
            json!({ "how": "copy", "url": format!("{scheme}://localhost:{port}") })
        }
        _ => Value::Null,
    }
}

#[tauri::command]
pub fn config_read(id: String) -> Result<ConfigText, String> {
    let path = root_of(&id)?.join("atlas.yaml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(ConfigText {
        text,
        path: path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn config_schema() -> String {
    config::json_schema()
}

fn validate(root: &Path, text: &str) -> Result<Vec<Problem>, String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    std::fs::write(dir.path().join("atlas.yaml"), text).map_err(|e| e.to_string())?;
    let local = root.join("atlas.local.yaml");
    if local.is_file() {
        std::fs::copy(&local, dir.path().join("atlas.local.yaml")).map_err(|e| e.to_string())?;
    }
    let res = config::load_checked(dir.path());
    Ok(match res {
        Ok((_, errs)) => errs.iter().map(problem).collect(),
        Err(e) => vec![Problem {
            path: String::new(),
            message: format!("{e:#}"),
        }],
    })
}

fn problem(e: &ConfigError) -> Problem {
    Problem {
        path: e.path.clone(),
        message: e.message.clone(),
    }
}

#[tauri::command]
pub fn config_validate(id: String, text: String) -> Result<Vec<Problem>, String> {
    validate(&root_of(&id)?, &text)
}

#[tauri::command]
pub fn config_write(id: String, text: String) -> Result<(), String> {
    write_checked(&root_of(&id)?, &text)
}

/// Validate, then tmp+rename; an invalid text never reaches disk.
fn write_checked(root: &Path, text: &str) -> Result<(), String> {
    let problems = validate(root, text)?;
    if let Some(p) = problems.first() {
        return Err(format!(
            "not saved: {} problem(s), first: {} {}",
            problems.len(),
            p.path,
            p.message
        ));
    }
    registry::write_atomic(&root.join("atlas.yaml"), text)
}

fn open(args: &[&str]) -> Result<(), String> {
    let st = std::process::Command::new("open")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("open {} failed", args.join(" ")))
    }
}

#[tauri::command]
pub fn open_in_editor(id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    let r = root.to_string_lossy();
    open(&["-a", "Visual Studio Code", &r]).or_else(|_| open(&[&r]))
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("refusing to open {url}"));
    }
    open(&[&url])
}

#[tauri::command]
pub fn reveal(id: String) -> Result<(), String> {
    let root = root_of(&id)?;
    open(&["-R", &root.join("atlas.yaml").to_string_lossy()])
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = "name: demo\nservices:\n  web: { command: \"true\", port: 4711 }\n";

    #[test]
    fn validate_reports_semantic_errors_with_paths() {
        let root = tempfile::tempdir().unwrap();
        let bad = format!("{OK}  api: {{ command: \"true\", depends_on: [nope] }}\n");
        let probs = validate(root.path(), &bad).unwrap();
        assert_eq!(probs.len(), 1);
        assert_eq!(probs[0].path, "services.api.depends_on.nope");
        assert!(probs[0].message.contains("unknown service 'nope'"));
        assert!(validate(root.path(), OK).unwrap().is_empty());
    }

    #[test]
    fn validate_merges_the_projects_local_override() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("atlas.local.yaml"),
            "services:\n  web: { helth: x }\n",
        )
        .unwrap();
        let probs = validate(root.path(), OK).unwrap();
        assert!(
            probs.iter().any(|p| p.path.contains("helth")),
            "{:?}",
            probs.iter().map(|p| &p.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn validate_turns_parse_failures_into_one_problem() {
        let root = tempfile::tempdir().unwrap();
        let probs = validate(root.path(), "services: [").unwrap();
        assert_eq!(probs.len(), 1);
        assert!(probs[0].path.is_empty());
    }

    #[test]
    fn write_refuses_invalid_and_saves_valid() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("atlas.yaml");
        std::fs::write(&file, OK).unwrap();
        let bad = format!("{OK}  api: {{ command: \"true\", depends_on: [nope] }}\n");
        let err = write_checked(root.path(), &bad).unwrap_err();
        assert!(err.contains("unknown service 'nope'"), "{err}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), OK);
        let fixed = format!("{OK}  api: {{ command: \"true\", depends_on: [web] }}\n");
        write_checked(root.path(), &fixed).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), fixed);
    }

    fn svc(yaml: &str) -> config::ServiceDef {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn kinds_and_targets_follow_config() {
        let cases = [
            (
                "database",
                "port: 54321",
                "database",
                "postgresql://localhost:54321",
            ),
            ("redis", "port: 63790", "cache", "redis://localhost:63790"),
            (
                "server",
                "{ command: x, port: 3070, open_url: 'http://localhost:3070/api-reference' }",
                "api",
                "http://localhost:3070/api-reference",
            ),
            (
                "web",
                "{ command: x, port: 3071 }",
                "web",
                "http://localhost:3071",
            ),
        ];
        for (name, yaml, kind, url) in cases {
            let s = svc(yaml);
            let k = service_kind(name, &s);
            assert_eq!(k, kind, "{name}");
            assert_eq!(open_target(k, &s)["url"], url, "{name}");
        }
        let w = svc("{ command: x }");
        assert_eq!(service_kind("worker", &w), "worker");
        assert!(open_target("worker", &w).is_null());
        assert_eq!(
            service_kind("migrate", &svc("{ command: x, oneshot: true }")),
            "task"
        );
    }
}

#[cfg(test)]
mod setup_tests {
    use super::*;

    fn cfg(yaml: &str) -> config::ProjectConfig {
        serde_yaml::from_str(&format!("name: t\nservices: {{}}\n{yaml}")).unwrap()
    }

    #[test]
    fn setup_info_reports_effective_command_and_schedule() {
        let dir = tempfile::tempdir().unwrap();
        // No pre_command, no package.json: nothing to run.
        let none = setup_info(dir.path(), &cfg(""));
        assert!(none["command"].is_null());
        assert_eq!(none["when"], "lockfile");
        assert_eq!(none["auto"], true);

        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("bun.lock"), "").unwrap();
        let auto = setup_info(dir.path(), &cfg(""));
        assert_eq!(auto["command"], "bun install");
        assert_eq!(auto["when"], "lockfile");

        let explicit = setup_info(dir.path(), &cfg("pre_command: bun install\n"));
        assert_eq!(explicit["command"], "bun install");
        assert_eq!(explicit["when"], "always");
        assert_eq!(explicit["auto"], false);

        let gated = setup_info(
            dir.path(),
            &cfg("pre_command: make deps\npre_command_when: lockfile\n"),
        );
        assert_eq!(gated["when"], "lockfile");

        let off = setup_info(dir.path(), &cfg("pre_command: \"\"\n"));
        assert!(off["command"].is_null());
    }
}
