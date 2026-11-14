//! Crew server registry: `~/.atlas/desktop/crew-servers.json`.
//!
//! Crew servers are remote machines running `atlas hostd` under the `atlas-crew` systemd unit
//! (installed via `atlas crew install`). They are also persisted in `settings.sshHosts` so the
//! existing `remote::keep` reconnection loop handles them automatically on startup.

use atlas_core::sys::CommandExt as _;
use std::path::PathBuf;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt as _, BufReader};

/// An entry in the crew-servers registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrewServer {
    /// SSH target, e.g. `atlas-crew@server` (a server installed as "Atlas Bot" is `atlas-bot@server`).
    pub host: String,
    /// Human-readable label shown in the sidebar.
    pub name: String,
}

fn path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".atlas/desktop/crew-servers.json")
}

pub fn load() -> Vec<CrewServer> {
    let Ok(data) = std::fs::read_to_string(path()) else {
        return vec![];
    };
    serde_json::from_str(&data).unwrap_or_default()
}

fn save(hosts: &[CrewServer]) -> Result<(), String> {
    let p = path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_string_pretty(hosts).map_err(|e| e.to_string())?;
    std::fs::write(&p, data).map_err(|e| e.to_string())
}
/// List all registered crew servers.
#[tauri::command]
pub fn crew_servers_list() -> Vec<CrewServer> {
    load()
}

/// Register a crew server and connect to it immediately.
///
/// The `host` is an SSH target (`user@hostname` or a `~/.ssh/config` alias). It is added
/// to `settings.sshHosts` so `remote::keep` reconnects it on every startup. When the
/// host is already registered, the existing entry is returned unchanged.
#[tauri::command]
pub fn crew_servers_add(app: AppHandle, host: String, name: String) -> Result<CrewServer, String> {
    let mut hosts = load();
    if let Some(existing) = hosts.iter().find(|h| h.host == host).cloned() {
        return Ok(existing);
    }
    // Persist in sshHosts so remote::keep reconnects on startup.
    crate::remote::ssh_add(app, host.clone())?;
    let entry = CrewServer {
        host: host.clone(),
        name,
    };
    hosts.push(entry.clone());
    save(&hosts)?;
    Ok(entry)
}

/// Remove a crew server from the registry and disconnect it.
#[tauri::command]
pub fn crew_servers_remove(app: AppHandle, host: String) -> Result<(), String> {
    let mut hosts = load();
    hosts.retain(|h| h.host != host);
    save(&hosts)?;
    crate::remote::ssh_remove(app, host)
}

#[derive(Debug, Clone, Serialize)]
pub struct CrewInstallLine {
    pub line: String,
    pub is_err: bool,
}

/// Run `atlas crew install <target> --yes`, streaming each output line as
/// `crew://install-line` to the frontend. Returns the error message on failure.
/// When `sudo_password` is provided the install runs with `--sudo-password-stdin`;
/// the password is piped to the child's stdin and never placed on argv.
#[tauri::command]
pub async fn crew_server_install(
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
        let mut a = vec!["crew", "install", &target, "--yes", "--sudo-password-stdin"];
        if let Some(ps) = &port_s {
            a.extend(["--port", ps]);
        }
        cmd.args(a).stdin(Stdio::piped());
    } else {
        let mut a = vec!["crew", "install", &target, "--yes"];
        if let Some(ps) = &port_s {
            a.extend(["--port", ps]);
        }
        cmd.args(a).stdin(Stdio::null());
    }
    let mut child = cmd.spawn().map_err(|e| format!("cannot run atlas: {e}"))?;
    // Pipe sudo password to stdin, then close it so the child can proceed
    // stdin and the password are dropped at the end of the block, closing the pipe.
    if let Some(pw) = sudo_password
        && let Some(mut stdin) = child.stdin.take()
    {
        let _ = stdin.write_all(format!("{pw}\n").as_bytes()).await;
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
            .unwrap_or_else(|| format!("atlas crew install failed ({st})"));
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
            "crew://install-line",
            CrewInstallLine {
                line: clean,
                is_err,
            },
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
pub async fn ssh_authorize_key(
    target: String,
    port: Option<u16>,
    password: String,
) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("no home directory")?;
    let pub_key_path = ["id_ed25519.pub", "id_rsa.pub"]
        .iter()
        .map(|n| home.join(".ssh").join(n))
        .find(|p| p.exists())
        .ok_or("no SSH public key found; generate one first")?;
    let pubkey =
        std::fs::read_to_string(&pub_key_path).map_err(|e| format!("reading public key: {e}"))?;
    let pubkey = pubkey.trim().to_string();

    let pubkey_line = format!("{pubkey}\n");

    // Remote command: create ~/.ssh and append the public key.
    // Uses stdin for the key content (avoids quoting issues in the shell command).
    let remote_cmd = "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
         cat >> ~/.ssh/authorized_keys && \
         chmod 600 ~/.ssh/authorized_keys";

    let mut ssh_args: Vec<String> = vec![
        "-o".into(),
        "PasswordAuthentication=yes".into(),
        "-o".into(),
        "PubkeyAuthentication=no".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "NumberOfPasswordPrompts=1".into(),
    ];
    if let Some(p) = port.filter(|&p| p != 22) {
        ssh_args.extend(["-p".into(), p.to_string()]);
    }
    ssh_args.push(target);
    ssh_args.push(remote_cmd.into());

    let out = run_ssh_with_password(
        "ssh",
        &ssh_args,
        &std::env::temp_dir(),
        &password,
        &pubkey_line,
    )
    .await?;

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

/// Runs `program args` with `password` served through a temporary `SSH_ASKPASS` helper in `dir`
/// and `stdin_data` on its stdin. The password is never on argv.
async fn run_ssh_with_password(
    program: &str,
    args: &[String],
    dir: &std::path::Path,
    password: &str,
    stdin_data: &str,
) -> Result<std::process::Output, String> {
    // Single-quote-escape the password for embedding in the shell script.
    let escaped = password.replace('\'', "'\\''");
    // The guard exists before the file does: whichever way this function returns (`?`, panic,
    // cancelled future), the helper holding the password is deleted.
    let askpass = AskpassFile(dir.join(format!("atlas-askpass-{}.sh", std::process::id())));
    askpass.write(&format!("#!/bin/sh\nprintf '%s' '{escaped}'\n"))?;

    let mut child = tokio::process::Command::new(program)
        .no_window()
        .args(args)
        .env("SSH_ASKPASS", &askpass.0)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ssh: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_data.as_bytes()).await;
    }

    child.wait_with_output().await.map_err(|e| e.to_string())
}

/// The temporary `SSH_ASKPASS` helper. Deleted when dropped, on every path.
struct AskpassFile(PathBuf);

impl AskpassFile {
    /// Creates the file owner-only (0700 from the first byte on unix) and writes `script` to it.
    fn write(&self, script: &str) -> Result<(), String> {
        use std::io::Write as _;
        // A leftover from a crashed run (any mode) is replaced, never reused.
        let _ = std::fs::remove_file(&self.0);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o700);
        }
        opts.open(&self.0)
            .and_then(|mut f| f.write_all(script.as_bytes()))
            .map_err(|e| e.to_string())
    }
}

impl Drop for AskpassFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// ─── per-agent ship helpers ───────────────────────────────────────────────────

/// Per-agent installation and authentication status on a crew server.
#[derive(Debug, Clone, Serialize)]
pub struct AgentShipStatus {
    pub installed: bool,
    pub signed_in: bool,
}

/// Check whether an agent is installed and signed in on a crew server.
/// `host_key` is the full key, e.g. `"ssh:atlas-crew@136.148.72.141"`.
/// The SSH target is derived by stripping the `"ssh:"` prefix.
#[tauri::command]
pub async fn crew_agent_check(
    host_key: String,
    agent_id: String,
) -> Result<AgentShipStatus, String> {
    let ssh_target = host_key.strip_prefix("ssh:").unwrap_or(&host_key);
    let script = check_script(&agent_id).ok_or_else(|| format!("unknown agent {agent_id:?}"))?;
    let ssh = tokio::process::Command::new("ssh")
        .no_window()
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "ServerAliveInterval=15",
        ])
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
    Ok(AgentShipStatus {
        installed,
        signed_in,
    })
}

/// Install an agent on a crew server, streaming each line as `crew://install-line` event.
/// `host_key` is the full key, e.g. `"ssh:atlas-crew@136.148.72.141"`.
#[tauri::command]
pub async fn crew_agent_install(
    app: AppHandle,
    host_key: String,
    agent_id: String,
) -> Result<(), String> {
    let ssh_target = host_key.strip_prefix("ssh:").unwrap_or(&host_key);
    let script =
        install_script(&agent_id).ok_or_else(|| format!("no installer for {agent_id:?}"))?;
    // Wrap in an interactive-login shell so ~/.bashrc / installers that source it work.
    let full_cmd = format!("bash -lc {}", shq(&script));
    let mut child = tokio::process::Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "ServerAliveInterval=15",
        ])
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
        return Err(format!(
            "agent install exited {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

/// Shell script that checks install+auth for `agent_id`. Returns tab-separated "0/1\t0/1\n".
fn check_script(agent_id: &str) -> Option<String> {
    let (bin, auth_check) = match agent_id {
        "omp" => (
            "omp",
            r#"omp auth export 2>/dev/null | grep -q '"providerType"' && signed_in=1 || true"#,
        ),
        "claude" => (
            "claude",
            r#"claude auth status >/dev/null 2>&1 && signed_in=1 || true"#,
        ),
        "codex" => (
            "codex",
            r#"codex login status >/dev/null 2>&1 && signed_in=1 || true"#,
        ),
        "opencode" => (
            "opencode",
            r#"_oc=$(opencode auth list 2>/dev/null); [ -n "$_oc" ] && ! echo "$_oc" | grep -q '0 credentials' && signed_in=1 || true"#,
        ),
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

/// The agent's own installer, then `atlas setup <agent> --apply`: without Atlas's hooks the agent never reports
/// "done" to hostd, so every crewmate task sat on "Working" until the 2 h timeout.
fn install_script(agent_id: &str) -> Option<String> {
    let (install, setup) = match agent_id {
        // A crewmate's omp runs in a pane nobody watches: its first-run wizard ("↑/↓ select · ⏎ confirm") waited
        // for a key forever and the task sat on "Working". Mark the wizard done for atlas-bot only (never on the
        // user's Mac, where the wizard is omp's real onboarding).
        "omp" => (
            r#"curl -fsSL https://omp.sh/install | sh && mkdir -p "$HOME/.omp/agent" && f="$HOME/.omp/agent/config.yml" && touch "$f" && chmod 600 "$f" && { grep -q '^setupVersion:' "$f" || echo 'setupVersion: 2' >> "$f"; }"#,
            "omp",
        ),
        "claude" => (
            "curl -fsSL https://claude.ai/install.sh | bash",
            "claude-code",
        ),
        "opencode" => ("curl -fsSL https://opencode.ai/install | bash", "opencode"),
        "codex" => (
            r#"npm_config_prefix="$HOME/.npm-global" npm install -g @openai/codex"#,
            "codex",
        ),
        _ => return None,
    };
    Some(format!(
        r#"{install} && export PATH="$HOME/.atlas/bin:$HOME/.bun/bin:$HOME/.local/bin:$HOME/.opencode/bin:$HOME/.npm-global/bin:$PATH" && atlas setup {setup} --apply </dev/null"#
    ))
}

/// Shell-quote a string for use as a single argument.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

// ─── provider probe ──────────────────────────────────────────────────────────

/// Call `<base_url>/models` from the desktop to test a custom gateway.
///
/// Uses `curl` so no new Rust dep is needed and the request goes from the
/// desktop process, bypassing the webview CSP.  The API key is passed in the
/// Authorization header and never written to stdout, stderr, or argv (we build
/// the header value as a Rust `String` and pass it directly as an arg; it
/// briefly appears in the process arg list on the local Mac, which is
/// acceptable for a desktop-only connection-test command).
///
/// Returns the list of model IDs on success, or an error string describing the
/// HTTP status / connection failure.
#[tauri::command]
pub async fn provider_probe(base_url: String, api_key: String) -> Result<Vec<String>, String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let auth = format!("Authorization: Bearer {api_key}");

    let out = tokio::process::Command::new("curl")
        .no_window()
        .args(["-sf", "-m", "15", "-H", &auth, "-w", "\n%{http_code}", &url])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("curl: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    // curl appends `\n<status_code>` via `-w "\n%{http_code}"`.
    let (body, status_str) = stdout.rsplit_once('\n').unwrap_or(("", stdout.as_ref()));
    let status: u16 = status_str.trim().parse().unwrap_or(0);

    if status == 0 {
        // curl itself failed (DNS, TLS, timeout …).
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            format!("could not reach {url}")
        } else {
            format!("connection error: {detail}")
        });
    }
    if status == 401 || status == 403 {
        return Err(format!("HTTP {status}: invalid API key"));
    }
    if status != 200 {
        return Err(format!("HTTP {status} from gateway"));
    }

    // Parse model list.  Supports the OpenAI { data: [{id}] } shape and the
    // Anthropic { models: [{id}] } shape.
    let val: serde_json::Value =
        serde_json::from_str(body.trim()).map_err(|_| "gateway returned non-JSON".to_string())?;

    let ids: Vec<String> = if let Some(arr) = val.get("data").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect()
    } else if let Some(arr) = val.get("models").and_then(|v| v.as_array()) {
        arr.iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect()
    } else {
        vec![]
    };

    Ok(ids)
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(hosts: &[CrewServer]) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let data = serde_json::to_string_pretty(hosts).unwrap();
        std::fs::write(tmp.path(), data).unwrap();
        let back: Vec<CrewServer> =
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
            CrewServer {
                host: "atlas-bot@server".into(),
                name: "My Server".into(),
            },
            CrewServer {
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

    // Incident: omp installed on a ship without Atlas's hooks never reported "done", so the crewmate's task showed
    // "Working · 62m" until the 2 h timeout. Every engine install must wire the hooks, with the setup name the
    // CLI actually accepts (claude's is `claude-code`), and only after the agent itself installed.
    #[test]
    fn every_engine_install_wires_atlas_hooks_after_installing() {
        for (id, setup) in [
            ("omp", "omp"),
            ("claude", "claude-code"),
            ("codex", "codex"),
            ("opencode", "opencode"),
        ] {
            let s = install_script(id).unwrap();
            let at = s
                .find(&format!("atlas setup {setup} --apply"))
                .unwrap_or_else(|| panic!("{id}: no hooks"));
            assert!(
                s[..at].contains("&&"),
                "{id}: hooks must run only after a successful install"
            );
        }
        assert!(install_script("unknown").is_none());
    }

    // Incident: a fresh omp on the ship stopped at its first-run wizard and every task hung on "Working".
    // Runs the real omp install script (installer stubbed) in a scratch HOME: the wizard must be marked done,
    // once, in a file only atlas-bot can read, keeping anything already in it.
    #[cfg(unix)]
    #[test]
    fn omp_install_marks_the_first_run_wizard_done_once() {
        let home = tempfile::tempdir().unwrap();
        let cfg = home.path().join(".omp/agent/config.yml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, "theme: dark\n").unwrap();
        let script = install_script("omp").unwrap();
        let prep = script
            .split(" && export PATH=")
            .next()
            .unwrap()
            .replace("curl -fsSL https://omp.sh/install | sh", "true");
        for _ in 0..2 {
            let ok = std::process::Command::new("sh")
                .arg("-c")
                .arg(&prep)
                .env("HOME", home.path())
                .status()
                .unwrap()
                .success();
            assert!(ok);
        }
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert_eq!(text, "theme: dark\nsetupVersion: 2\n");
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&cfg).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    fn askpass_files(dir: &std::path::Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("atlas-askpass-"))
            .collect()
    }

    // Crew review P0, PR #106 comment 6874910355: the SSH bootstrap wrote the account password to
    // $TMPDIR/atlas-askpass-<pid>.sh and removed it only after ssh finished; a failed spawn (ssh not
    // installed or not on PATH) returned early and left the password on disk.
    #[tokio::test]
    async fn a_failed_ssh_spawn_leaves_no_password_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_ssh_with_password(
            "atlas-no-such-ssh-binary",
            &[],
            dir.path(),
            "s3cret",
            "ssh-ed25519 AAAA key\n",
        )
        .await
        .unwrap_err();
        assert!(err.starts_with("ssh:"), "{err}");
        assert_eq!(askpass_files(dir.path()), Vec::<String>::new());
    }

    // Same class: the happy path and a failing ssh must also clean up, and the helper must hand the
    // password to ssh (including a single quote) without it ever being on argv.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_helper_serves_the_password_and_is_gone_after_ssh_exits() {
        let dir = tempfile::tempdir().unwrap();
        // A fake "ssh": runs the askpass helper like ssh does and fails when it prints something else.
        let fake = dir.path().join("fake-ssh");
        std::fs::write(
            &fake,
            "#!/bin/sh\n[ \"$($SSH_ASKPASS)\" = \"pa'ss\" ] || { echo 'Permission denied' >&2; exit 255; }\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ok = run_ssh_with_password(fake.to_str().unwrap(), &[], dir.path(), "pa'ss", "k\n")
            .await
            .unwrap();
        assert!(
            ok.status.success(),
            "{}",
            String::from_utf8_lossy(&ok.stderr)
        );
        assert_eq!(askpass_files(dir.path()), Vec::<String>::new());
        let bad = run_ssh_with_password(fake.to_str().unwrap(), &[], dir.path(), "nope", "k\n")
            .await
            .unwrap();
        assert!(!bad.status.success());
        assert_eq!(askpass_files(dir.path()), Vec::<String>::new());
    }

    // The helper holds the password: it must never exist with looser permissions than 0700, not
    // even between the write and the chmod.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_helper_is_never_readable_by_others() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake-ssh");
        // ssh's stdout is discarded by the real code, so the mode goes to stderr. GNU `stat -c %a`
        // first: on Linux `stat -f` is the *filesystem* status and succeeds with the wrong output
        // (Linux CI); BSD/macOS `stat` rejects `-c` and falls back to `-f %Lp`.
        std::fs::write(
            &fake,
            "#!/bin/sh\n(stat -c %a \"$SSH_ASKPASS\" 2>/dev/null || stat -f %Lp \"$SSH_ASKPASS\") >&2\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let out = run_ssh_with_password(fake.to_str().unwrap(), &[], dir.path(), "x", "k\n")
            .await
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "700");
    }
}
