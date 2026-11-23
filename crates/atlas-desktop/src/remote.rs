//! SSH hosts: a remote `atlas hostd` reached through an `ssh` tunnel.
//!
//! Per host alias (from `~/.ssh/config`): check `~/.atlas/bin/atlas` (upload
//! `~/.atlas/remote/atlas-<arch>-linux` when missing), start `atlas hostd`, then
//! keep `ssh -N -L <local sock>:<remote sock>` running and connect a `HostClient`
//! through it. The host's own hostd runs the agent hook server, so its agents'
//! status arrives as `agent_status` pushes over this connection.
//! Reconnects with backoff while the host stays in `settings.sshHosts`.

use atlas_core::ssh::valid_target;
use atlas_core::sys::CommandExt as _;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use atlas_host::HostClient;
use atlas_i18n::{t, t_in};
use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::{Child, Command};

use crate::host::HostHub;

#[derive(Default)]
pub struct Remotes {
    tunnels: Mutex<std::collections::HashMap<String, Child>>,
    status: Mutex<std::collections::HashMap<String, String>>,
    forwards: Mutex<Vec<(String, u16, u16, Child)>>,
    /// Where each connected host's hostd socket (the tunnel's end) is and its token, for the dedicated raw
    /// connections of terminal views (`pty_stream.rs`).
    endpoints: Mutex<std::collections::HashMap<String, (PathBuf, String)>>,
}

pub fn host_id(alias: &str) -> String {
    format!("ssh:{alias}")
}

/// The tunnel socket and token of the connected SSH host `host` (`ssh:<alias>`).
pub fn endpoint(app: &AppHandle, host: &str) -> Option<(PathBuf, String)> {
    app.state::<Arc<Remotes>>()
        .endpoints
        .lock()
        .get(host.strip_prefix("ssh:")?)
        .cloned()
}

/// [`valid_target`] with the refusal in the display language: it is shown in Settings and the crew onboarding.
pub fn valid_alias(alias: &str) -> Result<(), String> {
    valid_target(alias).map_err(|_| t!("native_remote_invalid_alias", alias = alias))
}

/// What the page shows for one host (`ssh_status`, the `remote://status` event). The wire form is one string:
/// an id, or `warning:` / `error:` followed by the sentence. `apps/desktop/src/lib/settings/sshStatus.ts` maps it
/// to a chip; the fixture `__fixtures__/ssh-status.json` is written from here, so a new status fails a test on
/// both sides until the page maps it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SshStatus {
    Connecting,
    Connected,
    Removed,
    /// Connected, but something is wrong with the host's atlas (the sentence is in the display language).
    Warning(String),
    /// Not connected, and why (the sentence is in the display language, ssh's own text inside).
    Error(String),
}

impl SshStatus {
    fn kind(&self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Removed => "removed",
            Self::Warning(_) => "warning",
            Self::Error(_) => "error",
        }
    }

    fn wire(&self) -> String {
        match self {
            Self::Connecting | Self::Connected | Self::Removed => self.kind().to_string(),
            // i18n-ignore: wire prefix that sshStatus.ts matches, followed by the translated sentence
            Self::Warning(why) | Self::Error(why) => format!("{}: {why}", self.kind()),
        }
    }
}

async fn ssh(alias: &str, script: &str, timeout: Duration) -> Result<String, String> {
    let fut = Command::new("ssh")
        .no_window()
        .args(crate::crew::server_ssh(alias).raw_argv(script))
        .stdin(Stdio::null())
        .output();
    let out = tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| {
            t!(
                "native_remote_no_answer",
                target = alias,
                seconds = timeout.as_secs()
            )
        })?
        .map_err(|e| {
            t!(
                "native_remote_spawn_failed",
                program = "ssh",
                error = e.to_string()
            )
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let detail = match stderr.trim() {
            "" => out.status.to_string(),
            s => s.to_string(),
        };
        Err(t!(
            "native_remote_ssh_failed",
            alias = alias,
            detail = detail
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
        return Err(t!("native_remote_no_home", alias = alias));
    }
    if !has {
        let bin = local_dir().join(format!("atlas-{arch}-{os}"));
        if !bin.is_file() {
            return Err(t!(
                "native_remote_atlas_missing",
                alias = alias,
                arch = arch,
                system = &os,
                path = bin.display().to_string()
            ));
        }
        ssh(alias, "mkdir -p ~/.atlas/bin", Duration::from_secs(15)).await?;
        let st = Command::new("scp")
            .no_window()
            .args(crate::crew::server_ssh(alias).scp_argv(&bin, ".atlas/bin/atlas"))
            .status()
            .await
            .map_err(|e| {
                t!(
                    "native_remote_spawn_failed",
                    program = "scp",
                    error = e.to_string()
                )
            })?;
        if !st.success() {
            return Err(t!("native_remote_upload_failed", alias = alias));
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
    let dir = local_dir();
    std::fs::create_dir_all(&dir).map_err(|e| {
        t!(
            "native_remote_dir_failed",
            path = dir.display().to_string(),
            error = e.to_string()
        )
    })?;
    let sock = dir.join(format!("{alias}.sock"));
    let _ = std::fs::remove_file(&sock);
    let child = Command::new("ssh")
        .no_window()
        .args(
            crate::crew::server_ssh(alias)
                .tunnel_argv(&format!("{}:{home}/.atlas/hostd.sock", sock.display())),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            t!(
                "native_remote_spawn_failed",
                program = "ssh",
                error = e.to_string()
            )
        })?;
    app.state::<Arc<Remotes>>()
        .tunnels
        .lock()
        .insert(alias.to_string(), child);
    let mut last = String::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        match HostClient::connect(&sock, &token, "desktop").await {
            Ok(c) => {
                app.state::<Arc<Remotes>>()
                    .endpoints
                    .lock()
                    .insert(alias.to_string(), (sock.clone(), token.clone()));
                return Ok(c);
            }
            Err(e) => last = format!("{e:#}"),
        }
    }
    Err(t!(
        "native_remote_hostd_unreachable",
        alias = alias,
        detail = last
    ))
}

/// The reconnect error in words a user can act on; ssh's own text stays at the end for detail.
fn plain_reason(e: &str) -> String {
    plain_reason_in(atlas_i18n::lang(), e)
}

fn plain_reason_in(lang: &str, e: &str) -> String {
    if e.contains("Connection refused") || e.contains("No route to host") || e.contains("timed out")
    {
        t_in!(lang, "native_remote_reason_down", detail = e)
    } else if e.contains("Permission denied") {
        t_in!(lang, "native_remote_reason_key", detail = e)
    } else if e.contains("Could not resolve hostname") {
        t_in!(lang, "native_remote_reason_dns", detail = e)
    } else {
        e.to_string()
    }
}

/// The warning of a host whose atlas hostd is older than this app: what it lacks and what to do.
fn lacks_in(lang: &str, missing: &[&str]) -> String {
    t_in!(
        lang,
        "native_remote_lacks_features",
        features = missing.join(", ")
    )
}

fn set_status(app: &AppHandle, alias: &str, s: &SshStatus) {
    let wire = s.wire();
    app.state::<Arc<Remotes>>()
        .status
        .lock()
        .insert(alias.to_string(), wire.clone());
    let _ = app.emit("remote://status", json!({"alias": alias, "status": wire}));
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
            set_status(&app, &alias, &SshStatus::Connecting);
            match connect_once(&app, &alias).await {
                Ok(c) => {
                    backoff = Duration::from_secs(2);
                    // A host's atlas is only uploaded when missing, so an older one keeps
                    // running: say what it lacks instead of a silent "connected".
                    let missing = match c.call("info", json!({})).await {
                        Ok(info) => atlas_host::upgrade::missing_caps(&info),
                        Err(_) => Vec::new(),
                    };
                    if missing.is_empty() {
                        set_status(&app, &alias, &SshStatus::Connected);
                    } else {
                        let why = lacks_in(atlas_i18n::lang(), &missing);
                        set_status(&app, &alias, &SshStatus::Warning(why));
                    }
                    crate::host::serve_client(&app, &host_id(&alias), c).await;
                }
                Err(e) => {
                    set_status(&app, &alias, &SshStatus::Error(e.clone()));
                    app.state::<Arc<crate::host::HostHub>>()
                        .set_remote_error(&host_id(&alias), &plain_reason(&e));
                }
            }
            if let Some(mut c) = app.state::<Arc<Remotes>>().tunnels.lock().remove(&alias) {
                let _ = c.start_kill();
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
        set_status(&app, &alias, &SshStatus::Removed);
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
        return Err(atlas_i18n::t!(
            "native_remote_no_atlas_yaml",
            alias = &alias,
            root = &root
        ));
    }
    let id = crate::registry::id_for_host(Some(&host), std::path::Path::new(&root));
    let fresh = crate::registry::edit(|entries| {
        if entries.iter().any(|e| e.id == id) {
            return Ok(false);
        }
        entries.push(crate::registry::Entry {
            id: id.clone(),
            root,
            pinned: false,
            added: atlas_core::time::utc_rfc3339_now(),
            parent: None,
            port_offset: 0,
            branch: None,
            host: Some(host),
            identity: None,
            project: false,
        });
        Ok(true)
    })?;
    if fresh {
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
        .map_err(|e| t!("native_remote_local_port_failed", error = e.to_string()))?
        .port();
    let child = Command::new("ssh")
        .no_window()
        .args(
            crate::crew::server_ssh(&alias)
                .tunnel_argv(&format!("127.0.0.1:{local}:localhost:{port}")),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            t!(
                "native_remote_spawn_failed",
                program = "ssh",
                error = e.to_string()
            )
        })?;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if std::net::TcpStream::connect(("127.0.0.1", local)).is_ok() {
            state.forwards.lock().push((alias, port, local, child));
            return Ok(local);
        }
    }
    Err(t!(
        "native_remote_forward_failed",
        port = port.to_string(),
        alias = &alias
    ))
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

#[cfg(test)]
mod reason_tests {
    use super::{SshStatus, lacks_in, plain_reason_in};
    use atlas_i18n::t_in;

    const LANGS: [&str; 2] = ["en", "pt-BR"];

    /// The sentence a reason starts with: the message with its `{detail}` left empty.
    fn lead(lang: &str, key: &str) -> String {
        let text = atlas_i18n::translate(lang, key, Some(&detail_args("")));
        text.trim_end().to_string()
    }

    fn detail_args(detail: &str) -> atlas_i18n::Args {
        let mut a = atlas_i18n::Args::new();
        a.set("detail", detail);
        a
    }

    // Incident: the test ship's container was stopped and the app said "not connected (Settings → Integrations →
    // SSH hosts)", which sent the user to settings that were fine. The reason must say what is actually wrong.
    #[test]
    fn a_server_that_is_down_is_named_as_down() {
        let ssh = "ssh: connect to host 127.0.0.1 port 2222: Connection refused";
        for lang in LANGS {
            let r = plain_reason_in(lang, ssh);
            assert!(
                r.starts_with(&lead(lang, "native_remote_reason_down")),
                "{lang}: {r}"
            );
            assert!(
                r.ends_with(ssh),
                "{lang}: ssh's own words stay at the end: {r}"
            );
        }
    }

    #[test]
    fn a_rejected_key_is_not_called_a_dead_server() {
        let ssh = "atlas-bot@h: Permission denied (publickey).";
        for lang in LANGS {
            let r = plain_reason_in(lang, ssh);
            assert!(
                r.starts_with(&lead(lang, "native_remote_reason_key")),
                "{lang}: {r}"
            );
            assert!(r.ends_with(ssh), "{lang}: {r}");
        }
    }

    #[test]
    fn a_name_that_does_not_resolve_is_named() {
        let ssh = "ssh: Could not resolve hostname nope: Name or service not known";
        for lang in LANGS {
            let r = plain_reason_in(lang, ssh);
            assert!(
                r.starts_with(&lead(lang, "native_remote_reason_dns")),
                "{lang}: {r}"
            );
            assert!(r.ends_with(ssh), "{lang}: {r}");
        }
    }

    #[test]
    fn the_three_hints_are_three_different_sentences() {
        let hints: Vec<String> = [
            "native_remote_reason_down",
            "native_remote_reason_key",
            "native_remote_reason_dns",
        ]
        .iter()
        .flat_map(|k| LANGS.map(|l| lead(l, k)))
        .collect();
        let mut unique = hints.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), hints.len(), "{hints:?}");
    }

    #[test]
    fn anything_else_is_passed_through_unchanged() {
        for lang in LANGS {
            assert_eq!(
                plain_reason_in(lang, "tunnel up but hostd unreachable: x"),
                "tunnel up but hostd unreachable: x"
            );
        }
    }

    // The classification reads ssh's own English text, never the app's translated sentences: our own "did not
    // answer" already says it, and must not get a second hint put in front of it in any language.
    #[test]
    fn the_apps_own_timeout_is_not_classified_again() {
        for lang in LANGS {
            let ours = t_in!(
                lang,
                "native_remote_no_answer",
                target = "box",
                seconds = 20
            );
            assert_eq!(plain_reason_in(lang, &ours), ours, "{lang}");
        }
    }

    #[test]
    fn the_outdated_warning_names_what_the_host_lacks() {
        let missing = vec!["mobile", "crew"];
        for lang in LANGS {
            let w = lacks_in(lang, &missing);
            assert!(w.contains("mobile, crew"), "{lang}: {w}");
        }
        assert_ne!(lacks_in("en", &missing), lacks_in("pt-BR", &missing));
    }

    /// One status of each kind, as the producer builds them (English, so the fixture is stable).
    fn samples() -> Vec<SshStatus> {
        let missing = vec!["mobile", "crew"];
        vec![
            SshStatus::Connecting,
            SshStatus::Connected,
            SshStatus::Removed,
            SshStatus::Warning(lacks_in("en", &missing)),
            SshStatus::Error(t_in!(
                "en",
                "native_remote_hostd_unreachable",
                alias = "example-host",
                detail = "connection refused"
            )),
        ]
    }

    #[test]
    fn the_wire_form_is_an_id_or_a_prefix_and_the_sentence() {
        assert_eq!(SshStatus::Connecting.wire(), "connecting");
        assert_eq!(SshStatus::Connected.wire(), "connected");
        assert_eq!(SshStatus::Removed.wire(), "removed");
        assert_eq!(SshStatus::Warning("w".into()).wire(), "warning: w");
        assert_eq!(SshStatus::Error("e: x".into()).wire(), "error: e: x");
    }

    fn fixture_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/ssh-status.json")
    }

    /// Contract with Settings > SSH hosts: `__fixtures__/ssh-status.json` maps each kind of status to the REAL
    /// string the producer sends (`SshStatus::wire`). `sshStatus.test.ts` runs the page's mapping over it, so a
    /// status added or reworded here is a red test there until the page maps it.
    /// `UPDATE_FIXTURES=1 cargo test -p atlas-desktop ssh_status_fixture` rewrites it.
    #[test]
    fn ssh_status_fixture_is_what_rust_sends() {
        let wires: serde_json::Map<String, serde_json::Value> = samples()
            .iter()
            .map(|s| (s.kind().to_string(), serde_json::Value::String(s.wire())))
            .collect();
        assert_eq!(wires.len(), samples().len(), "one sample per kind");
        // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(&serde_json::Value::Object(wires.clone()), &mut ser).unwrap();
        let fresh = String::from_utf8(buf).unwrap() + "\n";
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(fixture_path(), &fresh).unwrap();
        }
        let committed = std::fs::read_to_string(fixture_path())
            .expect("ssh-status.json (UPDATE_FIXTURES=1 writes it)");
        // Values, not bytes: key order depends on serde_json's `preserve_order` in the build.
        let committed: serde_json::Value = serde_json::from_str(&committed).unwrap();
        assert_eq!(
            committed,
            serde_json::Value::Object(wires),
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }
}
