//! hostd's side of the spawn helper: ask it to start a crewmate's process and get the pty master (or
//! one end of a socketpair) back. Blocking, one connection per request: nothing to keep in step
//! across a helper restart, and every caller is already on a blocking thread or can use one.

use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::protocol::{GitReq, Request, Response, SpawnReq};
use super::wire;

/// A reply is not expected to take long (the helper only forks); a helper that does not answer in
/// this time is as good as down.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// How a git run ended, with what it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOut {
    /// Exit code; `None` when a signal ended it (the deadline's kill, the size limit).
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    /// The end of stderr.
    pub stderr: String,
    pub timed_out: bool,
}

/// What the helper reported about a spawned process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spawned {
    pub session: u64,
    pub pid: u32,
}

/// A handle on the helper's socket. Cheap to clone: it is a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    path: PathBuf,
}

impl Client {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn call(&self, req: &Request) -> io::Result<(Response, Vec<OwnedFd>)> {
        self.call_within(req, REPLY_TIMEOUT)
    }

    fn call_within(&self, req: &Request, wait: Duration) -> io::Result<(Response, Vec<OwnedFd>)> {
        let conn = UnixStream::connect(&self.path)?;
        conn.set_read_timeout(Some(wait))?;
        conn.set_write_timeout(Some(REPLY_TIMEOUT))?;
        wire::send(&conn, req, &[])?;
        let f = wire::recv::<Response>(&conn)?.ok_or_else(|| {
            // The helper closes without a word on a peer it does not serve.
            io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "the spawn helper closed the connection (is this user allowed to use it?)",
            )
        })?;
        Ok((f.msg, f.fds))
    }

    /// Run git as the crewmate and wait for it. The helper ends it at `req.timeout_secs`; this waits a
    /// little longer than that for the answer.
    pub fn git(&self, req: &GitReq) -> io::Result<GitOut> {
        use std::io::Read;
        let wait = Duration::from_secs(req.timeout_secs) + REPLY_TIMEOUT;
        match self.call_within(&Request::Git(req.clone()), wait)? {
            (
                Response::Git {
                    code,
                    stderr,
                    timed_out,
                },
                mut fds,
            ) if fds.len() == 1 => {
                let mut stdout = Vec::new();
                // One byte over the helper's own limit is already an error, never a silent cut.
                std::fs::File::from(fds.remove(0))
                    .take(super::git::MAX_OUTPUT + 1)
                    .read_to_end(&mut stdout)?;
                if stdout.len() as u64 > super::git::MAX_OUTPUT {
                    return Err(io::Error::other("git wrote more than the helper allows"));
                }
                Ok(GitOut {
                    code,
                    stdout,
                    stderr,
                    timed_out,
                })
            }
            (Response::Git { .. }, fds) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "the helper sent {} fds with a git reply, expected 1",
                    fds.len()
                ),
            )),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Start `req` as its crewmate. The fd is the pty master (`Mode::Pty`) or hostd's end of the
    /// socketpair (`Mode::Pipe`).
    pub fn spawn(&self, req: &SpawnReq) -> io::Result<(Spawned, OwnedFd)> {
        match self.call(&Request::Spawn(req.clone()))? {
            (Response::Spawned { session, pid }, mut fds) if fds.len() == 1 => {
                Ok((Spawned { session, pid }, fds.remove(0)))
            }
            (Response::Spawned { .. }, fds) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the helper sent {} fds with a spawn, expected 1", fds.len()),
            )),
            (other, _) => Err(unexpected(other)),
        }
    }

    /// Signal a session's process (`sig` is one of `validate::SIGNALS`).
    pub fn signal(&self, session: u64, sig: i32) -> io::Result<()> {
        match self.call(&Request::Signal { session, sig })?.0 {
            Response::Done => Ok(()),
            other => Err(unexpected(other)),
        }
    }

    /// `(exited, code)` of a session. An unknown session (the helper restarted) is an error the
    /// caller answers by watching the pid.
    pub fn wait(&self, session: u64) -> io::Result<(bool, Option<i32>)> {
        self.wait_oom(session).map(|(e, c, _)| (e, c))
    }

    /// [`Client::wait`] plus the cap the session was OOM-killed under, if it was.
    pub fn wait_oom(&self, session: u64) -> io::Result<(bool, Option<i32>, Option<u64>)> {
        match self.call(&Request::Wait { session })?.0 {
            Response::Exit {
                exited,
                code,
                oom_cap,
            } => Ok((exited, code, oom_cap)),
            other => Err(unexpected(other)),
        }
    }

    /// Whether agents run under a memory cap.
    pub fn memory_cap(&self) -> io::Result<super::cgroup::Status> {
        match self.call(&Request::MemoryCap)?.0 {
            Response::MemoryCap(s) => Ok(s),
            other => Err(unexpected(other)),
        }
    }

    /// Set the cap of every agent, in bytes.
    pub fn set_memory_cap(&self, bytes: u64) -> io::Result<super::cgroup::Status> {
        match self.call(&Request::SetMemoryCap { bytes })?.0 {
            Response::MemoryCap(s) => Ok(s),
            other => Err(unexpected(other)),
        }
    }

    /// The working directory of a session's process.
    pub fn cwd(&self, session: u64) -> io::Result<String> {
        match self.call(&Request::Cwd { session })?.0 {
            Response::Cwd { path } => Ok(path),
            other => Err(unexpected(other)),
        }
    }

    /// End every process of a crewmate's user.
    pub fn reap(&self, user: &str) -> io::Result<()> {
        match self
            .call(&Request::Reap {
                user: user.to_string(),
            })?
            .0
        {
            Response::Done => Ok(()),
            other => Err(unexpected(other)),
        }
    }

    /// Block until a session's process exits; its code when the helper knows it. Polls the helper
    /// (a few times a second), because a connection held open per session would be one more thing
    /// to lose when the helper restarts. `None` once the helper no longer knows the session (it
    /// restarted: the caller then watches the pid), or stays unreachable for [`GIVE_UP`].
    pub fn wait_exit(&self, session: u64) -> Option<i32> {
        let mut down_since: Option<std::time::Instant> = None;
        loop {
            match self.wait(session) {
                Ok((true, code)) => return code,
                Ok((false, _)) => down_since = None,
                Err(e) if is_unknown_session(&e) => return None,
                // Connection refused while systemd restarts the helper: keep trying for a while.
                Err(_) => {
                    let t = *down_since.get_or_insert_with(std::time::Instant::now);
                    if t.elapsed() > GIVE_UP {
                        return None;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

/// How long [`Client::wait_exit`] tolerates an unreachable helper before it stops waiting.
const GIVE_UP: Duration = Duration::from_secs(30);

/// The helper answers `Error{ "unknown session N" }` for a session it does not have (after a restart).
pub fn is_unknown_session(e: &io::Error) -> bool {
    e.to_string().starts_with("unknown session")
}

fn unexpected(r: Response) -> io::Error {
    match r {
        Response::Error { reason } => io::Error::other(reason),
        other => io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unexpected reply from the spawn helper: {other:?}"),
        ),
    }
}
