//! Running a project: start/stop the stack (locally or through hostd for SSH checkouts),
//! per-service control, actions, and the config overview the dashboard renders.

use atlas_core::sys::CommandExt as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use atlas_tui::config;
use atlas_tui::{control, runner};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use super::services::{open_target, service_kind};
use super::setup::setup_info;
use super::{load_for, notify_changed, root_of};
use crate::host::HostHub;

fn host_hub(app: &AppHandle) -> std::sync::Arc<HostHub> {
    app.state::<std::sync::Arc<HostHub>>().inner().clone()
}

/// `$ATLAS_BIN`, then `~/.atlas/bin/atlas`, then `atlas` on the shell's PATH.
pub(crate) fn atlas_bin() -> Result<PathBuf, String> {
    let env = std::env::var_os("ATLAS_BIN").map(PathBuf::from);
    let home = dirs::home_dir().map(|h| h.join(".atlas/bin").join(atlas_core::paths::exe("atlas")));
    [env, home, atlas_core::paths::bundled_cli()]
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
        .or_else(|| crate::shell_env::which("atlas"))
        .ok_or_else(|| {
            if cfg!(windows) {
                "atlas CLI not found — install (PowerShell): irm https://atlas.codeatlas.com.br/install.ps1 | iex".into()
            } else {
                "atlas CLI not found — install: curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash".into()
            }
        })
}

#[tauri::command]
pub async fn stack_start(
    app: AppHandle,
    id: String,
    services: Vec<String>,
    profiles: Vec<String>,
) -> Result<(), String> {
    let e = super::find(&id)?;
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
        let hub = host_hub(&app);
        let r = hub
            .call(
                "exec",
                json!({"host": e.host(), "cwd": e.root, "argv": argv, "timeout_ms": 120_000}),
            )
            .await?;
        notify_changed(&app);
        return if r["code"] == 0 {
            Ok(())
        } else {
            Err("atlas dev --detach failed on the host".into())
        };
    }
    let mut cmd = tokio::process::Command::new(atlas_bin()?);
    cmd.no_window();
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
    notify_changed(&app);
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
    let e = super::find(&id)?;
    if e.is_remote() {
        let hub = host_hub(&app);
        hub.call(
            "exec",
            json!({"host": e.host(), "cwd": e.root, "argv": ["atlas", "down", "--dir", e.root], "timeout_ms": 30_000}),
        )
        .await?;
        notify_changed(&app);
        return Ok(());
    }
    let root = root_of(&id)?;
    let res = runner::down(&root, Duration::from_secs(10)).await?;
    notify_changed(&app);
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
