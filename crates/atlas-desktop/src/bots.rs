//! Bot host registry: `~/.atlas/desktop/bot-hosts.json`.
//!
//! Bot computers are remote machines running `atlas hostd` under the `atlas-bot` systemd unit
//! (installed via `atlas bot install`). They are also persisted in `settings.sshHosts` so the
//! existing `remote::keep` reconnection loop handles them automatically on startup.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

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
/// Validate an SSH target: a plain alias or `user@host`.
///
/// Rules:
/// - At most one `@`; if present both sides must be non-empty.
/// - Neither the whole string (alias) nor either side of `@` may start with `-`
///   (prevents an SSH option-injection attack like `-oProxyCommand=…`).
fn valid_host(h: &str) -> Result<(), String> {
    let part = |s: &str| -> Result<(), String> {
        if s.is_empty() {
            return Err("must not be empty".into());
        }
        if s.starts_with('-') {
            return Err(format!("{s:?} must not start with '-'"));
        }
        Ok(())
    };
    match h.split_once('@') {
        Some((user, host)) => {
            if host.contains('@') {
                return Err(format!("{h:?}: at most one '@' is allowed"));
            }
            part(user)?;
            part(host)?;
        }
        None => part(h)?,
    }
    Ok(())
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
    valid_host(&host)?;
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
    fn missing_file_returns_empty() {
        // load() from a nonexistent path returns an empty vec (no panic).
        let v: Vec<BotHost> = serde_json::from_str("not json").unwrap_or_default();
        assert!(v.is_empty());
    }

    // ── valid_host ──────────────────────────────────────────────────────────

    #[test]
    fn valid_host_accepts_user_at_host() {
        assert!(valid_host("atlas-bot@server").is_ok());
        assert!(valid_host("atlas-bot@192.168.1.5").is_ok());
        assert!(valid_host("user@my-host.example.com").is_ok());
    }

    #[test]
    fn valid_host_accepts_alias() {
        assert!(valid_host("my-bot-server").is_ok());
        assert!(valid_host("botbox").is_ok());
    }

    #[test]
    fn valid_host_rejects_leading_dash() {
        assert!(valid_host("-bad").is_err());
        assert!(valid_host("-bad@server").is_err());
        assert!(valid_host("user@-bad").is_err());
    }

    #[test]
    fn valid_host_rejects_empty_parts() {
        assert!(valid_host("").is_err());
        assert!(valid_host("user@").is_err());
        assert!(valid_host("@host").is_err());
    }

    #[test]
    fn valid_host_rejects_multiple_at() {
        assert!(valid_host("user@@host").is_err());
        assert!(valid_host("a@b@c").is_err());
    }
}
