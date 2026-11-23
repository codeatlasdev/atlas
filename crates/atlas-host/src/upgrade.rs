//! `upgrade {exe}`: the old daemon's side, and `--takeover`: the successor's side.
//! See [`crate::handoff`] for what travels and why.
//!
//! ```text
//! old                                   new (exe hostd --takeover <sock>)
//! bind <dir>/hostd.handoff.<pid>.sock
//! spawn new ─────────────────────────▶  connect
//! freeze every session (reader stops)
//! sendmsg(state JSON + master fds) ──▶  recvmsg, parse
//!                                  ◀──  'R' (received, will serve)
//! reply ok, stop serving, exit          wait for pid lock → bind hostd.sock → adopt
//! ```
//!
//! Until `R` arrives the old daemon is still authoritative: on any failure (spawn,
//! timeout, bad bundle) it thaws the sessions and keeps serving, and the caller gets
//! an error. After `R` there is no way back, so the old daemon only stops serving.

#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
#[cfg(unix)]
use std::process::Stdio;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use anyhow::{Context, Result, anyhow};
use atlas_core::progress::Coded;
use atlas_i18n::t_in;
use serde_json::{Value, json};

#[cfg(unix)]
use crate::handoff::{
    Bundle, SessionState, encode_chunks, handoff_socket, recv_with_fds, send_with_fds,
};
#[cfg(unix)]
use crate::paths;
use crate::server::Hub;

#[cfg(unix)]
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(unix)]
const ACK_TIMEOUT: Duration = Duration::from_secs(20);

/// Windows: a pty cannot move to another process, so there is no live upgrade (the daemon does
/// not advertise the `upgrade` capability and clients restart it when no terminal runs).
#[cfg(windows)]
pub(crate) async fn run(_hub: &Hub, _exe: &str) -> Result<Value, String> {
    let _ = Path::new("");
    Err("live upgrade is not supported on Windows".into())
}

/// Never constructed on Windows: `serve_with` refuses `--takeover` there.
#[cfg(windows)]
pub(crate) struct Takeover {
    pub lock: Option<std::fs::File>,
}

#[cfg(windows)]
impl Takeover {
    pub fn confirm(self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Old side of the handoff, for the `upgrade` command.
///
/// One daemon is handed over once, however many callers ask (`atlas update` and the app both
/// notice an old daemon at the same time): calls queue on `hub.upgraded`, and one that finds
/// the handoff done answers `handed_off: 0` instead of starting a second one. A handoff that
/// failed leaves the daemon authoritative, so the next caller may try again.
#[cfg(unix)]
pub(crate) async fn run(hub: &Hub, exe: &str) -> Result<Value, String> {
    let exe = Path::new(exe);
    if !exe.is_file() {
        return Err(format!("{} is not a file", exe.display()));
    }
    let mut done = hub.upgraded.lock().await;
    if *done {
        return Ok(json!({"handed_off": 0, "already": true}));
    }
    let r = run_inner(hub, exe).await.map_err(|e| format!("{e:#}"));
    *done = r.is_ok();
    r
}

#[cfg(unix)]
async fn run_inner(hub: &Hub, exe: &Path) -> Result<Value> {
    let sock = handoff_socket(&hub.dir, std::process::id());
    let _ = std::fs::remove_file(&sock);
    // i18n-ignore: context of an error chain (the handoff socket being bound), read in logs and in the detail of `hostd.upgrade_failed`
    let listener = UnixListener::bind(&sock).with_context(|| format!("bind {}", sock.display()))?;
    std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    let _cleanup = scopeguard(sock.clone());

    let log = paths::open_log(&hub.dir)?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("hostd")
        .arg("--dir")
        .arg(&hub.dir)
        .arg("--takeover")
        .arg(&sock)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // The successor must outlive us and the app.
    crate::exec::new_session(&mut cmd);
    let mut child = cmd
        .spawn()
        // i18n-ignore: context of an error chain (the program being started), read in logs and in the detail of `hostd.upgrade_failed`
        .with_context(|| format!("spawn {}", exe.display()))?;
    let child_pid = child.id();

    let (stream, child) = tokio::task::spawn_blocking(move || {
        let s = accept_timeout(&listener, &mut child, ACCEPT_TIMEOUT);
        (s, child)
    })
    .await?;
    // i18n-ignore: the cause of a failed hand-over: the reply of `upgrade`, shown as the detail of the coded `hostd.upgrade_failed` and in logs
    let stream = stream.context("takeover failed; the old host keeps running")?;
    // Reap it when it ends (it outlives us by design after the handoff).
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });

    let sessions = hub.sessions_snapshot();
    let mut states = Vec::new();
    let mut fds = Vec::new();
    let mut frozen = Vec::new();
    for s in &sessions {
        if let Some((mut st, fd)) = s.freeze().await {
            if let Some(fd) = fd {
                st.fd = Some(fds.len());
                fds.push(fd);
                frozen.push(s.clone());
            }
            states.push(st);
        }
    }
    let bundle = Bundle {
        from_version: env!("CARGO_PKG_VERSION").into(),
        sessions: states,
        lock: true,
        agents: hub.agents.snapshot(),
    };
    let lock_fd = hub.lock_fd();
    let moved = frozen.len();
    let res =
        tokio::task::spawn_blocking(move || send_and_ack(stream, &bundle, &fds, lock_fd)).await?;
    if let Err(e) = res {
        // Successor never confirmed: we stay authoritative.
        for s in &frozen {
            s.thaw();
        }
        // SAFETY: the child is ours (spawned above) and never took over; stop it.
        unsafe { libc::kill(child_pid as libc::pid_t, libc::SIGTERM) };
        // i18n-ignore: the cause of a failed hand-over: the reply of `upgrade`, shown as the detail of the coded `hostd.upgrade_failed` and in logs
        return Err(e.context("takeover failed; the old host keeps running"));
    }
    // Let the reply reach the client, then stop serving without killing the ptys.
    hub.handed_off_later(Duration::from_millis(300));
    Ok(json!({"handed_off": moved, "pid": child_pid}))
}

#[cfg(unix)]
fn scopeguard(p: std::path::PathBuf) -> impl Drop {
    struct G(std::path::PathBuf);
    impl Drop for G {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    G(p)
}

/// Wait for the successor to connect; fail at once if it exits first (bad binary).
#[cfg(unix)]
fn accept_timeout(
    l: &UnixListener,
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<UnixStream> {
    l.set_nonblocking(true)?;
    let t0 = Instant::now();
    loop {
        match l.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false)?;
                return Ok(s);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(st) = child.try_wait()? {
                    // i18n-ignore: the cause of a failed hand-over: the reply of `upgrade`, shown as the detail of the coded `hostd.upgrade_failed` and in logs
                    return Err(anyhow!("the new hostd exited early ({st})"));
                }
                if t0.elapsed() > timeout {
                    // i18n-ignore: the cause of a failed hand-over: the reply of `upgrade`, shown as the detail of the coded `hostd.upgrade_failed` and in logs
                    return Err(anyhow!("the new hostd did not connect in time"));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(unix)]
fn send_and_ack(
    mut s: UnixStream,
    bundle: &Bundle,
    fds: &[std::os::fd::RawFd],
    lock_fd: std::os::fd::RawFd,
) -> Result<()> {
    for (payload, chunk_fds) in encode_chunks(bundle, fds, lock_fd) {
        send_with_fds(s.as_raw_fd(), &payload, &chunk_fds)?;
    }
    s.set_read_timeout(Some(ACK_TIMEOUT))?;
    let mut ack = [0u8; 1];
    s.read_exact(&mut ack)
        // i18n-ignore: the cause of a failed hand-over: the reply of `upgrade`, shown as the detail of the coded `hostd.upgrade_failed` and in logs
        .context("waiting for the new hostd")?;
    if ack[0] != b'R' {
        // i18n-ignore: the cause of a failed hand-over: the reply of `upgrade`, shown as the detail of the coded `hostd.upgrade_failed` and in logs
        return Err(anyhow!("the new hostd rejected the handoff"));
    }
    Ok(())
}

/// What the successor got: the sessions, the already-locked pid file, and the open
/// handoff stream, over which it confirms with `R` once it serves.
#[cfg(unix)]
pub(crate) struct Takeover {
    pub sessions: Vec<(SessionState, Option<OwnedFd>)>,
    pub lock: Option<std::fs::File>,
    /// The predecessor's agent state, for `AgentHub::restore`.
    pub agents: serde_json::Value,
    stream: UnixStream,
}

#[cfg(unix)]
impl Takeover {
    /// Tell the old daemon we serve now (it may exit). Until this, it stays authoritative
    /// and takes the ptys back if we die.
    pub fn confirm(mut self) -> Result<()> {
        self.stream.write_all(b"R")?;
        Ok(())
    }
}

/// Successor side: connect to the old daemon and read every chunk.
#[cfg(unix)]
pub(crate) async fn receive(sock: &Path) -> Result<Takeover> {
    let sock = sock.to_path_buf();
    tokio::task::spawn_blocking(move || receive_blocking(&sock)).await?
}

#[cfg(unix)]
fn receive_blocking(sock: &Path) -> Result<Takeover> {
    // i18n-ignore: context of an error chain (the handoff socket), written to hostd.log by the successor
    let s = UnixStream::connect(sock).with_context(|| format!("connect {}", sock.display()))?;
    s.set_read_timeout(Some(ACK_TIMEOUT))?;
    let mut sessions = Vec::new();
    let lock;
    let agents;
    let mut pending: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut chunk_fds: Vec<Option<OwnedFd>> = Vec::new();
    'recv: loop {
        // i18n-ignore: the successor's side of the hand-over, written to hostd.log
        let (n, fds) = recv_with_fds(s.as_raw_fd(), &mut buf).context("receive handoff")?;
        if n == 0 {
            // i18n-ignore: the successor's side of the hand-over, written to hostd.log
            return Err(anyhow!("old hostd closed the handoff early"));
        }
        chunk_fds.extend(fds.into_iter().map(Some));
        pending.extend_from_slice(&buf[..n]);
        while let Some(nl) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = pending.drain(..=nl).collect();
            let b: Bundle =
                // i18n-ignore: the successor's side of the hand-over, written to hostd.log
                serde_json::from_slice(&line[..line.len() - 1]).context("bad handoff bundle")?;
            // The terminator is the line that carries the lock. An empty line without it is a data chunk
            // with no session: alpha.26 sends one first when no terminal runs (#670).
            if b.sessions.is_empty() && b.lock {
                agents = b.agents;
                lock = chunk_fds
                    .first_mut()
                    .and_then(Option::take)
                    .map(std::fs::File::from);
                break 'recv;
            }
            let st_empty = b.sessions.is_empty();
            for st in b.sessions {
                let fd = match st.fd {
                    Some(i) => Some(
                        chunk_fds
                            .get_mut(i)
                            .and_then(Option::take)
                            // i18n-ignore: the successor's side of the hand-over, written to hostd.log
                            .ok_or_else(|| anyhow!("pty {} arrived without its fd", st.id))?,
                    ),
                    None => None,
                };
                sessions.push((st, fd));
            }
            // Indices restart per chunk. An empty line carries none, and the fd that follows it may already
            // have arrived in the same read (alpha.26 sends both at once): it is the terminator's.
            if !st_empty {
                chunk_fds.clear();
            }
        }
    }
    Ok(Takeover {
        sessions,
        lock,
        agents,
        stream: s,
    })
}

/// What the user said about a daemon that cannot be replaced without ending terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    /// Nothing yet: ask.
    #[default]
    Ask,
    /// "Restart when idle": replace it the moment no terminal is running.
    WhenIdle,
    /// "Later": do not ask again this session (the status stays visible).
    Later,
}

/// What to do about the daemon an app or `atlas update` just connected to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Same version and every capability the app needs.
    Current,
    /// Older daemon that can hand its terminals over: `upgrade` (nothing is lost).
    Upgrade,
    /// Older daemon that cannot hand over and has no running terminal: restart it, silently.
    RestartNow,
    /// Cannot hand over, terminals are running, features are missing and the user has not
    /// chosen: ask (list the terminals, never kill them without consent).
    Prompt,
    /// The user chose "restart when idle": keep it until the last terminal exits.
    WaitIdle,
    /// The user chose "later": keep it, report it, do not ask again this session.
    Deferred,
    /// Cannot hand over and terminals are running, but nothing the app needs is missing
    /// (only the version differs): leave the terminals alone, report the skew.
    Passive,
}

/// Capabilities the app's features rely on.
pub const NEEDED: &[&str] = &["pty_attach_raw", "lsp", "agents"];

/// Capabilities in `info` (empty for a proto 1 daemon, which reports none).
pub fn caps_of(info: &Value) -> Vec<&str> {
    info["caps"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Needed capabilities the daemon lacks.
pub fn missing_caps(info: &Value) -> Vec<&'static str> {
    let caps = caps_of(info);
    NEEDED
        .iter()
        .copied()
        .filter(|c| !caps.contains(c))
        .collect()
}

/// Running terminals reported by `info` (`live_from_list` for a proto 1 daemon, which has no
/// `live_ptys`: the caller counts the unexited ones from `pty_list`).
pub fn live_ptys(info: &Value, live_from_list: usize) -> usize {
    info["live_ptys"]
        .as_u64()
        .map_or(live_from_list, |n| n as usize)
}

/// The daemon runs `bundled` or a newer version. A newer one is never replaced by an older
/// caller: the only binary that caller can hand it over to is the installed CLI, which is the
/// version the daemon already runs (#500). Versions that are not semver are compared by equality.
fn at_least(info: &Value, bundled: &str) -> bool {
    let Some(version) = info["version"].as_str() else {
        return false;
    };
    match (
        semver::Version::parse(version),
        semver::Version::parse(bundled),
    ) {
        (Ok(host), Ok(bundled)) => host >= bundled,
        _ => version == bundled,
    }
}

/// Decide from `info`, the live terminal count, the version bundled with the caller and the
/// user's [`Choice`]. Pure: the caller acts on it.
///
/// After a failed `upgrade` call again with `"upgrade"` removed from the caps: the daemon is
/// then treated like one that cannot hand over.
pub fn decide(info: &Value, live_from_list: usize, bundled: &str, choice: Choice) -> Decision {
    let missing = !missing_caps(info).is_empty();
    if !missing && at_least(info, bundled) {
        return Decision::Current;
    }
    if caps_of(info).contains(&"upgrade") {
        return Decision::Upgrade;
    }
    if live_ptys(info, live_from_list) == 0 {
        return Decision::RestartNow;
    }
    if !missing {
        // Same capabilities, only the version differs, and it cannot be upgraded live:
        // nothing the user would notice is missing, so leave their terminals alone.
        return Decision::Passive;
    }
    match choice {
        Choice::Ask => Decision::Prompt,
        Choice::WhenIdle => Decision::WaitIdle,
        Choice::Later => Decision::Deferred,
    }
}

/// Why a daemon is not current, for every place that reports it: the sentence in `message` (English, for logs and
/// the CLI), and `code` plus `args` for a client that writes its own (the desktop renders it from its catalog).
/// `None` when the daemon is current.
///
/// Code [`SKEW_CODE`], args `{version, app_version, missing}`: `version` is what the daemon reports (`unknown`
/// when it reports none), `app_version` the version of the caller, `missing` the features that are off until a
/// restart, a subset of [`FEATURES`] in that order (empty when only the version differs).
pub fn skew(info: &Value, bundled: &str) -> Option<Coded> {
    let version = info["version"].as_str().unwrap_or("unknown");
    let missing = missing_caps(info);
    if missing.is_empty() && at_least(info, bundled) {
        return None;
    }
    let off: Vec<&str> = FEATURES
        .iter()
        .copied()
        .filter(|c| missing.contains(c))
        .collect();
    let mut s = format!("Terminal host {version} is older than Atlas {bundled}");
    if !off.is_empty() {
        // Every feature name reads as plural or is listed with a sibling: "are" in every case.
        s.push_str(&format!(
            "; {} are off until it restarts",
            features_text("en", &off)
        ));
    }
    s.push('.');
    Some(Coded::new(
        SKEW_CODE,
        json!({
            "version": version,
            "app_version": bundled,
            "missing": off,
        }),
        s,
    ))
}

/// The code of [`skew`].
pub const SKEW_CODE: &str = "hostd.older";

/// The capabilities the app needs, in the order a person reads what a daemon without them loses first
/// (`args.missing` of `hostd.older`, which the desktop has a message for each of).
pub const FEATURES: &[&str] = &["lsp", "pty_attach_raw", "agents"];

/// What a daemon without capability `cap` cannot offer, in `tag`; an id this build does not know is shown as it is.
pub fn feature_name(tag: &str, cap: &str) -> String {
    match cap {
        "lsp" => t_in!(tag, "cmd_hostd_feature_lsp"),
        "pty_attach_raw" => t_in!(tag, "cmd_hostd_feature_pty_attach_raw"),
        "agents" => t_in!(tag, "cmd_hostd_feature_agents"),
        other => other.to_string(),
    }
}

/// The lost features as one list in `tag` ("a", "a and b", "a, b, and c" in English; "a, b e c" in Portuguese): one,
/// two or three names ([`NEEDED`] has three), in the order of [`FEATURES`] whatever order hostd lists its
/// capabilities in. The one place the sentence and `atlas hostd --check` get that list from.
pub fn features_text(tag: &str, caps: &[&str]) -> String {
    let mut caps = caps.to_vec();
    caps.sort_by_key(|c| {
        FEATURES
            .iter()
            .position(|f| f == c)
            .unwrap_or(FEATURES.len())
    });
    let names: Vec<String> = caps.iter().map(|c| feature_name(tag, c)).collect();
    let name = |i: usize| names.get(i).cloned().unwrap_or_default();
    t_in!(
        tag,
        "cmd_hostd_features_list",
        count = names.len(),
        first = name(0),
        second = name(1),
        third = name(2)
    )
}

/// The English sentence of [`skew`]; `None` when the daemon is current.
pub fn skew_reason(info: &Value, bundled: &str) -> Option<String> {
    skew(info, bundled).map(|c| c.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &[&str] = &["pty_attach_raw", "upgrade", "lsp", "agents"];

    fn info(version: &str, caps: &[&str], live: Option<u64>) -> Value {
        let mut v = json!({"version": version, "caps": caps});
        if let Some(l) = live {
            v["live_ptys"] = json!(l);
        }
        v
    }

    #[test]
    fn current_needs_same_version_and_every_needed_cap() {
        let d = |i: Value| decide(&i, 0, "1.2.0", Choice::Ask);
        assert_eq!(d(info("1.2.0", FULL, Some(3))), Decision::Current);
        // Same version missing a needed cap (no `lsp`) is not current.
        assert_eq!(
            d(info(
                "1.2.0",
                &["pty_attach_raw", "upgrade", "agents"],
                Some(3)
            )),
            Decision::Upgrade
        );
    }

    /// Regression: #500. An update hands the daemon over to the new version (`atlas update`),
    /// and the app still running the old version then saw "version differs" and handed it over
    /// again, to the very binary it already runs (`hostd.log`: two `1.5.0-alpha.26 ... took over
    /// 8 pty(s)` for one alpha.25 -> alpha.26 update). A daemon newer than the caller is never
    /// replaced by it, and is no skew either.
    #[test]
    fn a_daemon_newer_than_the_caller_is_current_and_never_handed_over() {
        let host = info("1.5.0-alpha.26", FULL, Some(8));
        assert_eq!(
            decide(&host, 8, "1.5.0-alpha.25", Choice::Ask),
            Decision::Current
        );
        assert_eq!(skew_reason(&host, "1.5.0-alpha.25"), None);
        // Numeric, not lexical, ordering: alpha.9 is older than alpha.10.
        assert_eq!(
            decide(
                &info("1.5.0-alpha.10", FULL, None),
                0,
                "1.5.0-alpha.9",
                Choice::Ask
            ),
            Decision::Current
        );
        assert_eq!(
            decide(
                &info("1.5.0-alpha.9", FULL, None),
                0,
                "1.5.0-alpha.10",
                Choice::Ask
            ),
            Decision::Upgrade
        );
        // Newer but missing a capability the caller needs: still not current.
        assert_ne!(
            decide(
                &info(
                    "1.5.0-alpha.26",
                    &["pty_attach_raw", "upgrade", "agents"],
                    None
                ),
                0,
                "1.5.0-alpha.25",
                Choice::Ask
            ),
            Decision::Current
        );
        // A version that is no semver cannot be ordered: it keeps the old "differs" rule.
        assert_eq!(
            decide(&info("dev", FULL, None), 0, "1.5.0-alpha.25", Choice::Ask),
            Decision::Upgrade
        );
    }

    #[test]
    fn upgradeable_daemons_hand_over_with_or_without_terminals() {
        for live in [0, 3] {
            assert_eq!(
                decide(&info("1.1.0", FULL, Some(live)), 0, "1.2.0", Choice::Ask),
                Decision::Upgrade
            );
        }
    }

    /// proto 1: no caps, no `live_ptys`; the `pty_list` count decides.
    #[test]
    fn proto1_restarts_when_empty_and_asks_when_not() {
        let old = json!({"version": "0.6.2"});
        assert_eq!(decide(&old, 0, "1.2.1", Choice::Ask), Decision::RestartNow);
        // Restarting an empty host needs no consent, whatever was chosen before.
        assert_eq!(
            decide(&old, 0, "1.2.1", Choice::Later),
            Decision::RestartNow
        );
        assert_eq!(decide(&old, 2, "1.2.1", Choice::Ask), Decision::Prompt);
        assert_eq!(
            decide(&old, 2, "1.2.1", Choice::WhenIdle),
            Decision::WaitIdle
        );
        assert_eq!(decide(&old, 2, "1.2.1", Choice::Later), Decision::Deferred);
    }

    #[test]
    fn a_failed_handoff_falls_back_to_the_proto1_path() {
        let mut i = info("1.1.0", FULL, Some(2));
        i["caps"] = json!(["pty_attach_raw", "lsp", "agents"]); // `upgrade` removed by the caller
        // Nothing needed is missing, only the version differs: report, do not kill.
        assert_eq!(decide(&i, 0, "1.2.0", Choice::Ask), Decision::Passive);
        i["live_ptys"] = json!(0);
        assert_eq!(decide(&i, 0, "1.2.0", Choice::Ask), Decision::RestartNow);
        i["caps"] = json!(["pty_attach_raw"]);
        i["live_ptys"] = json!(2);
        assert_eq!(decide(&i, 0, "1.2.0", Choice::Ask), Decision::Prompt);
    }

    // Guards silent loss of agent status: the app no longer runs a hook server, so a
    // host from before `agents` (1.5.0-alpha.14, every Windows host) leaves tabs without
    // state; it was judged "only the version differs" and left running quietly.
    #[test]
    fn a_host_without_agents_is_missing_a_feature() {
        let old = info("1.5.0-alpha.14", &["pty_attach_raw", "lsp"], Some(2));
        assert_eq!(missing_caps(&old), ["agents"]);
        assert_eq!(decide(&old, 2, "1.5.0", Choice::Ask), Decision::Prompt);
        let r = skew_reason(&old, "1.5.0").unwrap();
        assert!(r.contains("agent status"), "{r}");
    }

    #[test]
    fn only_missing_features_prompt() {
        let same_caps = info("1.1.0", &["pty_attach_raw", "lsp", "agents"], Some(2));
        assert_eq!(
            decide(&same_caps, 2, "1.2.0", Choice::Ask),
            Decision::Passive
        );
        assert_eq!(
            decide(&same_caps, 2, "1.2.0", Choice::Later),
            Decision::Passive
        );
    }

    #[test]
    fn live_count_prefers_the_daemon_over_the_list() {
        assert_eq!(live_ptys(&json!({"live_ptys": 4}), 9), 4);
        assert_eq!(live_ptys(&json!({}), 9), 9);
    }

    #[test]
    fn reason_names_the_versions_and_what_is_off() {
        let r = skew_reason(&json!({"version": "0.6.2"}), "1.2.1").unwrap();
        assert_eq!(
            r,
            "Terminal host 0.6.2 is older than Atlas 1.2.1; language features, fast terminal rendering, and agent status are off until it restarts."
        );
        let r = skew_reason(
            &info("1.1.0", &["pty_attach_raw", "lsp", "agents"], None),
            "1.2.0",
        )
        .unwrap();
        assert_eq!(r, "Terminal host 1.1.0 is older than Atlas 1.2.0.");
        // Only `lsp` missing: one plural noun, still "are".
        let r = skew_reason(
            &info("1.1.0", &["pty_attach_raw", "upgrade", "agents"], None),
            "1.2.0",
        )
        .unwrap();
        assert_eq!(
            r,
            "Terminal host 1.1.0 is older than Atlas 1.2.0; language features are off until it restarts."
        );
        assert_eq!(skew_reason(&info("1.2.0", FULL, None), "1.2.0"), None);
    }

    /// The code and args behind the sentence are what a client renders its own text from (the desktop's
    /// `hostd.older` message): the versions as given, the lost features by name and in the order the sentence lists.
    #[test]
    fn skew_carries_the_code_and_the_arguments_the_sentence_was_written_from() {
        let c = skew(&json!({"version": "0.6.2"}), "1.2.1").unwrap();
        assert_eq!(c.code, "hostd.older");
        assert_eq!(
            c.wire(),
            json!({
                "code": "hostd.older",
                "args": {"version": "0.6.2", "app_version": "1.2.1", "missing": ["lsp", "pty_attach_raw", "agents"]},
                "message": skew_reason(&json!({"version": "0.6.2"}), "1.2.1").unwrap(),
            })
        );
        // Only the version differs: no feature is lost.
        let c = skew(&info("1.1.0", FULL, None), "1.2.0").unwrap();
        assert_eq!(c.args["missing"], json!([]));
        // A daemon that reports no version is `unknown`, never an empty string in the sentence.
        let c = skew(&json!({"caps": FULL}), "1.2.0").unwrap();
        assert_eq!(c.args["version"], "unknown");
    }

    /// `FEATURES` is what the sentence and the desktop have a name for; `NEEDED` is what the app checks. A
    /// capability added to one and not the other would be reported as missing with no word for it.
    #[test]
    fn every_needed_capability_has_a_feature_name_and_the_other_way_round() {
        let mut needed: Vec<&str> = NEEDED.to_vec();
        let mut named: Vec<&str> = FEATURES.to_vec();
        needed.sort_unstable();
        named.sort_unstable();
        assert_eq!(needed, named);
        // The list message holds three names at most, and every language names every capability.
        assert!(NEEDED.len() <= 3, "{NEEDED:?}");
        for tag in atlas_i18n::locales() {
            for cap in NEEDED {
                let name = feature_name(tag, cap);
                assert!(!name.is_empty() && name != *cap, "{tag}: {cap} -> {name}");
            }
        }
    }

    /// One list for the sentence and for `atlas hostd --check`: the serial comma in English (like the desktop's
    /// `Intl.ListFormat`), "a, b e c" in Portuguese, in the order of `FEATURES` whatever order hostd reports.
    #[test]
    fn the_lost_features_read_as_one_list_in_each_language() {
        let list = |tag: &str, caps: &[&str]| features_text(tag, caps);
        assert_eq!(list("en", &["lsp"]), "language features");
        assert_eq!(
            list("en", &["lsp", "agents"]),
            "language features and agent status"
        );
        assert_eq!(
            list("en", &["pty_attach_raw", "lsp", "agents"]),
            "language features, fast terminal rendering, and agent status"
        );
        assert_eq!(
            list("pt-BR", &["pty_attach_raw", "lsp", "agents"]),
            "recursos de linguagem, renderização rápida do terminal e status dos agentes"
        );
        // The sentence uses the same text.
        let all = skew(&json!({"version": "0.6.2"}), "1.2.1").unwrap().message;
        assert!(
            all.contains(&features_text("en", &["lsp", "pty_attach_raw", "agents"])),
            "{all}"
        );
        assert_eq!(feature_name("en", "teleport"), "teleport");
    }
}

/// The successor reading what an old daemon sent: the real bytes of alpha.26 (captured from a running
/// 1.5.0-alpha.26 `atlas hostd` by `upgrade`, see `testdata/hostd/handoff/`) and the current encoder's own output.
#[cfg(all(test, unix))]
mod handoff_wire {
    use super::*;
    use crate::handoff::SessionState;

    /// One `recvmsg` of the successor stub: the bytes the old daemon sent and how many fds came with them.
    #[derive(serde::Deserialize)]
    struct Sent {
        fds: usize,
        data: String,
    }

    fn fixture(name: &str) -> Vec<Sent> {
        let p = crate::protocol_golden::fixture(&format!("handoff/{name}"));
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
    }

    /// Plays `sent` to `receive_blocking` over a real socket, each message with fresh fds (one end of a socket pair
    /// each: anything the receiver can own), and returns what the successor made of it.
    fn play(sent: Vec<(Vec<u8>, usize)>) -> Result<Takeover> {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("h.sock");
        let l = UnixListener::bind(&sock).unwrap();
        let old = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut keep = Vec::new();
            for (payload, n) in sent {
                let mut raw = Vec::new();
                for _ in 0..n {
                    let (r, w) = UnixStream::pair().unwrap();
                    raw.push(r.as_raw_fd());
                    keep.push((r, w));
                }
                send_with_fds(s.as_raw_fd(), &payload, &raw).unwrap();
            }
            // Like the old daemon: wait for the successor's `R` (or its EOF), then let go.
            let mut r = [0u8; 1];
            let _ = (&s).read(&mut r);
            drop(keep);
        });
        let got = receive_blocking(&sock).map(|mut t| {
            let _ = t.stream.write_all(b"R");
            t
        });
        old.join().unwrap();
        got
    }

    fn from_fixture(name: &str) -> Result<Takeover> {
        play(
            fixture(name)
                .into_iter()
                .map(|m| (m.data.into_bytes(), m.fds))
                .collect(),
        )
    }

    /// Regression: #670. `atlas update` with no terminal open failed every time with "waiting for the new hostd: failed
    /// to fill whole buffer": alpha.26 sends an empty chunk (no sessions, `lock: false`) before the terminator, and the
    /// successor took that first empty line for the terminator, left the pid-file fd unread, found the pid lock still
    /// held by the old daemon, exited, and the old daemon read EOF where it waited for `R`.
    #[test]
    fn a_handoff_of_an_alpha_26_daemon_with_no_terminal_reaches_the_terminator() {
        let t =
            from_fixture("alpha26-no-terminals.json").expect("successor must accept the handoff");
        assert!(t.sessions.is_empty());
        assert!(
            t.lock.is_some(),
            "the pid-file fd arrives with the terminator"
        );
        assert_eq!(
            t.agents["by_cwd"]["omp|/w"], "p1",
            "the agent state is in the terminator, not in the empty chunk before it"
        );
    }

    #[test]
    fn a_handoff_of_an_alpha_26_daemon_with_terminals_keeps_every_session_and_fd() {
        let t =
            from_fixture("alpha26-two-terminals.json").expect("successor must accept the handoff");
        let ids: Vec<_> = t.sessions.iter().map(|(s, _)| s.argv.join(" ")).collect();
        assert_eq!(ids, ["sh -c sleep 300", "sh -c echo gone"]);
        assert!(
            t.sessions[0].1.is_some(),
            "the live pty's master fd travels"
        );
        assert!(
            t.sessions[1].1.is_none(),
            "a finished pty travels as state only"
        );
        assert!(t.lock.is_some());
    }

    fn state(id: &str, fd: Option<usize>) -> SessionState {
        let mut s: SessionState = serde_json::from_value(json!({
            "id": id, "cwd": "/tmp", "argv": ["sh"], "scope": {}, "created_at": 1, "title": "",
            "cols": 80, "rows": 24, "pid": 1, "file_len": 0, "exited": false, "exit_code": null, "iso": null,
        }))
        .unwrap();
        s.fd = fd;
        s
    }

    /// What the current encoder sends is what the current successor reads, from no terminal up to more than one
    /// message of fds (the chunk boundary), with the agents and the lock arriving whatever the count.
    #[test]
    fn the_encoder_and_the_successor_agree_for_any_number_of_terminals() {
        for n in [0usize, 1, 31, 32, 33, 70] {
            let bundle = Bundle {
                from_version: "x".into(),
                sessions: (0..n).map(|i| state(&format!("s{i}"), Some(i))).collect(),
                lock: true,
                agents: json!({"panes": {}, "by_cwd": {"omp|/w": "p1"}}),
            };
            let fds: Vec<_> = (0..n as i32).collect();
            let sent = encode_chunks(&bundle, &fds, 99)
                .into_iter()
                .map(|(p, f)| (p, f.len()))
                .collect();
            let t = play(sent).unwrap_or_else(|e| panic!("{n} terminals: {e:#}"));
            assert_eq!(t.sessions.len(), n, "{n} terminals");
            assert!(
                t.sessions.iter().all(|(_, fd)| fd.is_some()),
                "{n} terminals"
            );
            assert!(t.lock.is_some(), "{n} terminals");
            assert_eq!(t.agents["by_cwd"]["omp|/w"], "p1", "{n} terminals");
        }
    }
}
