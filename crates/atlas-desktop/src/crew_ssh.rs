//! How "Add a crew server" signs in to a server (#517): which key, creating one, adding it with the account
//! password, the first contact, and what a failure means.
//!
//! Every ssh argv comes from `atlas_core::ssh::SshTarget` (the one place that knows the port, the picked key and
//! the Windows quoting). The password never leaves this process except through [`crate::askpass`]: not on argv, not
//! in the environment, not on disk.

use std::path::Path;
use std::process::Stdio;

use atlas_core::ssh::{self, LocalKey, SshTarget};
use atlas_core::sys::CommandExt as _;
use serde::Serialize;

use crate::askpass::{self, Askpass};

/// What went wrong, as far as the screen needs to tell apart: each kind has its own "what to do next" there.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectErrorKind {
    /// No `ssh` (or `ssh-keygen`) on this computer: on Windows, the optional "OpenSSH Client" feature is off.
    SshMissing,
    /// The server does not accept the key (or, with a passphrase, ssh could not use it without asking).
    KeyRefused,
    /// ssh cannot use the key file: its permissions are too open, or it is not a private key.
    KeyUnusable,
    /// The key file picked does not exist, and there is none to use.
    NoKey,
    /// The account password was wrong.
    PasswordRefused,
    /// The server only allows keys: a password cannot work there.
    PasswordNotAllowed,
    Unreachable,
    Timeout,
    NameNotFound,
    /// The server's host key is not the one this computer remembers (a reinstalled server, or someone in between).
    HostKeyChanged,
    /// sudo on the server asks for a password.
    SudoPassword,
    /// The sudo password was wrong.
    SudoRejected,
    Other,
}

/// The error of the connect commands: `kind` for the screen, `detail` the last line the tool printed (kept under
/// "Details"), `hint` a line the CLI itself wrote for the person (`hint: …` on its stderr) when it knows the fix.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConnectError {
    pub kind: ConnectErrorKind,
    pub detail: String,
    pub hint: Option<String>,
}

impl ConnectError {
    pub fn new(kind: ConnectErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            hint: None,
        }
    }

    /// Read a failed run: the kind from the error it ended with (the CLI's last `error:` line and its `caused by:`
    /// chain; a warning printed earlier, such as a download that timed out and was retried, is not the failure),
    /// the detail from its last line, the hint from the last `hint: ` line.
    pub fn from_lines(lines: &[String]) -> Self {
        let last = |f: &dyn Fn(&str) -> bool| {
            lines
                .iter()
                .rev()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && f(l))
                .map(str::to_owned)
        };
        let detail = last(&|l| !l.starts_with("hint: ")).unwrap_or_else(|| "failed".into());
        let error_from = lines
            .iter()
            .rposition(|l| l.trim_start().starts_with("error:"))
            .unwrap_or(0);
        Self {
            kind: classify(&lines[error_from..].join("\n")),
            detail,
            hint: last(&|l| l.starts_with("hint: "))
                .map(|l| l.trim_start_matches("hint: ").trim().to_owned()),
        }
    }

    pub fn other(detail: impl Into<String>) -> Self {
        Self::new(ConnectErrorKind::Other, detail)
    }
}

/// A program that could not be started: not being installed is the case with its own answer.
fn spawn_error(program: &str, e: &std::io::Error) -> ConnectError {
    let kind = if e.kind() == std::io::ErrorKind::NotFound {
        ConnectErrorKind::SshMissing
    } else {
        ConnectErrorKind::Other
    };
    ConnectError::new(kind, format!("{program}: {e}"))
}

/// The kind of a failure from what ssh and the CLI printed. ssh's own sentences are matched (they are fixed text of
/// OpenSSH, Windows build included); the order matters: a changed host key is also "verification failed", and the
/// CLI wraps ssh's line in its own ("could not connect to … over SSH: …"), so every check looks inside the text.
pub fn classify(text: &str) -> ConnectErrorKind {
    use ConnectErrorKind::*;
    let t = text.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| t.contains(n));
    if has(&[
        "remote host identification has changed",
        "host key verification failed",
    ]) {
        HostKeyChanged
    } else if has(&[
        "unprotected private key file",
        "bad permissions",
        "are too open",
        "invalid format",
    ]) {
        KeyUnusable
    } else if has(&["sudo password rejected"]) {
        SudoRejected
    } else if has(&["asks for a password", "passwordless sudo"]) {
        SudoPassword
    } else if has(&[
        "permission denied",
        "too many authentication failures",
        "no mutual signature algorithm",
    ]) {
        KeyRefused
    } else if has(&[
        "could not resolve hostname",
        "name or service not known",
        "nodename nor servname",
        "no such host is known",
        "temporary failure in name resolution",
    ]) {
        NameNotFound
    } else if has(&["timed out", "timeout", "did not answer"]) {
        Timeout
    } else if has(&[
        "connection refused",
        "no route to host",
        "network is unreachable",
        "connection closed by",
        "connection reset by",
    ]) {
        Unreachable
    } else if has(&["spawning ssh", "program not found"]) {
        SshMissing
    } else {
        Other
    }
}

// ─── which key ───────────────────────────────────────────────────────────────

/// The key ssh will sign in with, for the form to show.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KeyInfo {
    /// The private key file, in the spelling of this OS.
    pub identity: String,
    /// Whether `<identity>.pub` is next to it (without it the public key is derived when it is needed).
    pub has_public: bool,
}

/// What the connect form learns when it opens or the address / key changes.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KeyLookup {
    /// `ssh` runs on this computer. On Windows it is the optional "OpenSSH Client" feature, off on a fresh install.
    pub ssh_found: bool,
    pub key: Option<KeyInfo>,
}

fn show_path(p: &Path) -> String {
    let s = p.display().to_string();
    if cfg!(windows) {
        s.replace('/', "\\")
    } else {
        s
    }
}

fn key_info(k: &LocalKey) -> KeyInfo {
    KeyInfo {
        identity: show_path(&k.identity),
        has_public: k.public.is_some(),
    }
}

/// Whether `ssh` can be started at all (`ssh -V` prints its version and exits).
fn ssh_found() -> bool {
    std::process::Command::new("ssh")
        .no_window()
        .arg("-V")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// The key `ssh` would use for `target` (the address being typed; invalid or empty means any host), or the one the
/// user picked (`identity`). A picked file that is not there is no key: it never falls back to another one.
#[tauri::command]
pub async fn ssh_key_resolve(
    target: Option<String>,
    port: Option<u16>,
    identity: Option<String>,
) -> KeyLookup {
    tokio::task::spawn_blocking(move || {
        let target = target
            .map(|t| t.trim().to_owned())
            .filter(|t| ssh::valid_target(t).is_ok())
            .unwrap_or_else(|| "atlas-key-probe".into());
        let st = SshTarget::new(target, port).with_identity(identity.filter(|i| !i.is_empty()));
        KeyLookup {
            ssh_found: ssh_found(),
            key: ssh::find_key(&st).as_ref().map(key_info),
        }
    })
    .await
    .unwrap_or(KeyLookup {
        ssh_found: false,
        key: None,
    })
}

/// Generate an ed25519 key pair at `~/.ssh/id_ed25519` (no passphrase: Atlas signs in unattended). Fails when the
/// private key file already exists.
async fn create_default_key() -> Result<(), ConnectError> {
    let home = dirs::home_dir().ok_or_else(|| ConnectError::other("no home directory"))?;
    let key = home.join(".ssh/id_ed25519");
    if key.exists() {
        return Err(ConnectError::other(
            "an SSH key already exists at ~/.ssh/id_ed25519",
        ));
    }
    ensure_ssh_dir(&home).map_err(|e| ConnectError::other(e.to_string()))?;
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
        .map_err(|e| spawn_error("ssh-keygen", &e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(ConnectError::other(
            String::from_utf8_lossy(&out.stderr).trim(),
        ))
    }
}

/// `~/.ssh`, owner-only: the place of the keys and `known_hosts`, which a first-time user may not have yet.
fn ensure_ssh_dir(home: &Path) -> std::io::Result<()> {
    let dir = home.join(".ssh");
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Create the default key (the form's "Create SSH key") and say which it is.
#[tauri::command]
pub async fn ssh_keygen() -> Result<KeyInfo, ConnectError> {
    create_default_key().await?;
    let found = tokio::task::spawn_blocking(ssh::find_default_key)
        .await
        .ok()
        .flatten();
    found
        .as_ref()
        .map(key_info)
        .ok_or_else(|| ConnectError::other("the key was created but ssh does not list it"))
}

/// The key for `st`: the one ssh finds, else (no key picked) a new default key.
async fn ensure_key(st: &SshTarget) -> Result<LocalKey, ConnectError> {
    let probe = st.clone();
    let find = move || ssh::find_key(&probe);
    if let Some(k) = tokio::task::spawn_blocking(find.clone())
        .await
        .ok()
        .flatten()
    {
        return Ok(k);
    }
    if let Some(picked) = &st.identity {
        return Err(ConnectError::new(
            ConnectErrorKind::NoKey,
            format!("{} does not exist", show_path(picked)),
        ));
    }
    create_default_key().await?;
    tokio::task::spawn_blocking(find)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| ConnectError::other("the key was created but ssh does not list it"))
}

// ─── first contact ───────────────────────────────────────────────────────────

/// Reach the server once with the key (and trust its host key if this computer never saw it), so what is wrong with
/// the connection is told BEFORE anything is installed, in words of its own: the install's own ssh calls run under
/// `BatchMode` and could not even accept a new server's host key.
#[tauri::command]
pub async fn ssh_connect_check(
    target: String,
    port: Option<u16>,
    identity: Option<String>,
) -> Result<(), ConnectError> {
    let st = SshTarget::new(target, port).with_identity(identity.filter(|i| !i.is_empty()));
    st.valid().map_err(ConnectError::other)?;
    if let Some(home) = dirs::home_dir() {
        let _ = ensure_ssh_dir(&home);
    }
    let child = tokio::process::Command::new("ssh")
        .no_window()
        .args(st.first_contact_argv())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| spawn_error("ssh", &e))?;
    let out = tokio::time::timeout(std::time::Duration::from_secs(40), child.wait_with_output())
        .await
        .map_err(|_| {
            ConnectError::new(
                ConnectErrorKind::Timeout,
                format!("{} did not answer in 40 s", st.target),
            )
        })?
        .map_err(|e| ConnectError::other(e.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let lines: Vec<String> = String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::to_owned)
        .collect();
    Err(ConnectError::from_lines(&lines))
}

// ─── sign in with the account password and add the key ───────────────────────

/// The server side of "add this computer's key": create `~/.ssh`, append the key unless it is already there (a
/// retry must not stack copies), lock the files down. The key is in the script, not on stdin: a Windows client
/// sends scripts as `printf … | bash` (`ssh::windows_safe`), which owns the stdin of that bash.
fn authorize_script(pubkey: &str) -> String {
    let key = pubkey.lines().next().unwrap_or("").trim();
    format!(
        "umask 077; mkdir -p ~/.ssh && chmod 700 ~/.ssh && k={} && \
         {{ grep -qxF \"$k\" ~/.ssh/authorized_keys 2>/dev/null || printf '%s\\n' \"$k\" >> ~/.ssh/authorized_keys; }} && \
         chmod 600 ~/.ssh/authorized_keys",
        ssh::shq(key)
    )
}

/// argv (after `ssh`) of the password login: no key (the point is that there is none on the server yet), only the
/// password methods, a new host key recorded, one attempt (a wrong password is told, not retried). The password is
/// not an input: it is served through [`Askpass`].
fn authorize_argv(st: &SshTarget, script: &str) -> Vec<String> {
    let mut a: Vec<String> = [
        "-o",
        "PubkeyAuthentication=no",
        "-o",
        "PreferredAuthentications=password,keyboard-interactive",
        "-o",
        "PasswordAuthentication=yes",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "NumberOfPasswordPrompts=1",
        "-o",
        "ConnectTimeout=15",
    ]
    .map(String::from)
    .into();
    if let Some(p) = st.port {
        a.extend(["-p".into(), p.to_string()]);
    }
    a.extend([st.target.clone(), "--".into(), ssh::remote_command(script)]);
    a
}

/// Runs `program args` with `password` served to its `SSH_ASKPASS` helper (`askpass_exe`, see [`crate::askpass`]).
/// The password is never on argv, in the environment or on disk.
async fn run_ssh_with_password(
    program: &str,
    args: &[String],
    askpass_exe: &Path,
    password: String,
) -> Result<std::process::Output, ConnectError> {
    // The endpoint exists before ssh starts and ends with this function, whichever way it returns.
    let endpoint =
        Askpass::serve(password).map_err(|e| ConnectError::other(format!("askpass: {e}")))?;
    tokio::process::Command::new(program)
        .no_window()
        .args(args)
        .env("SSH_ASKPASS", askpass_exe)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env(askpass::SOCK_ENV, endpoint.sock())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| spawn_error(program, &e))?
        .wait_with_output()
        .await
        .map_err(|e| ConnectError::other(e.to_string()))
}

/// What a failed password login means. The same "Permission denied" is a wrong password when the server offers the
/// password method, and "this server only takes keys" when it lists only `publickey`.
fn password_failure(stderr: &str) -> ConnectError {
    let lines: Vec<String> = stderr.lines().map(str::to_owned).collect();
    let mut e = ConnectError::from_lines(&lines);
    if e.kind == ConnectErrorKind::KeyRefused {
        let t = stderr.to_lowercase();
        e.kind = if t.contains("(publickey)") && !t.contains("password") {
            ConnectErrorKind::PasswordNotAllowed
        } else {
            ConnectErrorKind::PasswordRefused
        };
    }
    e
}

/// Sign in to `target` with the account password and add this computer's public key to its `authorized_keys`, so
/// that every later connection (the install, hostd's tunnel) uses the key. Creates a key when there is none.
#[tauri::command]
pub async fn ssh_authorize_key(
    target: String,
    port: Option<u16>,
    identity: Option<String>,
    password: String,
) -> Result<(), ConnectError> {
    let st = SshTarget::new(target, port).with_identity(identity.filter(|i| !i.is_empty()));
    st.valid().map_err(ConnectError::other)?;
    if let Some(home) = dirs::home_dir() {
        let _ = ensure_ssh_dir(&home);
    }
    let key = ensure_key(&st).await?;
    let pubkey = tokio::task::spawn_blocking(move || key.public_text())
        .await
        .map_err(|e| ConnectError::other(e.to_string()))?
        .map_err(ConnectError::other)?;
    let exe =
        std::env::current_exe().map_err(|e| ConnectError::other(format!("current_exe: {e}")))?;
    let out = run_ssh_with_password(
        "ssh",
        &authorize_argv(&st, &authorize_script(&pubkey)),
        &exe,
        password,
    )
    .await?;
    if out.status.success() {
        Ok(())
    } else {
        Err(password_failure(&String::from_utf8_lossy(&out.stderr)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::to_owned).collect()
    }

    // OpenSSH's and the CLI's own sentences, as they are printed. Each one is what a first-time user meets and
    // must land on its own "what to do next".
    const CASES: &[(&str, &str, ConnectErrorKind)] = &[
        (
            "key refused by the server",
            "could not connect to ubuntu@203.0.113.7 over SSH: ubuntu@203.0.113.7: Permission denied (publickey).",
            ConnectErrorKind::KeyRefused,
        ),
        (
            "too many keys offered",
            "Received disconnect from 203.0.113.7 port 22:2: Too many authentication failures",
            ConnectErrorKind::KeyRefused,
        ),
        (
            "key file permissions too open (Windows)",
            "@         WARNING: UNPROTECTED PRIVATE KEY FILE!          @\nPermissions for 'C:\\\\Users\\\\me\\\\.ssh\\\\id_work' are too open.\nbad permissions: ignore key: C:\\\\Users\\\\me\\\\.ssh\\\\id_work",
            ConnectErrorKind::KeyUnusable,
        ),
        (
            "host unreachable",
            "could not connect to ubuntu@10.0.0.9 over SSH: ssh: connect to host 10.0.0.9 port 22: No route to host",
            ConnectErrorKind::Unreachable,
        ),
        (
            "port closed",
            "ssh: connect to host 127.0.0.1 port 1: Connection refused",
            ConnectErrorKind::Unreachable,
        ),
        (
            "timeout",
            "ssh: connect to host 203.0.113.7 port 22: Operation timed out",
            ConnectErrorKind::Timeout,
        ),
        (
            "name does not resolve",
            "ssh: Could not resolve hostname my-server.example: Name or service not known",
            ConnectErrorKind::NameNotFound,
        ),
        (
            "host key changed",
            "@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\nOffending ED25519 key in /home/me/.ssh/known_hosts:4\nHost key verification failed.",
            ConnectErrorKind::HostKeyChanged,
        ),
        (
            "sudo wants a password",
            "sudo on ubuntu@203.0.113.7 asks for a password — grant passwordless sudo to this user, or run it from a terminal: atlas crew install ubuntu@203.0.113.7",
            ConnectErrorKind::SudoPassword,
        ),
        (
            "sudo password wrong",
            "sudo password rejected — check your password and try again",
            ConnectErrorKind::SudoRejected,
        ),
        (
            "ssh not installed (Windows without the OpenSSH Client)",
            "spawning ssh\n  caused by: The system cannot find the file specified. (os error 2)",
            ConnectErrorKind::SshMissing,
        ),
        (
            "anything else",
            "writing the service failed: Failed to enable unit: Unit atlas-crew.service is masked",
            ConnectErrorKind::Other,
        ),
    ];

    #[test]
    fn every_known_failure_lands_on_its_own_kind() {
        for (name, text, want) in CASES {
            assert_eq!(classify(text), *want, "{name}: {text}");
        }
    }

    // The CLI prints its error and then a `caused by:` chain: the kind comes from all of it, the detail is the
    // last line, and a `hint:` line (the masked-unit fix) is carried apart from the detail.
    #[test]
    fn a_failed_run_keeps_its_last_line_and_the_cli_hint() {
        let e = ConnectError::from_lines(&lines(
            "▸ writing systemd service\nhint: run `sudo systemctl unmask atlas-crew.service` on the server, then retry\nerror: writing the service failed: Unit atlas-crew.service is masked\n",
        ));
        assert_eq!(e.kind, ConnectErrorKind::Other);
        assert_eq!(
            e.detail,
            "error: writing the service failed: Unit atlas-crew.service is masked"
        );
        assert_eq!(
            e.hint.as_deref(),
            Some("run `sudo systemctl unmask atlas-crew.service` on the server, then retry")
        );
        let none = ConnectError::from_lines(&lines("error: boom\n"));
        assert_eq!(none.hint, None);
    }

    // The install prints warnings on the way (the host's download timed out, so it is downloaded here): they are
    // not the failure. A later error that is none of the known ones must not be read as a timeout because of them.
    #[test]
    fn an_earlier_warning_does_not_decide_the_kind_of_the_final_error() {
        let e = ConnectError::from_lines(&lines(
            "◆ the host could not download atlas 1.5.0 (curl: (28) Connection timed out); downloading it here\nerror: writing the service failed: Unit atlas-crew.service is masked",
        ));
        assert_eq!(e.kind, ConnectErrorKind::Other);
        let real = ConnectError::from_lines(&lines(
            "◆ a warning\nerror: could not connect to u@h over SSH: ssh: connect to host h port 22: Operation timed out",
        ));
        assert_eq!(real.kind, ConnectErrorKind::Timeout);
    }

    // Same words, two meanings: "Permission denied" after a PASSWORD login is a wrong password, unless the server
    // lists only `publickey`, which means a password can never work there.
    #[test]
    fn a_denied_password_login_is_told_apart_from_a_server_that_takes_only_keys() {
        let wrong = password_failure("ubuntu@h: Permission denied (publickey,password).");
        assert_eq!(wrong.kind, ConnectErrorKind::PasswordRefused);
        let keys_only = password_failure("ubuntu@h: Permission denied (publickey).");
        assert_eq!(keys_only.kind, ConnectErrorKind::PasswordNotAllowed);
        let down = password_failure("ssh: connect to host h port 22: Connection refused");
        assert_eq!(down.kind, ConnectErrorKind::Unreachable);
    }

    // Against the real `ssh` of this machine, not only strings written from memory.
    #[tokio::test]
    async fn the_real_ssh_words_for_a_closed_port_and_an_unknown_name_are_classified() {
        let run = |args: &[&str]| {
            let o = std::process::Command::new("ssh")
                .args(args)
                .output()
                .expect("ssh");
            assert!(!o.status.success());
            String::from_utf8_lossy(&o.stderr).into_owned()
        };
        let closed = run(&[
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-p",
            "1",
            "127.0.0.1",
            "true",
        ]);
        assert_eq!(classify(&closed), ConnectErrorKind::Unreachable, "{closed}");
        let unknown = run(&[
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "nobody@atlas-no-such-host.invalid",
            "true",
        ]);
        assert_eq!(
            classify(&unknown),
            ConnectErrorKind::NameNotFound,
            "{unknown}"
        );
    }

    // The payload the screen reads is the serialization of these values: written from the real producer
    // (`UPDATE_FIXTURES=1 cargo test -p atlas-desktop crew_ssh`), read by `connectError.test.ts`.
    #[test]
    fn connect_errors_fixture_is_the_real_serialization() {
        let mut all: Vec<serde_json::Value> = CASES
            .iter()
            .map(|(name, text, _)| {
                serde_json::json!({ "name": name, "error": ConnectError::from_lines(&lines(text)) })
            })
            .collect();
        all.push(serde_json::json!({
            "name": "masked unit with the CLI's hint",
            "error": ConnectError::from_lines(&lines("hint: run `sudo systemctl unmask atlas-crew.service` on the server, then retry\nerror: writing the service failed: Unit atlas-crew.service is masked")),
        }));
        all.push(serde_json::json!({
            "name": "password refused",
            "error": password_failure("ubuntu@h: Permission denied (publickey,password)."),
        }));
        all.push(serde_json::json!({
            "name": "server takes only keys",
            "error": password_failure("ubuntu@h: Permission denied (publickey)."),
        }));
        all.push(serde_json::json!({
            "name": "picked key file is gone",
            "error": ConnectError::new(ConnectErrorKind::NoKey, "/h/.ssh/id_work does not exist"),
        }));
        let fresh = serde_json::Value::Array(all);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/crew-connect-errors.json");
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            // Tab-indented, as `prettier --check src` (useTabs) wants the committed file.
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(&fresh, &mut ser).unwrap();
            std::fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("fixture missing: run with UPDATE_FIXTURES=1"),
        )
        .unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }

    // The key lookup the form reads, written from the real serializer (`UPDATE_FIXTURES=1 cargo test -p
    // atlas-desktop crew_ssh`) and read by the connect tests: a key found, no key, and no ssh at all.
    #[test]
    fn key_lookup_fixture_is_the_real_serialization() {
        let fresh = serde_json::json!({
            "found": KeyLookup {
                ssh_found: true,
                key: Some(KeyInfo {
                    identity: "/Users/me/.ssh/id_ed25519".into(),
                    has_public: true,
                }),
            },
            "none": KeyLookup { ssh_found: true, key: None },
            "no_ssh": KeyLookup { ssh_found: false, key: None },
        });
        assert_eq!(
            fresh["found"]["key"]["identity"],
            "/Users/me/.ssh/id_ed25519"
        );
        assert_eq!(fresh["found"]["key"]["has_public"], true);
        assert_eq!(fresh["no_ssh"]["ssh_found"], false);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/crew-key-lookup.json");
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(&fresh, &mut ser).unwrap();
            std::fs::write(&path, String::from_utf8(buf).unwrap() + "\n").unwrap();
        }
        let committed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("fixture missing: run with UPDATE_FIXTURES=1"),
        )
        .unwrap();
        assert_eq!(
            committed, fresh,
            "stale fixture: re-run with UPDATE_FIXTURES=1"
        );
    }

    // The password login: no key, password methods only, the port before the target, the host key recorded, and
    // the password is not an input of the argv at all.
    #[test]
    fn the_password_login_argv_has_no_key_and_the_port_before_the_target() {
        let st = SshTarget::new("ubuntu@h", Some(2222)).with_identity(Some("/k/id"));
        let a = authorize_argv(&st, "true");
        assert!(
            a.windows(2)
                .any(|w| w[0] == "-o" && w[1] == "PubkeyAuthentication=no")
        );
        assert!(
            a.windows(2)
                .any(|w| w[0] == "-o" && w[1] == "StrictHostKeyChecking=accept-new")
        );
        assert!(!a.contains(&"-i".to_string()), "{a:?}");
        let p = a.iter().position(|x| x == "-p").unwrap();
        assert_eq!(a[p + 1], "2222");
        assert!(p < a.iter().position(|x| x == "ubuntu@h").unwrap());
        assert_eq!(a[a.len() - 2], "--");
    }

    // The key goes in the script and is quoted, so a comment with a quote cannot break out of it, and the script
    // has no stdin to read (a Windows client sends it through a pipe into bash).
    #[cfg(unix)]
    #[test]
    fn authorizing_adds_the_key_once_with_tight_permissions() {
        let home = tempfile::tempdir().unwrap();
        let key = "ssh-ed25519 AAAAC3Nza me@laptop's key";
        let script = authorize_script(&format!("{key}\n"));
        for _ in 0..2 {
            let st = std::process::Command::new("sh")
                .arg("-c")
                .arg(ssh::remote_command(&script))
                .env("HOME", home.path())
                .stdin(Stdio::null())
                .status()
                .unwrap();
            assert!(st.success());
        }
        let f = home.path().join(".ssh/authorized_keys");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), format!("{key}\n"));
        use std::os::unix::fs::PermissionsExt as _;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!((mode(&f), mode(&home.path().join(".ssh"))), (0o600, 0o700));
    }

    // A fake `ssh` that asks for the password the way ssh does (it runs `$SSH_ASKPASS` with the prompt) and records
    // its argv and environment. The password reaches it through the endpoint; it appears in neither the argv nor the
    // environment, the helper is the program named by SSH_ASKPASS (never a script holding the password), and
    // nothing is left on disk when the call ends.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_password_reaches_ssh_but_never_argv_environment_or_disk() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let exec = |name: &str, body: &str| {
            let p = dir.path().join(name);
            std::fs::write(&p, body).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        // Stands in for the app's executable: talks to the endpoint like `askpass::helper` does (bytes until close).
        let helper = exec(
            "helper",
            "#!/bin/sh\nexec perl -e 'use IO::Socket::UNIX; my $s = IO::Socket::UNIX->new(Peer => $ENV{ATLAS_ASKPASS_SOCK}) or exit 1; print while <$s>;'\n",
        );
        let record = dir.path().join("record");
        let fake = exec(
            "fake-ssh",
            &format!(
                "#!/bin/sh\n{{ echo \"ARGV: $*\"; env; echo \"ASKPASS_IS: $SSH_ASKPASS\"; echo \"GOT: $($SSH_ASKPASS \"ubuntu@h's password: \")\"; ls \"$(dirname \"$ATLAS_ASKPASS_SOCK\")\"; }} > {}\n",
                record.display()
            ),
        );
        let pw = "pa'ss w0rd";
        let out = run_ssh_with_password(
            fake.to_str().unwrap(),
            &["-o".into(), "BatchMode=no".into(), "ubuntu@h".into()],
            &helper,
            pw.to_string(),
        )
        .await
        .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rec = std::fs::read_to_string(&record).unwrap();
        assert!(rec.contains(&format!("GOT: {pw}")), "{rec}");
        for line in rec.lines().filter(|l| !l.starts_with("GOT: ")) {
            assert!(!line.contains(pw), "the password leaked into: {line}");
        }
        assert!(
            rec.contains(&format!("ASKPASS_IS: {}", helper.display())),
            "{rec}"
        );
        assert!(rec.contains("SSH_ASKPASS_REQUIRE=force"), "{rec}");
        // The endpoint's directory held only the socket and is gone.
        let sock_dir = rec
            .lines()
            .find_map(|l| l.strip_prefix("ATLAS_ASKPASS_SOCK="))
            .unwrap();
        assert!(!Path::new(sock_dir).exists(), "{sock_dir}");
        // The temp dir of this test holds the fake files and no copy of the password.
        for e in std::fs::read_dir(dir.path()).unwrap() {
            let body = std::fs::read(e.unwrap().path()).unwrap();
            assert!(
                !String::from_utf8_lossy(&body).contains("w0rd") || body == rec.as_bytes(),
                "password on disk"
            );
        }
    }

    // A failed spawn (no ssh installed) names `ssh`, and the endpoint is not left behind.
    #[tokio::test]
    async fn a_missing_ssh_is_its_own_error() {
        let e = run_ssh_with_password(
            "atlas-no-such-ssh-binary",
            &[],
            Path::new("/nonexistent"),
            "s3cret".into(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.kind, ConnectErrorKind::SshMissing);
        assert!(e.detail.starts_with("atlas-no-such-ssh-binary:"), "{e:?}");
    }
}
