//! Signing the engines in on a crew server, from a phone: `crew_engine_login` and `crew_engine_status`.
//!
//! A crewmate's engine (claude, codex, omp, opencode) is the crew user's own CLI login. When it expires the
//! crew server cannot work, and with no PC on nobody could sign it in again. [`login`] opens a terminal as the
//! crew user that runs ONLY the engine's login command (the phone attaches with its terminal screen), and
//! [`status`] says per engine whether it is signed in. A task that fails with the engine's own auth error
//! flips it to `needs_signin` at once ([`note_task_failure`]) and a `crew_engine_auth` push tells the clients.
//!
//! [`ENGINES`] is the one table: the allowlist (the security boundary, a phone with the `Act` scope can start
//! nothing else), the login and status argv, and the strings each CLI prints when its sign-in is gone.
//! Sources (read 2026-11-14):
//! - claude: `claude auth login`, `claude auth status` (exit 1 when logged out): code.claude.com/docs/en/cli-reference;
//!   errors: code.claude.com/docs/en/errors ("Not logged in · Please run /login", "Login expired · Please run /login",
//!   "OAuth token has expired", "Invalid API key").
//! - codex: `codex login --device-auth` (no browser on the server: a code to enter on the phone) and
//!   `codex login status` (`Not logged in` on stderr): openai/codex codex-rs/cli/src/main.rs and login.rs;
//!   errors: codex-rs/login/src/auth/manager.rs ("Your access token could not be refreshed ... Please log out and
//!   sign in again.").
//! - omp: `omp login` (the terminal counterpart of `/login`, a provider picker) and `omp auth export`
//!   (credential rows): omp.sh/docs, can1357/oh-my-pi docs/providers.md; errors: packages/ai `NoApiKeyError`
//!   ("No API key for provider: X") and its auth-failure pattern.
//! - opencode: `opencode auth login` and `opencode auth list`: opencode.ai/docs/cli/providers; errors:
//!   `ProviderAuthError` (packages/schema/src/v1/session.ts).

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{Value, json};

use crate::protocol::{Mode, PtyCreate, Scope};
use crate::pty::Session;
use crate::server::Hub;

use atlas_core::sys::CommandExt;

use super::proc::run_bounded;

/// One engine CLI the crew can sign in.
pub(crate) struct Engine {
    pub id: &'static str,
    /// The terminal command of the sign-in (the whole argv, never extended by a client).
    pub login: &'static [&'static str],
    /// Exit 0 = signed in. omp and opencode have no such exit code: see [`signed_in_output`].
    pub status: &'static [&'static str],
    /// Case-insensitive fragments of what the engine says when its sign-in is gone.
    signatures: &'static [&'static str],
}

pub(crate) const ENGINES: &[Engine] = &[
    Engine {
        id: "claude",
        login: &["claude", "auth", "login"],
        status: &["claude", "auth", "status"],
        signatures: &[
            "please run /login",
            "not logged in",
            "login expired",
            "oauth token has expired",
            "oauth token revoked",
            "invalid api key",
            "invalid authentication credentials",
        ],
    },
    Engine {
        id: "codex",
        login: &["codex", "login", "--device-auth"],
        status: &["codex", "login", "status"],
        signatures: &[
            "please log out and sign in again",
            "please sign in again",
            "not logged in",
            "refresh token",
        ],
    },
    Engine {
        id: "omp",
        login: &["omp", "login"],
        status: &["omp", "auth", "export"],
        signatures: &[
            "no api key for provider",
            "no api key available",
            "run /login",
        ],
    },
    Engine {
        id: "opencode",
        login: &["opencode", "auth", "login"],
        status: &["opencode", "auth", "list"],
        signatures: &["providerautherror", "unauthorized", "api key is missing"],
    },
];

fn find(id: &str) -> Option<&'static Engine> {
    ENGINES.iter().find(|e| e.id == id)
}

/// Whether `text` (a failed task's error) is `engine` saying its sign-in is gone. Only that engine's own
/// signatures count: another failure never flips a sign-in, and one engine's text says nothing of another's.
pub(crate) fn is_auth_error(engine: &str, text: &str) -> bool {
    let Some(e) = find(engine) else { return false };
    let low = text.to_lowercase();
    // The engine module prefixes its failures with its own name (`claude: agent error: ...`).
    if let Some((who, _)) = low.split_once(':')
        && ENGINES.iter().any(|o| o.id == who)
        && who != engine
    {
        return false;
    }
    e.signatures.iter().any(|s| low.contains(s))
}

/// Whether the status command's output says signed in, for the engines whose exit code does not.
fn signed_in_output(engine: &str, stdout: &str) -> bool {
    match engine {
        // `omp auth export` prints the stored credential rows.
        "omp" => stdout.contains("\"providerType\""),
        // `opencode auth list` ends with "N credentials" (0 when none).
        "opencode" => {
            let t = stdout.trim();
            !t.is_empty() && !t.contains("0 credentials")
        }
        _ => true,
    }
}

/// What the crew knows of each engine's sign-in. `reason` is the failure that flipped it, for the settings row.
#[derive(Default)]
pub(crate) struct EngineAuth {
    flipped: parking_lot::Mutex<BTreeMap<String, String>>,
}

impl EngineAuth {
    fn flip(&self, engine: &str, reason: &str) -> bool {
        self.flipped
            .lock()
            .insert(engine.to_string(), reason.chars().take(300).collect())
            .is_none()
    }

    fn clear(&self, engine: &str) {
        self.flipped.lock().remove(engine);
    }

    fn reason(&self, engine: &str) -> Option<String> {
        self.flipped.lock().get(engine).cloned()
    }
}

/// The PATH the engine CLIs are found on for the crew user: where their installers put them. The same entries
/// as the desktop's agent check (`atlas-desktop/src/crew.rs check_script`) and the crew service unit.
const USER_BINS: [&str; 4] = [".bun/bin", ".local/bin", ".opencode/bin", ".npm-global/bin"];

/// `PATH` for an engine command: the user's bin directories first.
fn engine_path() -> std::ffi::OsString {
    #[cfg(all(test, unix))]
    if let Some(dir) = TEST_PATH.lock().clone() {
        // Only the fakes (and the system shell tools they use): a developer's real `codex` must not answer.
        let mut p = std::ffi::OsString::from(dir);
        p.push(":/usr/bin:/bin");
        return p;
    }
    let mut dirs: Vec<std::path::PathBuf> = vec![];
    let home = atlas_core::paths::home_dir();
    dirs.extend(USER_BINS.iter().map(|d| home.join(d)));
    dirs.extend(std::env::split_paths(&atlas_core::login_env::path_var()));
    std::env::join_paths(dirs).unwrap_or_else(|_| atlas_core::login_env::path_var())
}

#[cfg(all(test, unix))]
static TEST_PATH: parking_lot::Mutex<Option<std::path::PathBuf>> = parking_lot::Mutex::new(None);

/// Test-only: the directory that is the whole PATH of the engine CLIs (fake engines).
#[cfg(all(test, unix))]
pub(crate) fn set_test_path(dir: Option<std::path::PathBuf>) {
    *TEST_PATH.lock() = dir;
}

/// `CrewEngineLogin`: a terminal that runs only `engine`'s login command, as the crew user (hostd's own user).
/// The reply is `{engine, pty}`; the phone attaches with the terminal screen and types the code or the answers
/// itself. Audited as `engine_login_started`: who asked and for which engine, never what is typed.
pub(crate) async fn login(hub: &Hub, conn_id: u64, engine: &str) -> Result<Value, String> {
    let e = find(engine).ok_or_else(|| format!("unknown engine {engine:?}"))?;
    let mut env = BTreeMap::new();
    env.insert(
        "PATH".to_string(),
        engine_path().to_string_lossy().into_owned(),
    );
    // No pane key: this is a person's terminal, not an agent pane, so no hook reports it as a crewmate.
    let req = PtyCreate {
        cwd: atlas_core::paths::home_dir().to_string_lossy().into_owned(),
        argv: e.login.iter().map(|s| s.to_string()).collect(),
        env,
        cols: 80,
        rows: 24,
        scope: Scope::default(),
        title: Some(format!("sign in: {}", e.id)),
        mode: Mode::Pty,
    };
    let scroll = crate::paths::scrollback_dir(&hub.dir);
    let session = tokio::task::spawn_blocking(move || Session::spawn(req, &scroll))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;
    let pty = session.id.clone();
    hub.sessions.lock().insert(pty.clone(), session.clone());
    reap_when_done(hub, &session);
    // Whether the sign-in worked is not read from the terminal: the next status read (the settings screen runs
    // one when it opens) asks the engine itself.
    hub.crew.audit.record(
        None,
        "engine_login_started",
        json!({"engine": e.id, "by": hub.origin_of(conn_id)}),
    );
    Ok(json!({"engine": e.id, "pty": pty}))
}

/// How long a finished sign-in terminal stays attachable: the phone still shows what the CLI said last
/// ("Successfully logged in", or why not) and can re-open it. Nobody has to `pty_kill` it.
const REAP_AFTER_EXIT: Duration = Duration::from_secs(10 * 60);

/// Remove the login terminal from the hub [`REAP_AFTER_EXIT`] after its program ends. The terminal screen only
/// detaches, so without this every sign-in (one is started per tap) would stay in `pty_list` until a restart.
/// Listens like a client that wants no output (`server::idle_agent_on_exit`).
fn reap_when_done(hub: &Hub, s: &std::sync::Arc<Session>) {
    let (tx, mut rx) = crate::pty::out_channel();
    s.attach(hub.fresh_conn_id(), tx, false, false);
    let (id, hub_ref) = (
        s.id.clone(),
        hub.crew.hub_arc().map(|h| std::sync::Arc::downgrade(&h)),
    );
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if line.contains(r#""ev":"pty_exit""#) {
                break;
            }
        }
        tokio::time::sleep(reap_after_exit()).await;
        if let Some(hub) = hub_ref.and_then(|h| h.upgrade())
            && let Some(s) = hub.sessions.lock().remove(&id)
        {
            s.remove_files();
        }
    });
}

fn reap_after_exit() -> Duration {
    #[cfg(all(test, unix))]
    if let Some(d) = *TEST_REAP.lock() {
        return d;
    }
    REAP_AFTER_EXIT
}

#[cfg(all(test, unix))]
static TEST_REAP: parking_lot::Mutex<Option<Duration>> = parking_lot::Mutex::new(None);

/// Test-only: a shortened [`REAP_AFTER_EXIT`].
#[cfg(all(test, unix))]
pub(crate) fn set_test_reap(d: Option<Duration>) {
    *TEST_REAP.lock() = d;
}

/// One engine's row of `crew_engine_status`.
async fn probe(hub: &Hub, e: &'static Engine, refresh: bool) -> Value {
    let flipped = hub.crew.engine_auth.reason(e.id);
    if !refresh {
        if let Some(reason) = flipped {
            return json!({"engine": e.id, "state": "needs_signin", "reason": reason});
        }
        if let Some(state) = hub.crew.engine_auth_seen.lock().get(e.id) {
            return json!({"engine": e.id, "state": state});
        }
    }
    let path = engine_path();
    // Not on the engine PATH: nothing to sign in. Resolved here, not read from an OS error text (which differs
    // by platform and language), the way the terminal layer resolves a program.
    let Some(program) = atlas_core::paths::which_in(e.status[0], &path) else {
        return remember(hub, e, "not_installed", flipped);
    };
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(&e.status[1..]).env("PATH", &path);
    cmd.no_window();
    let state = match run_bounded(&mut cmd, e.id, Duration::from_secs(20)).await {
        // Did not start or did not answer in time: the sign-in is not known either way.
        Err(_) => "unknown",
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if out.status.success() && signed_in_output(e.id, &stdout) {
                hub.crew.engine_auth.clear(e.id);
                "signed_in"
            } else {
                "needs_signin"
            }
        }
    };
    remember(hub, e, state, flipped)
}

/// Keep `state` as the last answer for `e` and build its row (`reason` only while a task's failure holds it).
fn remember(hub: &Hub, e: &Engine, state: &str, flipped: Option<String>) -> Value {
    hub.crew
        .engine_auth_seen
        .lock()
        .insert(e.id.to_string(), state.to_string());
    match (state, flipped) {
        ("needs_signin", Some(reason)) => json!({"engine": e.id, "state": state, "reason": reason}),
        _ => json!({"engine": e.id, "state": state}),
    }
}

/// `CrewEngineStatus`: `{engines: [{engine, state, reason?}]}` with `state` one of `signed_in`, `needs_signin`,
/// `not_installed`, `unknown` (the check did not answer). `refresh` runs each engine's status command (bounded);
/// without it the last answer and what failed tasks reported are returned at once.
pub(crate) async fn status(hub: &Hub, refresh: bool) -> Result<Value, String> {
    // Run together: a slow `codex login status` does not hold the other probes.
    let mut probes = ENGINES.iter().map(|e| probe(hub, e, refresh));
    let rows = match (probes.next(), probes.next(), probes.next(), probes.next()) {
        (Some(a), Some(b), Some(c), Some(d)) => {
            let (a, b, c, d) = tokio::join!(a, b, c, d);
            vec![a, b, c, d]
        }
        _ => unreachable!("ENGINES has four entries (test engines_table_has_four_entries)"),
    };
    Ok(json!({"engines": rows}))
}

/// A task of a crewmate on `engine` failed with `error`: when that is the engine's auth error its sign-in is
/// gone. Flips it once (the first time pushes `crew_engine_auth`, so a phone's settings row updates).
pub(crate) fn note_task_failure(hub: &Hub, engine: &str, error: &str) {
    if !is_auth_error(engine, error) {
        return;
    }
    if hub.crew.engine_auth.flip(engine, error) {
        hub.crew
            .engine_auth_seen
            .lock()
            .insert(engine.to_string(), "needs_signin".to_string());
        hub.broadcast(
            json!({"ev": "crew_engine_auth", "engine": engine, "state": "needs_signin"})
                .to_string(),
        );
    }
}

#[cfg(test)]
#[test]
fn engines_table_has_four_entries() {
    assert_eq!(ENGINES.len(), 4, "status() joins exactly these four probes");
}
