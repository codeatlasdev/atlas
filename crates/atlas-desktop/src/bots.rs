//! Bot host registry: `~/.atlas/desktop/bot-hosts.json`.
//!
//! Bot computers are remote machines running `atlas hostd` under the `atlas-bot` systemd unit
//! (installed via `atlas bot install`). They are also persisted in `settings.sshHosts` so the
//! existing `remote::keep` reconnection loop handles them automatically on startup.

use atlas_core::sys::CommandExt as _;
use std::path::PathBuf;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt as _, BufReader};


/// An entry in the bot-hosts registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotHost {
    /// SSH target, e.g. `atlas-bot@server` or `atlas-bot@192.168.1.10`.
    pub host: String,
    /// Human-readable label shown in the sidebar.
    pub name: String,
}

fn path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".atlas/desktop/bot-hosts.json")
}

pub fn load() -> Vec<BotHost> {
    let Ok(data) = std::fs::read_to_string(path()) else {
        return vec![];
    };
    serde_json::from_str(&data).unwrap_or_default()
}

fn save(hosts: &[BotHost]) -> Result<(), String> {
    let p = path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(hosts).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())
}
/// List all registered bot computers.
#[tauri::command]
pub fn bot_hosts_list() -> Vec<BotHost> {
    load()
}

/// Register a bot computer and connect to it immediately.
///
/// The `host` is an SSH target (`user@hostname` or a `~/.ssh/config` alias). It is added
/// to `settings.sshHosts` so `remote::keep` reconnects it on every startup. When the
/// host is already registered, the existing entry is returned unchanged.
#[tauri::command]
pub fn bot_hosts_add(app: AppHandle, host: String, name: String) -> Result<BotHost, String> {
    let mut hosts = load();
    if let Some(existing) = hosts.iter().find(|h| h.host == host).cloned() {
        return Ok(existing);
    }
    // Persist in sshHosts so remote::keep reconnects on startup.
    crate::remote::ssh_add(app, host.clone())?;
    let entry = BotHost {
        host: host.clone(),
        name,
    };
    hosts.push(entry.clone());
    save(&hosts)?;
    Ok(entry)
}

/// Remove a bot computer from the registry and disconnect it.
#[tauri::command]
pub fn bot_hosts_remove(app: AppHandle, host: String) -> Result<(), String> {
    let mut hosts = load();
    hosts.retain(|h| h.host != host);
    save(&hosts)?;
    crate::remote::ssh_remove(app, host)
}

#[derive(Debug, Clone, Serialize)]
pub struct BotInstallLine {
    pub line: String,
    pub is_err: bool,
}

/// Run `atlas bot install <target> --yes`, streaming each output line as
/// `bot://install-line` to the frontend. Returns the error message on failure.
/// When `sudo_password` is provided the install runs with `--sudo-password-stdin`;
/// the password is piped to the child's stdin and never placed on argv.
#[tauri::command]
pub async fn bot_install(
    app: AppHandle,
    target: String,
    port: Option<u16>,
    sudo_password: Option<String>,
) -> Result<(), String> {
    let bin = crate::cmds::atlas_bin()?;
    let mut cmd = tokio::process::Command::new(bin);
    cmd.no_window()
        .envs(crate::shell_env::vars_async().await)
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let port_s = port.filter(|&p| p != 22).map(|p| p.to_string());
    if sudo_password.is_some() {
        let mut a = vec!["bot", "install", &target, "--yes", "--sudo-password-stdin"];
        if let Some(ps) = &port_s { a.extend(["--port", ps]); }
        cmd.args(a).stdin(Stdio::piped());
    } else {
        let mut a = vec!["bot", "install", &target, "--yes"];
        if let Some(ps) = &port_s { a.extend(["--port", ps]); }
        cmd.args(a).stdin(Stdio::null());
    }
    let mut child = cmd.spawn().map_err(|e| format!("cannot run atlas: {e}"))?;
    // Pipe sudo password to stdin, then close it so the child can proceed
    if let Some(pw) = sudo_password {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(format!("{pw}\n").as_bytes()).await;
            // stdin is dropped here, closing the pipe
        }
        // pw is dropped here
    }
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let (last_out, last_err) = tokio::join!(
        pump_install(stdout, app.clone(), false),
        pump_install(stderr, app.clone(), true),
    );
    let st = child.wait().await.map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        let msg = [last_err, last_out]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or_else(|| format!("atlas bot install failed ({st})"));
        Err(msg)
    }
}

async fn pump_install<R: tokio::io::AsyncRead + Unpin>(
    rd: R,
    app: AppHandle,
    is_err: bool,
) -> String {
    let mut lines = BufReader::new(rd).lines();
    let mut last = String::new();
    while let Ok(Some(l)) = lines.next_line().await {
        let clean = atlas_tui::runtime::text::strip_ansi(&l);
        if !clean.trim().is_empty() {
            last = clean.clone();
        }
        let _ = app.emit(
            "bot://install-line",
            BotInstallLine { line: clean, is_err },
        );
    }
    last
}

// ─── SSH key helpers ─────────────────────────────────────────────────────────

/// Return the path of the Mac's SSH public key, or null if none exists.
/// Checks ~/.ssh/id_ed25519.pub first, then id_rsa.pub.
#[tauri::command]
pub fn ssh_key_find() -> Option<String> {
    let home = dirs::home_dir()?;
    for name in ["id_ed25519.pub", "id_rsa.pub"] {
        let p = home.join(".ssh").join(name);
        if p.exists() {
            return Some(p.to_string_lossy().into_owned());
        }
    }
    None
}

/// Generate an ed25519 key pair at `~/.ssh/id_ed25519`.
/// Fails if the private key file already exists.
#[tauri::command]
pub async fn ssh_keygen() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("no home directory")?;
    let key = home.join(".ssh/id_ed25519");
    if key.exists() {
        return Err("SSH key already exists at ~/.ssh/id_ed25519".into());
    }
    let ssh_dir = home.join(".ssh");
    std::fs::create_dir_all(&ssh_dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&ssh_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let out = tokio::process::Command::new("ssh-keygen")
        .no_window()
        .args(["-t", "ed25519", "-N", "", "-f"])
        .arg(&key)
        .args(["-C", "atlas"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("ssh-keygen: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Authorize this Mac's SSH public key on `target` using password-based SSH
/// authentication. Uses SSH_ASKPASS + SSH_ASKPASS_REQUIRE=force so the password
/// is never on argv; wipes it from memory after use.
#[tauri::command]
pub async fn ssh_authorize_key(target: String, port: Option<u16>, password: String) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("no home directory")?;
    let pub_key_path = ["id_ed25519.pub", "id_rsa.pub"]
        .iter()
        .map(|n| home.join(".ssh").join(n))
        .find(|p| p.exists())
        .ok_or("no SSH public key found; generate one first")?;
    let pubkey = std::fs::read_to_string(&pub_key_path)
        .map_err(|e| format!("reading public key: {e}"))?;
    let pubkey = pubkey.trim().to_string();

    // Write a temporary askpass helper that prints the password.
    // Single-quote-escape the password for embedding in the shell script.
    let escaped = password.replace('\'', "'\\''" );
    let askpass = std::env::temp_dir()
        .join(format!("atlas-askpass-{}.sh", std::process::id()));
    let script = format!("#!/bin/sh\nprintf '%s' '{escaped}'\n");
    std::fs::write(&askpass, &script).map_err(|e| e.to_string())?;
    drop(password); // wipe from local scope (best-effort)
    drop(escaped);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&askpass, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }

    // Remote command: create ~/.ssh and append the public key.
    // Uses stdin for the key content (avoids quoting issues in the shell command).
    let remote_cmd =
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
         cat >> ~/.ssh/authorized_keys && \
         chmod 600 ~/.ssh/authorized_keys";
    let pubkey_line = format!("{pubkey}\n");

    let mut ssh_args: Vec<String> = vec![
        "-o".into(), "PasswordAuthentication=yes".into(),
        "-o".into(), "PubkeyAuthentication=no".into(),
        "-o".into(), "StrictHostKeyChecking=accept-new".into(),
        "-o".into(), "NumberOfPasswordPrompts=1".into(),
    ];
    if let Some(p) = port.filter(|&p| p != 22) {
        ssh_args.extend(["-p".into(), p.to_string()]);
    }
    ssh_args.push(target.clone());
    ssh_args.push(remote_cmd.into());

    let mut child = tokio::process::Command::new("ssh")
        .no_window()
        .args(&ssh_args)
        .env("SSH_ASKPASS", &askpass)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ssh: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(pubkey_line.as_bytes()).await;
    }

    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&askpass); // always delete the helper

    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if err.contains("Permission denied") || err.contains("Authentication failed") {
            Err("Wrong password — check and try again.".into())
        } else if err.is_empty() {
            Err(format!("ssh exited with {}", out.status))
        } else {
            Err(err)
        }
    }
}

// ─── per-agent ship helpers ───────────────────────────────────────────────────

/// Per-agent installation and authentication status on a bot ship.
#[derive(Debug, Clone, Serialize)]
pub struct AgentShipStatus {
    pub installed: bool,
    pub signed_in: bool,
}

/// Check whether an agent is installed and signed in on a bot ship.
/// `host_key` is the full key, e.g. `"ssh:atlas-bot@136.148.72.141"`.
/// The SSH target is derived by stripping the `"ssh:"` prefix.
#[tauri::command]
pub async fn bot_agent_check(host_key: String, agent_id: String) -> Result<AgentShipStatus, String> {
    let ssh_target = host_key.strip_prefix("ssh:").unwrap_or(&host_key);
    let script = check_script(&agent_id).ok_or_else(|| format!("unknown agent {agent_id:?}"))?;
    let ssh = tokio::process::Command::new("ssh")
        .no_window()
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=15", "-o", "ServerAliveInterval=15"])
        .arg(ssh_target)
        .arg(&script)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    // A check that never answers must not leave the row on "Checking…": give up and say so.
    let out = tokio::time::timeout(std::time::Duration::from_secs(45), ssh)
        .await
        .map_err(|_| format!("{ssh_target} did not answer in 45 s"))?
        .map_err(|e| format!("ssh: {e}"))?;
    if !out.status.success() && out.stdout.is_empty() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut parts = stdout.trim().splitn(2, '\t');
    let installed = parts.next().unwrap_or("0") == "1";
    let signed_in = parts.next().unwrap_or("0") == "1";
    Ok(AgentShipStatus { installed, signed_in })
}

/// Install an agent on a bot ship, streaming each line as `bot://install-line` event.
/// `host_key` is the full key, e.g. `"ssh:atlas-bot@136.148.72.141"`.
#[tauri::command]
pub async fn bot_agent_install(app: AppHandle, host_key: String, agent_id: String) -> Result<(), String> {
    let ssh_target = host_key.strip_prefix("ssh:").unwrap_or(&host_key);
    let script = install_script(&agent_id).ok_or_else(|| format!("no installer for {agent_id:?}"))?;
    // Wrap in an interactive-login shell so ~/.bashrc / installers that source it work.
    let full_cmd = format!("bash -lc {}", shq(script));
    let mut child = tokio::process::Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=15", "-o", "ServerAliveInterval=15"])
        .arg(ssh_target)
        .arg(&full_cmd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("ssh: {e}"))?;
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let app2 = app.clone();
    tokio::spawn(pump_install(stdout, app2, false));
    tokio::spawn(pump_install(stderr, app.clone(), true));
    let status = child.wait().await.map_err(|e| format!("ssh wait: {e}"))?;
    if !status.success() {
        return Err(format!("agent install exited {}", status.code().unwrap_or(-1)));
    }
    Ok(())
}

/// Shell script that checks install+auth for `agent_id`. Returns tab-separated "0/1\t0/1\n".
fn check_script(agent_id: &str) -> Option<String> {
    let (bin, auth_check) = match agent_id {
        "omp" => ("omp",
            r#"omp auth export 2>/dev/null | grep -q '"providerType"' && signed_in=1 || true"#),
        "claude" => ("claude",
            r#"claude auth status >/dev/null 2>&1 && signed_in=1 || true"#),
        "codex" => ("codex",
            r#"codex login status >/dev/null 2>&1 && signed_in=1 || true"#),
        "opencode" => ("opencode",
            r#"_oc=$(opencode auth list 2>/dev/null); [ -n "$_oc" ] && ! echo "$_oc" | grep -q '0 credentials' && signed_in=1 || true"#),
        _ => return None,
    };
    Some(format!(
        concat!(
            r#"export PATH="$HOME/.bun/bin:$HOME/.local/bin:$HOME/.opencode/bin:$HOME/.npm-global/bin:$PATH"; "#,
            r#"bin=0; command -v {bin} >/dev/null 2>&1 && bin=1; "#,
            r#"signed_in=0; "#,
            r#"if [ "$bin" = "1" ]; then {auth_check}; "#,
            r#"  ef="$HOME/.atlas/agent-env/{id}.env"; "#,
            r#"  [ "$signed_in" = "0" ] && [ -f "$ef" ] && grep -qE '^[A-Z_]+=.' "$ef" 2>/dev/null && signed_in=1 || true; "#,
            r#"fi; "#,
            r#"printf '%s\t%s\n' "$bin" "$signed_in""#
        ),
        bin = bin,
        auth_check = auth_check,
        id = agent_id,
    ))
}

fn install_script(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "omp" => Some("curl -fsSL https://omp.sh/install | sh"),
        "claude" => Some("curl -fsSL https://claude.ai/install.sh | bash"),
        "opencode" => Some("curl -fsSL https://opencode.ai/install | bash"),
        "codex" => Some(r#"npm_config_prefix="$HOME/.npm-global" npm install -g @openai/codex"#),
        _ => None,
    }
}

/// Shell-quote a string for use as a single argument.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(hosts: &[BotHost]) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let data = serde_json::to_string_pretty(hosts).unwrap();
        std::fs::write(tmp.path(), data).unwrap();
        let back: Vec<BotHost> =
            serde_json::from_str(&std::fs::read_to_string(tmp.path()).unwrap()).unwrap();
        assert_eq!(hosts, back.as_slice());
    }

    #[test]
    fn empty_list_roundtrips() {
        roundtrip(&[]);
    }

    #[test]
    fn entries_roundtrip() {
        roundtrip(&[
            BotHost {
                host: "atlas-bot@server".into(),
                name: "My Server".into(),
            },
            BotHost {
                host: "atlas-bot@192.168.1.5".into(),
                name: "Local VM".into(),
            },
        ]);
    }

    #[test]
    fn check_script_covers_all_agents() {
        for id in ["omp", "claude", "codex", "opencode"] {
            let s = check_script(id).unwrap();
            assert!(s.contains(".bun/bin"), "{id}: missing .bun/bin in PATH");
            assert!(s.contains(".local/bin"), "{id}: missing .local/bin in PATH");
            assert!(s.contains(id), "{id}: missing bin name in script");
        }
        assert!(check_script("unknown").is_none());
    }
}
