//! SSH hosts: a remote `atlas hostd` reached through an `ssh` tunnel.
//!
//! Per host alias (from `~/.ssh/config`): check `~/.atlas/bin/atlas` (upload
//! `~/.atlas/remote/atlas-<arch>-linux` when missing), start `atlas hostd`, then
//! keep `ssh -N -L <local sock>:<remote sock> -R <port>:127.0.0.1:<hook port>`
//! running and connect a `HostClient` through it. The reverse forward lets the
//! omp status extension on the host reach this app's hook server; the host's
//! `~/.atlas/desktop/hook-endpoint.env` is written to point at it.
//! Reconnects with backoff while the host stays in `settings.sshHosts`.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use atlas_host::HostClient;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::{Child, Command};

use crate::host::HostHub;

const SSH_OPTS: [&str; 6] = [
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=10",
    "-o",
    "ServerAliveInterval=15",
];

#[derive(Default)]
pub struct Remotes {
    tunnels: Mutex<std::collections::HashMap<String, Child>>,
    status: Mutex<std::collections::HashMap<String, String>>,
    forwards: Mutex<Vec<(String, u16, u16, Child)>>,
}

pub fn host_id(alias: &str) -> String {
    format!("ssh:{alias}")
}

fn valid_alias(a: &str) -> Result<(), String> {
    if !a.is_empty()
        && a.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        Ok(())
    } else {
        Err(format!("invalid ssh host alias {a:?}"))
    }
}

async fn ssh(alias: &str, script: &str, timeout: Duration) -> Result<String, String> {
    let fut = Command::new("ssh")
        .args(SSH_OPTS)
        .arg(alias)
        .arg(script)
        .stdin(Stdio::null())
        .output();
    let out = tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| format!("ssh {alias}: timed out"))?
        .map_err(|e| format!("ssh: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "ssh {alias}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn local_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".atlas/remote")
}

/// `Host` aliases from `~/.ssh/config` (no wildcards).
#[tauri::command]
pub fn ssh_config_hosts() -> Vec<String> {
    let p = dirs::home_dir().unwrap_or_default().join(".ssh/config");
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().strip_prefix("Host "))
        .flat_map(|h| h.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .filter(|h| !h.contains(['*', '?', '!']))
        .collect()
}

/// Make sure atlas is on the host (upload the matching Linux build when
/// missing) and hostd runs. Returns `(home, token)`.
async fn prepare(alias: &str) -> Result<(String, String), String> {
    let probe = ssh(
        alias,
        r#"printf '%s\n' "$HOME" "$(uname -m)" "$(uname -s)"; test -x "$HOME/.atlas/bin/atlas" && echo yes || echo no"#,
        Duration::from_secs(20),
    )
    .await?;
    let mut l = probe.lines();
    let (home, arch, os, has) = (
        l.next().unwrap_or("").to_string(),
        l.next().unwrap_or(""),
        l.next().unwrap_or("").to_lowercase(),
        l.next() == Some("yes"),
    );
    if home.is_empty() {
        return Err(format!("{alias}: could not read $HOME"));
    }
    if !has {
        let bin = local_dir().join(format!("atlas-{arch}-{os}"));
        if !bin.is_file() {
            return Err(format!(
                "atlas is not installed on {alias} ({arch} {os}); put a build at {} or install it there",
                bin.display()
            ));
        }
        ssh(alias, "mkdir -p ~/.atlas/bin", Duration::from_secs(15)).await?;
        let st = Command::new("scp")
            .args(["-q", "-o", "BatchMode=yes"])
            .arg(&bin)
            .arg(format!("{alias}:.atlas/bin/atlas"))
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if !st.success() {
            return Err(format!("upload to {alias} failed"));
        }
        ssh(
            alias,
            "chmod +x ~/.atlas/bin/atlas",
            Duration::from_secs(15),
        )
        .await?;
    }
    // Detached from the ssh session so it survives the tunnel dropping.
    ssh(
        alias,
        r#"A="$HOME/.atlas/bin/atlas"; "$A" hostd --status >/dev/null 2>&1 || { setsid "$A" hostd >>"$HOME/.atlas/hostd.log" 2>&1 </dev/null & sleep 1; }; "$A" hostd --status >/dev/null && cat "$HOME/.atlas/hostd.token""#,
        Duration::from_secs(30),
    )
    .await
    .map(|t| (home, t.trim().to_string()))
}

async fn connect_once(app: &AppHandle, alias: &str) -> Result<HostClient, String> {
    let (home, token) = prepare(alias).await?;
    let hook_port = *app
        .state::<Arc<crate::agent::hooks::AgentHub>>()
        .port
        .lock();
    let hook_token = app
        .state::<Arc<crate::agent::hooks::AgentHub>>()
        .token
        .lock()
        .clone();
    // The status extension on the host reads this file; same port through -R.
    ssh(
        alias,
        &format!(
            "mkdir -p ~/.atlas/desktop && umask 077 && printf 'ATLAS_HOOK_PORT={hook_port}\\nATLAS_HOOK_TOKEN={hook_token}\\n' > ~/.atlas/desktop/hook-endpoint.env"
        ),
        Duration::from_secs(15),
    )
    .await?;
    let dir = local_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let sock = dir.join(format!("{alias}.sock"));
    let _ = std::fs::remove_file(&sock);
    let child = Command::new("ssh")
        .args(SSH_OPTS)
        .args([
            "-N",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "StreamLocalBindUnlink=yes",
        ])
        .arg("-L")
        .arg(format!("{}:{home}/.atlas/hostd.sock", sock.display()))
        .arg("-R")
        .arg(format!("127.0.0.1:{hook_port}:127.0.0.1:{hook_port}"))
        .arg(alias)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("ssh tunnel: {e}"))?;
    app.state::<Arc<Remotes>>()
        .tunnels
        .lock()
        .insert(alias.to_string(), child);
    let mut last = String::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        match HostClient::connect(&sock, &token, "desktop").await {
            Ok(c) => return Ok(c),
            Err(e) => last = format!("{e:#}"),
        }
    }
    Err(format!("{alias}: tunnel up but hostd unreachable: {last}"))
}

fn set_status(app: &AppHandle, alias: &str, s: &str) {
    app.state::<Arc<Remotes>>()
        .status
        .lock()
        .insert(alias.to_string(), s.to_string());
    let _ = app.emit("remote://status", json!({"alias": alias, "status": s}));
}

fn configured() -> Vec<String> {
    crate::shell_env::load_settings()
        .extra
        .get("sshHosts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|h| h["alias"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Keep `alias` connected (backoff 2 s → 60 s) while it stays configured.
pub fn keep(app: &AppHandle, alias: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut backoff = Duration::from_secs(2);
        while configured().contains(&alias) {
            set_status(&app, &alias, "connecting");
            match connect_once(&app, &alias).await {
                Ok(c) => {
                    backoff = Duration::from_secs(2);
                    set_status(&app, &alias, "connected");
                    crate::host::serve_client(&app, &host_id(&alias), c).await;
                }
                Err(e) => set_status(&app, &alias, &format!("error: {e}")),
            }
            if let Some(mut c) = app.state::<Arc<Remotes>>().tunnels.lock().remove(&alias) {
                let _ = c.start_kill();
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
        set_status(&app, &alias, "removed");
    });
}

/// Connect every configured host (startup).
pub fn start(app: &AppHandle) {
    for alias in configured() {
        keep(app, alias);
    }
}

#[tauri::command]
pub fn ssh_status(
    state: tauri::State<'_, Arc<Remotes>>,
) -> std::collections::HashMap<String, String> {
    state.status.lock().clone()
}

/// Add (and connect) a host; it is remembered in `settings.sshHosts`.
#[tauri::command]
pub fn ssh_add(app: AppHandle, alias: String) -> Result<(), String> {
    valid_alias(&alias)?;
    let mut hosts: Vec<Value> = crate::shell_env::load_settings()
        .extra
        .get("sshHosts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !hosts.iter().any(|h| h["alias"] == alias.as_str()) {
        hosts.push(json!({"alias": alias}));
        let mut patch = serde_json::Map::new();
        patch.insert("sshHosts".into(), Value::Array(hosts));
        crate::shell_env::settings_patch(patch)?;
    }
    keep(&app, alias);
    Ok(())
}

#[tauri::command]
pub fn ssh_remove(app: AppHandle, alias: String) -> Result<(), String> {
    let hosts: Vec<Value> = crate::shell_env::load_settings()
        .extra
        .get("sshHosts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|h| h["alias"] != alias.as_str())
        .collect();
    let mut patch = serde_json::Map::new();
    patch.insert("sshHosts".into(), Value::Array(hosts));
    crate::shell_env::settings_patch(patch)?;
    if let Some(mut c) = app.state::<Arc<Remotes>>().tunnels.lock().remove(&alias) {
        let _ = c.start_kill();
    }
    Ok(())
}

/// One-shot reachability check: `{home, arch, os, atlas}`.
#[tauri::command]
pub async fn ssh_test(alias: String) -> Result<Value, String> {
    valid_alias(&alias)?;
    let out = ssh(
        &alias,
        r#"printf '%s\n' "$HOME" "$(uname -m)" "$(uname -s)"; "$HOME/.atlas/bin/atlas" version 2>/dev/null | head -1"#,
        Duration::from_secs(20),
    )
    .await?;
    let mut l = out.lines();
    Ok(json!({"home": l.next(), "arch": l.next(), "os": l.next(), "atlas": l.next()}))
}

/// Register a checkout that lives on `alias` (must contain atlas.yaml).
#[tauri::command]
pub async fn projects_add_remote(
    app: AppHandle,
    hub: tauri::State<'_, Arc<HostHub>>,
    alias: String,
    root: String,
) -> Result<String, String> {
    let host = host_id(&alias);
    let root = root.trim_end_matches('/').to_string();
    if !hub.exists(&host, &format!("{root}/atlas.yaml")).await {
        return Err(format!("no atlas.yaml in {alias}:{root}"));
    }
    let mut entries = crate::registry::load()?;
    let id = crate::registry::id_for_host(Some(&host), std::path::Path::new(&root));
    if !entries.iter().any(|e| e.id == id) {
        entries.push(crate::registry::Entry {
            id: id.clone(),
            root,
            pinned: false,
            added: chrono::Utc::now().to_rfc3339(),
            parent: None,
            port_offset: 0,
            branch: None,
            host: Some(host),
            identity: None,
        });
        crate::registry::save(&entries)?;
        let _ = app.emit("projects://changed", ());
    }
    Ok(id)
}

/// `ssh -N -L <local>:localhost:<remote>`: open a remote port locally. Returns the local port.
#[tauri::command]
pub async fn ssh_forward(
    state: tauri::State<'_, Arc<Remotes>>,
    alias: String,
    port: u16,
) -> Result<u16, String> {
    valid_alias(&alias)?;
    if let Some(f) = state
        .forwards
        .lock()
        .iter()
        .find(|f| f.0 == alias && f.1 == port)
    {
        return Ok(f.2);
    }
    let local = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map_err(|e| e.to_string())?
        .port();
    let child = Command::new("ssh")
        .args(SSH_OPTS)
        .args(["-N", "-o", "ExitOnForwardFailure=yes", "-L"])
        .arg(format!("127.0.0.1:{local}:localhost:{port}"))
        .arg(&alias)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if std::net::TcpStream::connect(("127.0.0.1", local)).is_ok() {
            state.forwards.lock().push((alias, port, local, child));
            return Ok(local);
        }
    }
    Err(format!("forward {alias}:{port} did not come up"))
}

#[tauri::command]
pub fn ssh_forwards(state: tauri::State<'_, Arc<Remotes>>) -> Vec<Value> {
    state
        .forwards
        .lock()
        .iter()
        .map(|(a, r, l, _)| json!({"alias": a, "remote": r, "local": l}))
        .collect()
}
