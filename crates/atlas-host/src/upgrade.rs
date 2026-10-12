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

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};

use crate::handoff::{
    Bundle, SessionState, encode_chunks, handoff_socket, recv_with_fds, send_with_fds,
};
use crate::paths;
use crate::server::Hub;

const ACCEPT_TIMEOUT: Duration = Duration::from_secs(20);
const ACK_TIMEOUT: Duration = Duration::from_secs(20);

/// Old side of the handoff, for the `upgrade` command.
pub(crate) async fn run(hub: &Hub, exe: &str) -> Result<Value, String> {
    let exe = Path::new(exe);
    if !exe.is_file() {
        return Err(format!("{} is not a file", exe.display()));
    }
    run_inner(hub, exe).await.map_err(|e| format!("{e:#}"))
}

async fn run_inner(hub: &Hub, exe: &Path) -> Result<Value> {
    let sock = handoff_socket(&hub.dir, std::process::id());
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).with_context(|| format!("bind {}", sock.display()))?;
    std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    let _cleanup = scopeguard(sock.clone());

    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::log(&hub.dir))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("hostd")
        .arg("--dir")
        .arg(&hub.dir)
        .arg("--takeover")
        .arg(&sock)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // SAFETY: setsid is async-signal-safe; the successor must outlive us and the app.
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
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
pub(crate) struct Takeover {
    pub sessions: Vec<(SessionState, Option<OwnedFd>)>,
    pub lock: Option<std::fs::File>,
    stream: UnixStream,
}

impl Takeover {
    /// Tell the old daemon we serve now (it may exit). Until this, it stays authoritative
    /// and takes the ptys back if we die.
    pub fn confirm(mut self) -> Result<()> {
        self.stream.write_all(b"R")?;
        Ok(())
    }
}

/// Successor side: connect to the old daemon and read every chunk.
pub(crate) async fn receive(sock: &Path) -> Result<Takeover> {
    let sock = sock.to_path_buf();
    tokio::task::spawn_blocking(move || receive_blocking(&sock)).await?
}

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

/// What the desktop should do about the daemon it just connected to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Same version and every capability the app needs.
    Current,
    /// Older daemon that can hand its terminals over: `upgrade`.
    Upgrade,
    /// Older daemon that cannot hand over (proto 1) but has nothing running: restart it.
    RestartEmpty,
    /// Older daemon that cannot hand over and has live terminals: keep it, tell the user.
    Outdated,
}

/// Capabilities the app's features rely on.
const NEEDED: &[&str] = &["pty_attach_raw"];

/// Decide from `info` (and, for a proto 1 daemon that does not report `live_ptys`,
/// the live count the caller derived from `pty_list`).
pub fn plan(info: &Value, live_from_list: usize, bundled: &str) -> Plan {
    let caps: Vec<&str> = info["caps"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let missing = NEEDED.iter().any(|c| !caps.contains(c));
    if !missing && info["version"].as_str() == Some(bundled) {
        return Plan::Current;
    }
    if caps.contains(&"upgrade") {
        return Plan::Upgrade;
    }
    let live = info["live_ptys"]
        .as_u64()
        .map_or(live_from_list, |n| n as usize);
    if live == 0 {
        Plan::RestartEmpty
    } else if missing {
        Plan::Outdated
    } else {
        // Same capabilities, only the version differs, and it cannot be upgraded live:
        // nothing the user would notice is missing, so leave their terminals alone.
        Plan::Current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(version: &str, caps: &[&str], live: Option<u64>) -> Value {
        let mut v = json!({"version": version, "caps": caps});
        if let Some(l) = live {
            v["live_ptys"] = json!(l);
        }
        v
    }

    #[test]
    fn plan_matrix() {
        let raw = ["pty_attach_raw", "upgrade"];
        assert_eq!(
            plan(&info("1.2.0", &raw, Some(3)), 0, "1.2.0"),
            Plan::Current
        );
        // Same-version daemon missing a needed cap is not current.
        assert_eq!(
            plan(&info("1.2.0", &["upgrade"], Some(3)), 0, "1.2.0"),
            Plan::Upgrade
        );
        // Newer app, upgradeable daemon: hand over, with or without terminals.
        assert_eq!(
            plan(&info("1.1.0", &raw, Some(3)), 0, "1.2.0"),
            Plan::Upgrade
        );
        assert_eq!(
            plan(&info("1.1.0", &raw, Some(0)), 0, "1.2.0"),
            Plan::Upgrade
        );
        // proto 1 (no caps, no live_ptys): the list decides.
        let old = json!({"version": "0.5.13"});
        assert_eq!(plan(&old, 0, "1.2.0"), Plan::RestartEmpty);
        assert_eq!(plan(&old, 2, "1.2.0"), Plan::Outdated);
        // A daemon with the caps but no `upgrade` and a different version stays put.
        assert_eq!(
            plan(&info("1.1.0", &["pty_attach_raw"], Some(2)), 2, "1.2.0"),
            Plan::Current
        );
    }
}
