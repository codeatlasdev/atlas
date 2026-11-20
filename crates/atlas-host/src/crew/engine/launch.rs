//! How an agent turn is launched, in one place: what every engine gets (`TurnCtx`), the process it is
//! described as (`command`: cwd, crewmate secrets, then hostd's own `ATLAS_*` set), where it runs
//! (`Exec`: as the crew user, or as the crewmate's own Linux user through the spawn helper, issue #200)
//! and how it is waited for (`output`, `rpc`: cancel, timeout, tree kill). The one description of the
//! atlas MCP server that each engine renders in its own config language is `McpServer`.
//!
//! Everything an engine adds is data (`Cmd`); nothing outside this file knows which executor ran it.
//! Through the helper a process gets one end of a socketpair as stdin and stdout with stderr merged into
//! stdout: there is no separate stderr, so on failure the error text is the output.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(unix)]
use super::turn_timeout;
use super::{ToolSink, TurnCancel, TurnProc};
use crate::crew::store::{CrewmateSession, EngineConfig};

#[cfg(unix)]
use crate::crew::spawn::client::Client;
#[cfg(unix)]
use crate::crew::spawn::protocol::{Mode, SpawnReq};

/// One process to run: what an engine adds to [`TurnCtx::command`] before [`output`] or [`rpc`] start it.
#[derive(Debug, Clone)]
pub struct Cmd {
    /// An engine binary: a bare name (`claude`) that the helper resolves in its pinned directories and
    /// the crew user finds on its `PATH`, or (tests) a path.
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: String,
}

impl Cmd {
    pub fn new(program: &str, cwd: &str) -> Self {
        Self {
            program: program.to_string(),
            args: vec![],
            env: BTreeMap::new(),
            cwd: cwd.to_string(),
        }
    }

    pub fn arg(&mut self, a: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        self.args.push(a.as_ref().to_string_lossy().into_owned());
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

/// Where a turn's processes run.
#[derive(Debug, Clone, Default)]
pub struct Exec {
    #[cfg(unix)]
    iso: Option<(Client, String)>,
}

impl Exec {
    /// The crew user runs the process (no helper): how every crewmate ran before isolation.
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

    /// Whether the process runs as the crewmate's own user (through the helper).
    pub fn is_isolated(&self) -> bool {
        #[cfg(unix)]
        {
            self.iso.is_some()
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
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

/// Everything one turn needs, whatever the engine.
pub struct TurnCtx<'a> {
    pub engine: &'a EngineConfig,
    pub session: CrewmateSession,
    pub prompt: &'a str,
    /// Where the engine writes what its agent reads (mcp config, codex output file, omp extension):
    /// `<store>/crew/<id>/`, or `crew-runtime/` under isolation, where the agent can read it but not
    /// change it.
    pub crewmate_dir: &'a Path,
    pub crewmate_id: &'a str,
    pub hook_port: u16,
    pub hook_token: &'a str,
    /// PTY pane key for permission prompts (`crew:<id>:<task_id>`).
    pub pane_key: &'a str,
    /// Crewmate-scoped secrets, forwarded as env vars (hostd's `ATLAS_*` always win over them).
    pub extra_env: &'a BTreeMap<String, String>,
    pub work_dir: &'a str,
    /// Told of every finished tool call, for engines whose stream carries them (omp RPC).
    pub tools: Option<&'a ToolSink<'a>>,
    /// Fires when the task is cancelled or the crew paused: the turn's process tree ends.
    pub cancel: &'a TurnCancel,
    /// Where the turn's processes run.
    pub exec: &'a Exec,
}

impl TurnCtx<'_> {
    /// The env hostd owns for this turn. The one list of these names and values: the agent, the
    /// approval extension and the MCP server all read them, so a crewmate secret must never replace them.
    pub fn atlas_env(&self) -> [(&'static str, String); 5] {
        [
            ("ATLAS_HOOK_PORT", self.hook_port.to_string()),
            ("ATLAS_HOOK_TOKEN", self.hook_token.to_string()),
            ("ATLAS_CREWMATE_ID", self.crewmate_id.to_string()),
            ("ATLAS_PANE_KEY", self.pane_key.to_string()),
            ("ATLAS_HOOK_DECISION_SECS", "1800".to_string()),
        ]
    }

    /// `program` in the work dir with the crewmate's secrets, then hostd's `ATLAS_*` set on top (set
    /// last, so they win). The one place this is built, for the crew user and the helper alike.
    pub fn command(&self, program: &str) -> Cmd {
        let mut cmd = Cmd::new(program, self.work_dir);
        for (k, v) in self.extra_env {
            cmd.env(k, v);
        }
        for (k, v) in self.atlas_env() {
            cmd.env(k, v);
        }
        cmd
    }

    /// The atlas MCP server for this crewmate (`atlas mcp --crewmate <id>`), described once.
    pub fn mcp_server(&self) -> McpServer {
        McpServer {
            command: std::env::current_exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "atlas".into()),
            args: vec!["mcp".into(), "--crewmate".into(), self.crewmate_id.into()],
            // What `crew/mcp.rs` reads; the decision timeout is the agent's, not the server's.
            env: self
                .atlas_env()
                .into_iter()
                .filter(|(k, _)| *k != "ATLAS_HOOK_DECISION_SECS")
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }
}

// tree-access: reached only for a crewmate that is not isolated; an isolated one runs through the spawn helper (`output_isolated`)
fn tokio_cmd(cmd: &Cmd) -> tokio::process::Command {
    // tree-access: reached only for a crewmate that is not isolated; an isolated one runs through the spawn helper (`output_isolated`)
    let mut c = tokio::process::Command::new(&cmd.program);
    c.args(&cmd.args).current_dir(&cmd.cwd).envs(&cmd.env);
    c
}

/// Run `cmd` as a turn (own process group, tree killed on drop, cancel and timeout) and return what it
/// printed. A process that cannot start is `"<name>: spawn failed: …"`. Whether a non-zero exit is an
/// error is the engine's call (claude still parses a result it printed).
pub async fn output(
    exec: &Exec,
    cmd: Cmd,
    cancel: &TurnCancel,
    name: &str,
) -> Result<Output, String> {
    #[cfg(unix)]
    if let Some((client, user)) = &exec.iso {
        return output_isolated(client, user, &cmd, cancel, name).await;
    }
    let _ = exec;
    let mut c = tokio_cmd(&cmd);
    c.stdout(std::process::Stdio::piped());
    c.stderr(std::process::Stdio::piped());
    let out = TurnProc::spawn(&mut c)
        .map_err(|e| format!("{name}: spawn failed: {e}"))?
        .output(cancel, name)
        .await?;
    Ok(Output {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// The error for a turn whose process exited non-zero: `"<name>: exited <code>: <stderr>"`.
pub fn exit_error(name: &str, out: &Output) -> String {
    format!(
        "{name}: exited {}: {}",
        out.code.unwrap_or(-1),
        out.error_text()
    )
}

/// An engine's line protocol on stdin/stdout (omp RPC). Dropping it ends the process and everything
/// it started: the local arm kills the process group, the helper's arm signals the session's group.
pub struct Rpc {
    pub writer: Box<dyn AsyncWrite + Unpin + Send>,
    pub reader: Box<dyn AsyncRead + Unpin + Send>,
    /// Ends the process tree when dropped.
    pub guard: Box<dyn Send>,
    /// Asked once the stream has ended (the process is gone): the memory cap the kernel killed it under
    /// (#296), if it did. Only a process started through the helper has one.
    pub oom: Option<Box<dyn FnOnce() -> Option<u64> + Send>>,
}

/// Start `cmd` with its stdin and stdout connected to the caller. The process is not expected to exit
/// by itself: the engine drops the handle when the turn is over.
pub async fn rpc(exec: &Exec, cmd: Cmd, name: &str) -> Result<Rpc, String> {
    #[cfg(unix)]
    if let Some((client, user)) = &exec.iso {
        return rpc_isolated(client, user, &cmd, name).await;
    }
    let _ = exec;
    let mut c = tokio_cmd(&cmd);
    c.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut proc = TurnProc::spawn(&mut c).map_err(|e| format!("{name}: spawn failed: {e}"))?;
    let stdin = proc
        .child()
        .stdin
        .take()
        .ok_or_else(|| format!("{name}: no stdin pipe"))?;
    let stdout = proc
        .child()
        .stdout
        .take()
        .ok_or_else(|| format!("{name}: no stdout pipe"))?;
    Ok(Rpc {
        writer: Box::new(stdin),
        reader: Box::new(stdout),
        // `TurnProc` kills the whole group when dropped.
        guard: Box::new(proc),
        oom: None,
    })
}

/// The turn timeout as text for an error ("2 h", or "90 s" for a test's shortened one). Only the
/// helper's arm words it (the local arm's `TurnProc` has its own message).
#[cfg(unix)]
fn timeout_text() -> String {
    let s = turn_timeout().as_secs();
    if s >= 3600 && s.is_multiple_of(3600) {
        format!("{} h", s / 3600)
    } else {
        format!("{s} s")
    }
}

#[cfg(unix)]
fn wire_req(user: &str, cmd: &Cmd) -> SpawnReq {
    let mut argv = vec![cmd.program.clone()];
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
fn stream(fd: std::os::fd::OwnedFd) -> std::io::Result<tokio::net::UnixStream> {
    let s = std::os::unix::net::UnixStream::from(fd);
    s.set_nonblocking(true)?;
    tokio::net::UnixStream::from_std(s)
}

/// A helper session that is ended (SIGKILL of its process group) when this is dropped: the helper's
/// counterpart of `TurnProc`, so a turn future dropped by a cancel or a timeout can never leave the agent,
/// or the tools it started, running as another user and spending. Like `TurnProc` it does not reach a tool
/// that left the group with `setsid`; and it never uses `Reap`, which ends every process of the crewmate,
/// a concurrent terminal agent of the same crewmate included.
#[cfg(unix)]
struct Session {
    client: Client,
    session: u64,
    live: bool,
}

#[cfg(unix)]
impl Session {
    /// The process is done by itself: its tree still goes (a tool it left behind must not outlive the turn).
    fn end(&mut self) {
        if !std::mem::replace(&mut self.live, false) {
            return;
        }
        let (client, session) = (self.client.clone(), self.session);
        // Off the caller's thread: the helper answers quickly, but this may run in a runtime.
        std::thread::spawn(move || {
            let _ = client.signal(session, libc::SIGKILL);
        });
    }
}

#[cfg(unix)]
impl Drop for Session {
    fn drop(&mut self) {
        self.end();
    }
}

#[cfg(unix)]
async fn spawn_isolated(
    client: &Client,
    user: &str,
    cmd: &Cmd,
    name: &str,
) -> Result<(Session, tokio::net::UnixStream), String> {
    let req = wire_req(user, cmd);
    let c = client.clone();
    let (sp, fd) = tokio::task::spawn_blocking(move || c.spawn(&req))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{name}: spawn failed: {e}"))?;
    let session = Session {
        client: client.clone(),
        session: sp.session,
        live: true,
    };
    let s = stream(fd).map_err(|e| format!("{name}: {e}"))?;
    Ok((session, s))
}

#[cfg(unix)]
async fn output_isolated(
    client: &Client,
    user: &str,
    cmd: &Cmd,
    cancel: &TurnCancel,
    name: &str,
) -> Result<Output, String> {
    use tokio::io::AsyncReadExt;
    let (mut session, mut s) = spawn_isolated(client, user, cmd, name).await?;
    let mut buf = Vec::new();
    // The child's stdin is the same socket: nothing to send, and it must not wait for EOF on it
    // (headless engines do not read stdin), so only the read side matters.
    let read = tokio::select! {
        r = s.read_to_end(&mut buf) => Ok(r),
        () = cancel.fired() => Err(format!("{name}: cancelled")),
        () = tokio::time::sleep(turn_timeout()) => Err(format!("{name}: timed out after {}", timeout_text())),
    };
    match read {
        // Dropping `session` signals the process group.
        Err(e) => Err(e),
        Ok(r) => {
            // A reset when the child dies is the normal end of a socketpair that was not closed first.
            if let Err(e) = r
                && e.kind() != std::io::ErrorKind::ConnectionReset
            {
                return Err(format!("{name}: read failed: {e}"));
            }
            let (c, sid) = (session.client.clone(), session.session);
            let code = tokio::task::spawn_blocking(move || c.wait_exit(sid))
                .await
                .map_err(|e| e.to_string())?;
            // The kernel killed the engine for memory: whatever it printed before is a partial turn, and
            // "exited 137" does not say which setting to raise (#296).
            let c = session.client.clone();
            if let Ok(Ok((_, _, Some(cap)))) =
                tokio::task::spawn_blocking(move || c.wait_oom(sid)).await
            {
                session.end();
                return Err(format!(
                    "{name}: {}",
                    crate::crew::spawn::cgroup::oom_reason(cap)
                ));
            }
            // The CLI is done, but a tool it left behind must not outlive the turn.
            session.end();
            Ok(Output {
                code,
                stdout: String::from_utf8_lossy(&buf).into_owned(),
                stderr: String::new(),
            })
        }
    }
}

/// The cap session `sid` was OOM-killed under, once the helper has collected its exit (it does that every
/// 100 ms, so right after the stream ended it may still be "running"; a process that is really still alive
/// is never OOM-killed, so waiting about a second is enough to tell). Blocking.
#[cfg(unix)]
fn oom_cap_of(client: &Client, sid: u64) -> Option<u64> {
    for _ in 0..10 {
        match client.wait_oom(sid) {
            Ok((true, _, cap)) => return cap,
            Ok((false, _, _)) => std::thread::sleep(std::time::Duration::from_millis(100)),
            Err(_) => return None,
        }
    }
    None
}

#[cfg(unix)]
async fn rpc_isolated(client: &Client, user: &str, cmd: &Cmd, name: &str) -> Result<Rpc, String> {
    let (session, s) = spawn_isolated(client, user, cmd, name).await?;
    let (r, w) = s.into_split();
    let (client, sid) = (session.client.clone(), session.session);
    Ok(Rpc {
        writer: Box::new(w),
        reader: Box::new(r),
        guard: Box::new(session),
        oom: Some(Box::new(move || oom_cap_of(&client, sid))),
    })
}

/// One MCP stdio server, engine-neutral. Each engine renders it in its own config language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl McpServer {
    /// claude's `--mcp-config` file: `{"mcpServers": {"atlas": {command, args, env}}}`.
    pub fn claude_json(&self) -> Value {
        json!({ "mcpServers": { "atlas": {
            "command": self.command, "args": self.args, "env": self.env,
        } } })
    }

    /// codex's per-run `-c key=value` overrides (values are TOML). `default_tools_approval_mode =
    /// "approve"` is required: `codex exec` has nobody to ask, so without it every call of this
    /// server is cancelled with "user cancelled MCP tool call" (openai/codex#29857, #24135).
    pub fn codex_overrides(&self) -> Vec<String> {
        let toml = |s: &str| Value::String(s.to_string()).to_string();
        let args: Vec<String> = self.args.iter().map(|a| toml(a)).collect();
        let env: Vec<String> = self
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", toml(k), toml(v)))
            .collect();
        let mut out = vec![];
        for (key, value) in [
            ("command", toml(&self.command)),
            ("args", format!("[{}]", args.join(","))),
            ("env", format!("{{{}}}", env.join(","))),
            ("default_tools_approval_mode", toml("approve")),
        ] {
            out.push("-c".to_string());
            out.push(format!("mcp_servers.atlas.{key}={value}"));
        }
        out
    }

    /// opencode's config with the atlas server as `mcp.atlas`: `type: "local"`, `command: [..]`,
    /// `environment: {..}`. This is the V1 shape on purpose: V2 reads it natively ("Supported V1
    /// syntax remains quiet", opencode.ai/docs/migrate-v1), while V1 cannot read V2's `mcp.servers`
    /// (it would see a server named "servers" and run without the crew tools, silently). One shape
    /// that both majors load needs no version probe. Passed as `OPENCODE_CONFIG_CONTENT`, which
    /// opencode merges after the project's own config and never writes into the user's repo.
    pub fn opencode_config(&self) -> Value {
        let mut command = vec![self.command.clone()];
        command.extend(self.args.iter().cloned());
        json!({ "mcp": { "atlas": {
            "type": "local", "command": command, "environment": self.env,
        } } })
    }
}
