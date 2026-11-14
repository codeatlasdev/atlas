//! How an engine's process is started: as the crew user (today), or as the crewmate's own Linux
//! user through the spawn helper (issue #200). Every engine builds a [`Cmd`] and asks [`Runner`] for
//! one of two shapes; nothing else in an engine knows which one it got.
//!
//! - [`Runner::output`]: run to the end, collect what it printed (claude, codex, opencode).
//! - [`Runner::rpc`]: a line protocol on stdin/stdout that the engine drives (omp).
//!
//! Through the helper a process gets one end of a socketpair as stdin and stdout, with stderr merged
//! into stdout: there is no separate stderr to read, so on failure the error text is the output.

use std::collections::BTreeMap;
#[cfg(unix)]
use std::io;
use std::time::Duration;

#[cfg(unix)]
use tokio::io::AsyncReadExt;
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(unix)]
use crate::crew::spawn::client::Client;
#[cfg(unix)]
use crate::crew::spawn::protocol::{Mode, SpawnReq};

/// One process to run.
#[derive(Debug, Clone)]
pub struct Cmd {
    /// An engine binary by bare name (`claude`); the helper resolves it in its pinned directories,
    /// and the crew user finds it on its own `PATH`.
    pub program: &'static str,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: String,
}

impl Cmd {
    pub fn new(program: &'static str, cwd: &str) -> Self {
        Self {
            program,
            args: vec![],
            env: BTreeMap::new(),
            cwd: cwd.to_string(),
        }
    }

    pub fn arg(&mut self, a: impl Into<String>) -> &mut Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I: IntoIterator<Item = String>>(&mut self, a: I) -> &mut Self {
        self.args.extend(a);
        self
    }

    pub fn env(&mut self, k: impl Into<String>, v: impl Into<String>) -> &mut Self {
        self.env.insert(k.into(), v.into());
        self
    }

    pub fn envs<'a>(&mut self, e: impl IntoIterator<Item = (&'a String, &'a String)>) -> &mut Self {
        for (k, v) in e {
            self.env.insert(k.clone(), v.clone());
        }
        self
    }
}

/// Where a process runs.
#[derive(Debug, Clone, Default)]
pub struct Runner {
    #[cfg(unix)]
    iso: Option<(Client, String)>,
}

/// What a finished process left.
#[derive(Debug)]
pub struct Output {
    /// `None` when the code is unknown (a signal death off the helper path).
    pub code: Option<i32>,
    pub stdout: String,
    /// Empty through the helper (merged into `stdout`).
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    /// The text to show when the process failed: stderr, or the merged output.
    pub fn error_text(&self) -> &str {
        if self.stderr.is_empty() {
            &self.stdout
        } else {
            &self.stderr
        }
    }
}

/// An engine's line protocol on stdin/stdout.
pub struct Rpc {
    pub writer: Box<dyn AsyncWrite + Unpin + Send>,
    pub reader: Box<dyn AsyncRead + Unpin + Send>,
    pub killer: Killer,
}

/// Ends the process behind an [`Rpc`] (the engine always does, it is not expected to exit by itself).
pub struct Killer(Box<dyn FnOnce() + Send>);

impl Killer {
    pub fn kill(self) {
        (self.0)()
    }
}

impl Runner {
    /// The crew user runs the process (no helper).
    pub fn local() -> Self {
        Self::default()
    }

    /// The crewmate's own user runs it, through the helper.
    #[cfg(unix)]
    pub fn isolated(client: Client, user: String) -> Self {
        Self {
            iso: Some((client, user)),
        }
    }

    // Only the Unix isolation tests ask (`runner/isolation.rs`, `cfg(all(test, unix))`).
    #[cfg(all(test, unix))]
    pub fn is_isolated(&self) -> bool {
        self.iso.is_some()
    }

    /// Run `cmd` to the end and collect its output, giving up after `limit` (the process is ended).
    pub async fn output(&self, cmd: Cmd, limit: Duration) -> Result<Output, String> {
        #[cfg(unix)]
        if let Some((client, user)) = &self.iso {
            return output_isolated(client.clone(), user.clone(), cmd, limit).await;
        }
        output_local(cmd, limit).await
    }

    /// Start `cmd` with its stdin and stdout connected to the caller.
    pub async fn rpc(&self, cmd: Cmd) -> Result<Rpc, String> {
        #[cfg(unix)]
        if let Some((client, user)) = &self.iso {
            return rpc_isolated(client.clone(), user.clone(), cmd).await;
        }
        rpc_local(cmd)
    }
}

fn tokio_cmd(cmd: &Cmd) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(cmd.program);
    c.args(&cmd.args).current_dir(&cmd.cwd).envs(&cmd.env);
    c
}

async fn output_local(cmd: Cmd, limit: Duration) -> Result<Output, String> {
    let mut c = tokio_cmd(&cmd);
    c.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let out = tokio::time::timeout(limit, c.output())
        .await
        .map_err(|_| format!("{}: timed out after {}", cmd.program, human(limit)))?
        .map_err(|e| format!("{}: spawn failed: {e}", cmd.program))?;
    Ok(Output {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

fn rpc_local(cmd: Cmd) -> Result<Rpc, String> {
    let mut c = tokio_cmd(&cmd);
    c.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = c
        .spawn()
        .map_err(|e| format!("{}: spawn failed: {e}", cmd.program))?;
    let stdin = child.stdin.take().ok_or("no stdin pipe")?;
    let stdout = child.stdout.take().ok_or("no stdout pipe")?;
    Ok(Rpc {
        writer: Box::new(stdin),
        reader: Box::new(stdout),
        killer: Killer(Box::new(move || {
            // Start the kill and reap in the background: the caller is done with the process.
            tokio::spawn(async move {
                let _ = child.kill().await;
                let _ = child.wait().await;
            });
        })),
    })
}

fn human(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 && s.is_multiple_of(3600) {
        format!("{} h", s / 3600)
    } else {
        format!("{s} s")
    }
}

#[cfg(unix)]
fn wire_req(user: &str, cmd: &Cmd) -> SpawnReq {
    let mut argv = vec![cmd.program.to_string()];
    argv.extend(cmd.args.iter().cloned());
    SpawnReq {
        user: user.to_string(),
        argv,
        env: cmd.env.clone(),
        cwd: cmd.cwd.clone(),
        mode: Mode::Pipe,
        cols: 120,
        rows: 40,
    }
}

/// hostd's end of the helper's socketpair as an async stream.
#[cfg(unix)]
fn stream(fd: std::os::fd::OwnedFd) -> io::Result<tokio::net::UnixStream> {
    let s = std::os::unix::net::UnixStream::from(fd);
    s.set_nonblocking(true)?;
    tokio::net::UnixStream::from_std(s)
}

#[cfg(unix)]
async fn output_isolated(
    client: Client,
    user: String,
    cmd: Cmd,
    limit: Duration,
) -> Result<Output, String> {
    let program = cmd.program;
    let req = wire_req(&user, &cmd);
    let c2 = client.clone();
    let (sp, fd) = tokio::task::spawn_blocking(move || c2.spawn(&req))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{program}: spawn failed: {e}"))?;
    let mut s = stream(fd).map_err(|e| format!("{program}: {e}"))?;
    let mut buf = Vec::new();
    // The child's stdin is the same socket: nothing to send, and it must not wait for EOF on it
    // (headless engines do not read stdin), so only the read side matters.
    let read = tokio::time::timeout(limit, s.read_to_end(&mut buf)).await;
    if read.is_err() {
        let c = client.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let _ = c.signal(sp.session, libc::SIGKILL);
            let _ = c.reap(&user);
        })
        .await;
        return Err(format!("{program}: timed out after {}", human(limit)));
    }
    if let Ok(Err(e)) = read {
        // A reset when the child dies is the normal end of a socketpair that was not closed first.
        if e.kind() != io::ErrorKind::ConnectionReset {
            return Err(format!("{program}: read failed: {e}"));
        }
    }
    let code = tokio::task::spawn_blocking(move || client.wait_exit(sp.session))
        .await
        .map_err(|e| e.to_string())?;
    Ok(Output {
        code,
        stdout: String::from_utf8_lossy(&buf).into_owned(),
        stderr: String::new(),
    })
}

#[cfg(unix)]
async fn rpc_isolated(client: Client, user: String, cmd: Cmd) -> Result<Rpc, String> {
    let program = cmd.program;
    let req = wire_req(&user, &cmd);
    let c2 = client.clone();
    let (sp, fd) = tokio::task::spawn_blocking(move || c2.spawn(&req))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{program}: spawn failed: {e}"))?;
    let s = stream(fd).map_err(|e| format!("{program}: {e}"))?;
    let (r, w) = s.into_split();
    Ok(Rpc {
        writer: Box::new(w),
        reader: Box::new(r),
        killer: Killer(Box::new(move || {
            // Off the caller's thread: the helper answers quickly, but the caller may be in a runtime.
            std::thread::spawn(move || {
                let _ = client.signal(sp.session, libc::SIGKILL);
                // What the engine started (tools, language servers) dies with it.
                let _ = client.reap(&user);
            });
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the Unix rpc tests read and write lines.
    #[cfg(unix)]
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    fn cmd(program: &'static str, args: &[&str]) -> Cmd {
        let mut c = Cmd::new(program, "/");
        c.args(args.iter().map(|a| a.to_string()));
        c
    }

    #[tokio::test]
    async fn a_local_process_returns_code_stdout_and_stderr_separately() {
        // The platform shell: `cmd` on Windows, where there is no /bin/sh.
        let mut c = if cfg!(windows) {
            cmd("cmd", &["/d", "/c", "echo out& echo err 1>&2& exit /b 3"])
        } else {
            cmd("/bin/sh", &["-c", "echo out; echo err >&2; exit 3"])
        };
        c.env("X", "1");
        // `program` is 'static by design (engine names); a literal is.
        let o = Runner::local()
            .output(c, Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(o.code, Some(3));
        assert_eq!(o.stdout.trim(), "out");
        assert_eq!(o.stderr.trim(), "err");
        assert!(!o.success());
        assert_eq!(o.error_text().trim(), "err");
    }

    #[tokio::test]
    async fn a_local_process_that_outlives_the_limit_is_a_timeout_error() {
        // Something that runs for 30 s on each OS (Windows has no /bin/sleep; ping waits 1 s per echo).
        let c = if cfg!(windows) {
            cmd("ping", &["-n", "31", "127.0.0.1"])
        } else {
            cmd("/bin/sleep", &["30"])
        };
        let e = Runner::local()
            .output(c, Duration::from_millis(200))
            .await
            .unwrap_err();
        assert!(e.contains("timed out"), "{e}");
    }

    #[tokio::test]
    async fn a_missing_local_program_is_a_spawn_error() {
        let e = Runner::local()
            .output(cmd("/nonexistent/engine", &[]), Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(e.contains("spawn failed"), "{e}");
    }

    // A POSIX shell reading a line and then blocking: `cmd` has no equivalent that behaves the same
    // with a pipe for stdin and no console. Crew servers are Linux; the output test above covers
    // the local runner on Windows.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_local_rpc_is_a_line_protocol_and_kill_ends_it() {
        let rpc = Runner::local()
            .rpc(cmd("/bin/sh", &["-c", "read l; echo echo=$l; sleep 30"]))
            .await
            .unwrap();
        let Rpc {
            mut writer, reader, ..
        } = rpc;
        writer.write_all(b"hi\n").await.unwrap();
        let mut lines = BufReader::new(reader).lines();
        assert_eq!(lines.next_line().await.unwrap().unwrap(), "echo=hi");
    }

    #[test]
    fn human_durations_read_naturally() {
        assert_eq!(human(Duration::from_secs(7200)), "2 h");
        assert_eq!(human(Duration::from_secs(90)), "90 s");
    }

    #[cfg(unix)]
    mod isolated {
        use super::*;
        use crate::crew::spawn::testing::{USER, rig};

        fn run(r: &crate::crew::spawn::testing::Rig) -> Runner {
            Runner::isolated(r.client.clone(), USER.into())
        }

        fn c(r: &crate::crew::spawn::testing::Rig, fake: &str) -> Cmd {
            let mut c = Cmd::new("claude", &r.work().to_string_lossy());
            c.env("FAKE", fake).arg("-p").arg("a b; echo pwned");
            c
        }

        // The contract engines rely on: stdout and the exit code arrive as they would from a child of
        // hostd, with stderr merged into stdout (the one difference, which `error_text` hides).
        #[tokio::test(flavor = "multi_thread")]
        async fn an_isolated_process_returns_its_output_args_as_data_and_its_code() {
            let r = rig();
            r.engine("claude", "echo \"args=$*\"; echo err >&2; eval \"$FAKE\"");
            let o = run(&r)
                .output(c(&r, "exit 4"), Duration::from_secs(20))
                .await
                .unwrap();
            assert_eq!(o.code, Some(4));
            assert!(o.stdout.contains("args=-p a b; echo pwned"), "{}", o.stdout);
            assert!(o.stdout.contains("err"), "stderr is merged: {}", o.stdout);
            assert!(o.stderr.is_empty());
            assert!(o.error_text().contains("err"));
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn an_isolated_process_that_outlives_the_limit_is_ended_and_reported() {
            let r = rig();
            r.engine("claude", "echo up; /bin/sleep 30");
            let t = std::time::Instant::now();
            let e = run(&r)
                .output(c(&r, ""), Duration::from_millis(500))
                .await
                .unwrap_err();
            assert!(e.contains("timed out"), "{e}");
            assert!(t.elapsed() < Duration::from_secs(10));
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn a_refused_isolated_spawn_is_a_spawn_error() {
            let r = rig(); // no engine installed
            let e = run(&r)
                .output(c(&r, ""), Duration::from_secs(5))
                .await
                .unwrap_err();
            assert!(e.contains("spawn failed"), "{e}");
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn an_isolated_rpc_is_a_line_protocol_over_the_socketpair() {
            let r = rig();
            r.engine("claude", "read l; echo echo=$l; read m; echo echo=$m");
            let rpc = run(&r).rpc(c(&r, "")).await.unwrap();
            let Rpc {
                mut writer, reader, ..
            } = rpc;
            let mut lines = BufReader::new(reader).lines();
            writer.write_all(b"one\n").await.unwrap();
            assert_eq!(lines.next_line().await.unwrap().unwrap(), "echo=one");
            writer.write_all(b"two\n").await.unwrap();
            assert_eq!(lines.next_line().await.unwrap().unwrap(), "echo=two");
        }
    }
}
