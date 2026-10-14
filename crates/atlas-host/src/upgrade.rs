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
use serde_json::Value;
#[cfg(any(unix, test))]
use serde_json::json;

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
#[cfg(unix)]
pub(crate) async fn run(hub: &Hub, exe: &str) -> Result<Value, String> {
    let exe = Path::new(exe);
    if !exe.is_file() {
        return Err(format!("{} is not a file", exe.display()));
    }
    run_inner(hub, exe).await.map_err(|e| format!("{e:#}"))
}

#[cfg(unix)]
async fn run_inner(hub: &Hub, exe: &Path) -> Result<Value> {
    let sock = handoff_socket(&hub.dir, std::process::id());
    let _ = std::fs::remove_file(&sock);
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
        .with_context(|| format!("spawn {}", exe.display()))?;
    let child_pid = child.id();

    let (stream, child) = tokio::task::spawn_blocking(move || {
        let s = accept_timeout(&listener, &mut child, ACCEPT_TIMEOUT);
        (s, child)
    })
    .await?;
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
                    return Err(anyhow!("the new hostd exited early ({st})"));
                }
                if t0.elapsed() > timeout {
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
    for (payload, chunk_fds) in encode_chunks(bundle, fds, Some(lock_fd)) {
        send_with_fds(s.as_raw_fd(), &payload, &chunk_fds)?;
    }
    s.set_read_timeout(Some(ACK_TIMEOUT))?;
    let mut ack = [0u8; 1];
    s.read_exact(&mut ack)
        .context("waiting for the new hostd")?;
    if ack[0] != b'R' {
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
    let s = UnixStream::connect(sock).with_context(|| format!("connect {}", sock.display()))?;
    s.set_read_timeout(Some(ACK_TIMEOUT))?;
    let mut sessions = Vec::new();
    let mut lock = None;
    let mut pending: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut chunk_fds: Vec<Option<OwnedFd>> = Vec::new();
    'recv: loop {
        let (n, fds) = recv_with_fds(s.as_raw_fd(), &mut buf).context("receive handoff")?;
        if n == 0 {
            return Err(anyhow!("old hostd closed the handoff early"));
        }
        chunk_fds.extend(fds.into_iter().map(Some));
        pending.extend_from_slice(&buf[..n]);
        while let Some(nl) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = pending.drain(..=nl).collect();
            let b: Bundle =
                serde_json::from_slice(&line[..line.len() - 1]).context("bad handoff bundle")?;
            if b.sessions.is_empty() {
                if b.lock {
                    lock = chunk_fds
                        .first_mut()
                        .and_then(Option::take)
                        .map(std::fs::File::from);
                }
                break 'recv;
            }
            for st in b.sessions {
                let fd = match st.fd {
                    Some(i) => Some(
                        chunk_fds
                            .get_mut(i)
                            .and_then(Option::take)
                            .ok_or_else(|| anyhow!("pty {} arrived without its fd", st.id))?,
                    ),
                    None => None,
                };
                sessions.push((st, fd));
            }
            // Indices restart per chunk.
            chunk_fds.clear();
        }
    }
    Ok(Takeover {
        sessions,
        lock,
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
pub const NEEDED: &[&str] = &["pty_attach_raw", "lsp"];

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

/// Decide from `info`, the live terminal count, the version bundled with the caller and the
/// user's [`Choice`]. Pure: the caller acts on it.
///
/// After a failed `upgrade` call again with `"upgrade"` removed from the caps: the daemon is
/// then treated like one that cannot hand over.
pub fn decide(info: &Value, live_from_list: usize, bundled: &str, choice: Choice) -> Decision {
    let missing = !missing_caps(info).is_empty();
    if !missing && info["version"].as_str() == Some(bundled) {
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

/// The sentence shown wherever a daemon that is not current is reported; `None` when it is.
pub fn skew_reason(info: &Value, bundled: &str, app: &str) -> Option<String> {
    let version = info["version"].as_str().unwrap_or("unknown");
    let missing = missing_caps(info);
    if missing.is_empty() && version == bundled {
        return None;
    }
    let mut s = format!("Terminal host {version} is older than {app}");
    if !missing.is_empty() {
        let lost = [
            ("lsp", "language features"),
            ("pty_attach_raw", "fast terminal rendering"),
        ];
        let off: Vec<&str> = lost
            .iter()
            .filter(|(c, _)| missing.contains(c))
            .map(|(_, d)| *d)
            .collect();
        // Both feature names are plural ("language features", "fast terminal rendering" reads
        // as one thing but is listed with its sibling): "are" in every case.
        s.push_str(&format!(
            "; {} are off until it restarts",
            off.join(" and ")
        ));
    }
    s.push('.');
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &[&str] = &["pty_attach_raw", "upgrade", "lsp"];

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
            d(info("1.2.0", &["pty_attach_raw", "upgrade"], Some(3))),
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
        i["caps"] = json!(["pty_attach_raw", "lsp"]); // `upgrade` removed by the caller
        // Nothing needed is missing, only the version differs: report, do not kill.
        assert_eq!(decide(&i, 0, "1.2.0", Choice::Ask), Decision::Passive);
        i["live_ptys"] = json!(0);
        assert_eq!(decide(&i, 0, "1.2.0", Choice::Ask), Decision::RestartNow);
        i["caps"] = json!(["pty_attach_raw"]);
        i["live_ptys"] = json!(2);
        assert_eq!(decide(&i, 0, "1.2.0", Choice::Ask), Decision::Prompt);
    }

    #[test]
    fn only_missing_features_prompt() {
        let same_caps = info("1.1.0", &["pty_attach_raw", "lsp"], Some(2));
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
        let r = skew_reason(&json!({"version": "0.6.2"}), "1.2.1", "Atlas 1.2.1").unwrap();
        assert_eq!(
            r,
            "Terminal host 0.6.2 is older than Atlas 1.2.1; language features and fast terminal rendering are off until it restarts."
        );
        let r = skew_reason(
            &info("1.1.0", &["pty_attach_raw", "lsp"], None),
            "1.2.0",
            "Atlas 1.2.0",
        )
        .unwrap();
        assert_eq!(r, "Terminal host 1.1.0 is older than Atlas 1.2.0.");
        // Only `lsp` missing: one plural noun, still "are".
        let r = skew_reason(
            &info("1.1.0", &["pty_attach_raw", "upgrade"], None),
            "1.2.0",
            "Atlas 1.2.0",
        )
        .unwrap();
        assert_eq!(
            r,
            "Terminal host 1.1.0 is older than Atlas 1.2.0; language features are off until it restarts."
        );
        assert_eq!(
            skew_reason(&info("1.2.0", FULL, None), "1.2.0", "Atlas 1.2.0"),
            None
        );
    }
}
