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
}
