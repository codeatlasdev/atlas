//! Wire protocol: NDJSON over a unix socket, one multiplexed connection per client.
//!
//! - First client line: `{"hello":{"token":"<hex>","client":"desktop|cli|test","proto":2}}`;
//!   reply `{"ok":true,"data":{..info}}` or `{"ok":false,"error":"unauthorized"}` + close.
//! - Request `{"id":n,"cmd":"..","args":{..}}` → `{"id":n,"ok":true,"data":{..}}` |
//!   `{"id":n,"ok":false,"error":".."}`.
//! - Push (no `id`): `{"ev":"pty_output"|"pty_replay"|"pty_title"|"pty_activity"|"pty_exit"|"pty_dropped"|"fs_event",..}`.
//!   `pty_activity {pty}` goes to every attached connection (listeners too) at most once a second while the
//!   program prints; an older daemon simply never sends it.
//! - Every connection's outgoing queue is capped at 8 MiB (`pty::OUT_CAP`). A client that stops reading
//!   past that gets `{"ev":"pty_dropped","pty":"..","reason":"slow_client"}` and no more output for that pty
//!   (the pty reader and other clients are never held up); it re-sends `pty_attach` with `replay: true`
//!   (the last 2 MiB) and carries on. `pty_exit` and replies always get through. A broadcast (`lsp_*`,
//!   `agent_*`) skips a connection over the cap; that client re-syncs on reconnect.
//! - `pty_attach_raw` (first and only command on a connection): after its reply line the
//!   connection carries the pty's raw output bytes (replay first), no framing, until the pty
//!   exits (hostd closes) or the client closes. Terminal renderers use it: no JSON, no base64.
//! - `info` (and the handshake reply) reports `{version, proto, caps, pid, queued, ..}`; `queued` is
//!   `{total_bytes, cap_bytes, dropped, connections: [{conn, bytes}]}`: what is waiting to be written to each
//!   connection and how many clients were dropped for not reading. `caps` lists
//!   what this daemon can do (`pty_attach_raw`, `upgrade`); clients check it instead of
//!   guessing from error strings. A proto 1 daemon has no `caps` and cannot `upgrade`.
//! - `upgrade {exe}`: live takeover. hostd starts `exe hostd --takeover`, hands it every live pty
//!   master fd over a unix socket (`SCM_RIGHTS`) plus session state, and exits. Children keep
//!   running; clients reconnect and reattach. See `handoff`.
//! - `lsp_*` commands (cap `lsp`): language-server features. `lsp_open {path, language_id,
//!   text, version}` → `{server, status, capabilities}` (`status`: ready|missing|disabled|
//!   crashed; first open of a language may download its server); `lsp_change`, `lsp_close`,
//!   `lsp_save`, `lsp_request {path, method, params, rid?}` (raw LSP result; `$/capabilities`
//!   gives the server's capabilities), `lsp_cancel {id: rid}`, `lsp_status` → `{servers}`,
//!   `lsp_apply_edit_result {id, applied}`. `lsp_open` also takes `root` (the worktree that opened it) and `overrides` (the user's
//!   `lsp.<language>` settings: `command`, `args`, `initialization_options`, `settings`,
//!   `disabled`) and replies with `reason` (why `missing`/`crashed`: `{code, message, hint?,
//!   command?}`) and, for `disabled`, `supported` (languages that have a server).
//!   `lsp_catalog` (servers + what is installed), `lsp_install {name}`, `lsp_uninstall
//!   {name}`, `lsp_restart {name, root?}` and `lsp_log {name, lines?}` serve Settings →
//!   Languages and the status bar. Each status entry carries `label`, `version`, `source`
//!   (`path|managed|custom`), `log` and, once dead, `tail` (last stderr lines). Pushes to every connection: `lsp_diagnostics
//!   {path, version, diagnostics}`, `lsp_status {servers}` (full list on every change) and
//!   `lsp_apply_edit {id, edit}`. Positions are UTF-16 and paths absolute. Language servers
//!   are NOT handed over by `upgrade`: they die with the old daemon, so clients re-send
//!   `lsp_open` for every open document after reconnecting.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A field that is present (`null` included) is `Some`; only a missing one is `None` (via `#[serde(default)]`).
fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}

pub const PROTO: u32 = 2;
/// Capabilities reported by `info`; the desktop gates features on these.
/// Windows has no live `upgrade`: pty handles cannot move to a successor the way Unix passes
/// master fds, so clients restart the daemon when it has no running terminal (see
/// `upgrade::decide`).
pub fn caps(crew_host: bool) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = vec!["pty_attach_raw", "lsp", "agents"];
    #[cfg(unix)]
    v.push("upgrade");
    if crew_host {
        v.push("crew");
    }
    v
}
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub struct Hello {
    pub token: String,
    #[serde(default)]
    pub client: String,
    #[serde(default)]
    pub proto: u32,
}

#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub cmd: HostCmd,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Scope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyCreate {
    pub cwd: String,
    /// Empty = the user's login shell (`$SHELL -l`).
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub title: Option<String>,
    /// `pty` (terminal) or `pipe` (stdin/stdout line protocol, e.g. omp rpc-ui).
    #[serde(default)]
    pub mode: Mode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Pty,
    Pipe,
}

fn yes() -> bool {
    true
}
fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}
fn default_exec_timeout() -> u64 {
    60_000
}
fn default_lines() -> u32 {
    200
}
fn default_log_lines() -> usize {
    80
}
fn default_history_limit() -> usize {
    200
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd", content = "args", rename_all = "snake_case")]
pub enum HostCmd {
    PtyCreate(PtyCreate),
    PtyAttach {
        pty: String,
        #[serde(default)]
        replay: bool,
        /// `false`: title/exit events only (a raw stream carries the bytes).
        #[serde(default = "yes")]
        output: bool,
    },
    /// Turns the connection into a raw byte stream of this pty (see module docs).
    PtyAttachRaw {
        pty: String,
        #[serde(default)]
        replay: bool,
    },
    PtyDetach {
        pty: String,
    },
    PtyWrite {
        pty: String,
        data: String,
    },
    PtyResize {
        pty: String,
        cols: u16,
        rows: u16,
    },
    PtySignal {
        pty: String,
        signal: String,
    },
    PtyKill {
        pty: String,
    },
    PtyList {},
    PtyInfo {
        pty: String,
    },
    PtyRead {
        pty: String,
        #[serde(default = "default_lines")]
        lines: u32,
    },
    PtyClear {
        pty: String,
    },
    Exec {
        cwd: String,
        argv: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        stdin: Option<String>,
        #[serde(default = "default_exec_timeout")]
        timeout_ms: u64,
    },
    FsList {
        path: String,
    },
    FsRead {
        path: String,
        #[serde(default)]
        max: Option<u64>,
        /// Byte offset to start reading at (chunked reads of big files).
        #[serde(default)]
        offset: Option<u64>,
    },
    FsWrite {
        path: String,
        data: String,
        #[serde(default)]
        mode: Option<u32>,
    },
    FsStat {
        path: String,
    },
    FsMkdir {
        path: String,
    },
    FsRename {
        from: String,
        to: String,
    },
    FsRemove {
        path: String,
        #[serde(default)]
        recursive: bool,
    },
    FsWatch {
        path: String,
        id: String,
    },
    FsUnwatch {
        id: String,
    },
    Info {},
    Upgrade {
        /// The `atlas` binary to take over with (must be this user's own, e.g. the freshly
        /// installed `~/.atlas/bin/atlas`).
        exe: String,
    },
    Shutdown {},
    /// Open (or re-open after a reconnect) a document for language-server features.
    LspOpen {
        path: String,
        language_id: String,
        text: String,
        #[serde(default)]
        version: i64,
        /// The user's per-language customization (`lsp.<language>` settings).
        #[serde(default)]
        overrides: Option<Value>,
        /// Worktree root that opened the document: where a dependency file (`node_modules`, the cargo
        /// registry, Rust std) is served from, and the fallback when no project marker is found.
        #[serde(default)]
        root: Option<String>,
    },
    /// LSP `contentChanges` (incremental, UTF-16 positions), applied in order.
    LspChange {
        path: String,
        version: i64,
        changes: Vec<Value>,
    },
    LspClose {
        path: String,
    },
    LspSave {
        path: String,
        #[serde(default)]
        text: Option<String>,
    },
    /// Raw LSP request on the document's server; `$/capabilities` returns the server's
    /// capabilities. `rid` names the call for `lsp_cancel`.
    LspRequest {
        path: String,
        method: String,
        #[serde(default)]
        params: Value,
        #[serde(default)]
        rid: Option<String>,
    },
    LspCancel {
        id: String,
    },
    LspStatus {},
    /// Every known server with what is installed on this machine (never installs).
    LspCatalog {},
    /// Install or reinstall a managed server now (progress arrives as `lsp_status` pushes).
    LspInstall {
        name: String,
        /// Remove Atlas's managed copy first (Settings → Editor → Languages → Update).
        #[serde(default)]
        reinstall: bool,
    },
    /// Remove Atlas's managed copy of a server (PATH binaries are never touched).
    LspUninstall {
        name: String,
    },
    /// Restart a server (all roots, or only `root`); open documents are replayed.
    LspRestart {
        name: String,
        #[serde(default)]
        root: Option<String>,
    },
    /// Tail of a server's stderr log: `{path, text}`.
    LspLog {
        name: String,
        #[serde(default = "default_log_lines")]
        lines: usize,
    },
    /// The client's answer to a pushed `lsp_apply_edit`.
    LspApplyEditResult {
        id: String,
        applied: bool,
    },
    /// Remember which pane runs `agent` in `cwd` (fallback for hooks without a pane key).
    AgentBind {
        pane: String,
        agent: String,
        cwd: String,
    },
    /// The pane's process exited: back to Idle.
    AgentPaneExited {
        pane: String,
    },
    /// `{panes: {pane_key: status}, hook_port, pending_approvals: [agent_approval event]}`.
    AgentStatusAll {},
    /// `{agents: [{id, label, setup, state: ok|missing|unknown}]}`: installed agents and whether
    /// their hooks report to Atlas (`atlas setup <agent> --check`).
    AgentIntegrations {},
    /// Answer a pending permission prompt (`agent_approval` push) by its `req`.
    AgentApprove {
        pane: String,
        req: String,
        allow: bool,
    },
    /// Type `text` into the pane's terminal, then Enter, only while the agent is idle/done.
    /// With `steer` and an omp agent it is also allowed while the agent works: omp queues
    /// what is typed mid-turn as steering for the running turn.
    AgentSend {
        pane: String,
        text: String,
        #[serde(default)]
        steer: bool,
    },
    /// The pane's agent and every subagent it started, however deep, as flat nodes:
    /// `{nodes: [{id, parent, name, agent, model, status, task, duration_ms, tokens, cost,
    /// tool_count, now, report, updated_at}]}` (`agents::tree`). The root is `id: ""`.
    AgentTree {
        pane: String,
    },
    /// The agent's conversation from its session file as a timeline:
    /// `{items, messages, title, truncated, subagents}` (`agents::history`). Takes the pane,
    /// never a path: the file is the one hostd recorded for that pane; `sub` names one of
    /// its `subagents` to read that transcript instead.
    AgentHistory {
        pane: String,
        #[serde(default = "default_history_limit")]
        limit: usize,
        #[serde(default)]
        sub: Option<String>,
    },
    /// Interrupt the agent's running turn: Escape into its terminal, as a person would.
    AgentInterrupt {
        pane: String,
    },
    /// Start `agent` (a registry id: omp, claude, codex…) in a new terminal of project
    /// `project` (an id of `project_list`), optionally with a first prompt. The desktop owns
    /// tabs, so this goes through its control socket: `{pane}` of the new tab.
    AgentLaunch {
        project: String,
        agent: String,
        #[serde(default)]
        prompt: Option<String>,
    },
    /// Local projects of the desktop registry with their live service state.
    ProjectList {},
    /// One project's status, config, last log line per service and recent error count.
    ProjectDetail {
        id: String,
    },
    /// Service logs (`query` = search, case-insensitive substring or `/regex/`).
    ProjectLogs {
        id: String,
        #[serde(default)]
        service: Option<String>,
        #[serde(default)]
        lines: Option<u64>,
        #[serde(default)]
        query: Option<String>,
    },
    /// `op`: `restart` | `start` | `stop` one service, or `up` (start the stack detached).
    ProjectService {
        id: String,
        #[serde(default)]
        service: Option<String>,
        op: String,
    },
    /// Run an atlas.yaml action; ones with `confirm` need `confirmed: true`.
    ProjectAction {
        id: String,
        key: String,
        #[serde(default)]
        confirmed: bool,
    },
    /// New one-time pairing code + this host's iroh address (desktop only).
    MobilePairStart {},
    MobileDevices {},
    MobileRevoke {
        node_id: String,
    },
    MobileSetScopes {
        node_id: String,
        scopes: Vec<crate::mobile::Scope>,
    },
    CrewmateList {},
    CrewmateCreate {
        name: String,
        role: String,
        agent: String,
        approval: String,
        #[serde(default)]
        workdir: Option<String>,
        #[serde(default)]
        look: Option<crate::crew::store::CrewmateLook>,
        #[serde(default)]
        engine: Option<crate::crew::store::EngineConfig>,
        #[serde(default)]
        project: Option<crate::crew::store::ProjectRef>,
        /// Initial setup step; None = no setup needed.
        #[serde(default)]
        setup_step: Option<String>,
    },
    CrewmateUpdate {
        id: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        agent: Option<String>,
        #[serde(default)]
        approval: Option<String>,
        #[serde(default)]
        look: Option<crate::crew::store::CrewmateLook>,
        #[serde(default)]
        engine: Option<crate::crew::store::EngineConfig>,
        #[serde(default)]
        project: Option<crate::crew::store::ProjectRef>,
        /// `None` = don't touch; `Some(None)` = clear (setup done); `Some(Some(s))` = set step.
        /// serde's default maps `null` to `None` like a missing key, so "Looks good" (which sends `null`) never
        /// ended the setup: `present` keeps `null` as `Some(None)`.
        #[serde(default, deserialize_with = "present")]
        setup_step: Option<Option<String>>,
        /// `None` = don't touch; `Some(None)` = clear (use crew default); `Some(Some(true/false))` = set.
        #[serde(default, deserialize_with = "present")]
        short_replies: Option<Option<bool>>,
    },
    CrewmateDelete {
        id: String,
    },
    CrewmateTask {
        id: String,
        prompt: String,
        source: String,
        #[serde(default)]
        tz: Option<String>,
    },
    CrewmateTasks {
        id: String,
        #[serde(default = "crate::crew::store::default_task_limit")]
        limit: usize,
    },
    CrewmateCancel {
        id: String,
        task: String,
    },
    CrewmateMemory {
        id: String,
    },
    CrewmateMemorySet {
        id: String,
        text: String,
    },
    CrewmateRoutineSet {
        id: String,
        routines: Vec<serde_json::Value>,
    },
    PushRegister {
        token: String,
        platform: String,
    },
    PushUnregister {
        token: String,
    },
    CrewTelegramSet {
        #[serde(default)]
        token: Option<String>,
    },
    CrewTelegramLink {},
    /// Whether Telegram is configured and how many chats are linked. Never returns the token.
    CrewTelegramStatus {},
    MobileInvite {
        url: String,
        name: String,
    },
    MobileInvites {},
    MobileInviteDismiss {
        url: String,
    },
    CrewmateTriggersSet {
        id: String,
        triggers: Vec<serde_json::Value>,
    },
    CrewmateSecretSet {
        id: String,
        key: String,
        value: String,
    },
    CrewmateAnswer {
        id: String,
        ask_id: String,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        text: Option<String>,
    },
    CrewmateUsage {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        since: Option<u64>,
    },
    EngineLimits {},
    /// `gh auth status` without secrets. snake_case of `GitHub` would be wrong.
    #[serde(rename = "crew_github_status")]
    CrewGithubStatus {},
    /// Ensure gh installed, start `gh auth login --web`, return device code+URL.
    #[serde(rename = "crew_github_login_start")]
    CrewGithubLoginStart {},
    /// Poll running login: pending/done/error. On done configures git.
    #[serde(rename = "crew_github_login_poll")]
    CrewGithubLoginPoll {},
    /// `gh auth logout` and remove setup-git/insteadOf config.
    #[serde(rename = "crew_github_logout")]
    CrewGithubLogout {},
    /// Answer a waiting-action task (provide_secret, input, choose, sign_in_engine).
    #[serde(rename = "crewmate_action_answer")]
    CrewmateActionAnswer {
        id: String,
        task: String,
        /// For provide_secret: the secret value (written to crewmate secrets, never stored in task).
        /// For other kinds: the user's response text or chosen option.
        value: String,
    },
    /// If gh is already logged in, wake any connect_github waiters immediately.
    #[serde(rename = "crew_github_connect_resume")]
    CrewGithubConnectResume {},
    /// Read crew-level reply-summary settings.
    #[serde(rename = "crew_settings_get")]
    CrewSettingsGet {},
    /// Write crew-level reply-summary settings.
    #[serde(rename = "crew_settings_set")]
    CrewSettingsSet {
        short_replies: bool,
        #[serde(default)]
        short_replies_model: Option<String>,
    },
    /// The crew servers this Mac's desktop knows (`~/.atlas/desktop/crew-servers.json`), which
    /// Atlas Mobile reaches through this hostd (`crate::crew`).
    CrewServers {},
    /// Run `cmd` on crew server `server` through this hostd. A phone may only send what it
    /// could send this hostd directly, with the same scope (`mobile::required_scope`).
    CrewCall {
        server: String,
        cmd: String,
        #[serde(default)]
        args: Value,
    },
}

impl HostCmd {
    /// The command a `crew_call` carries, parsed like any request.
    pub fn crew_inner(cmd: &str, args: &Value) -> Result<HostCmd, String> {
        serde_json::from_value(serde_json::json!({"cmd": cmd, "args": args}))
            .map_err(|e| format!("bad crew request: {e}"))
    }
}

/// Build a response line.
pub fn reply(id: u64, res: Result<Value, String>) -> String {
    let v = match res {
        Ok(data) => serde_json::json!({"id": id, "ok": true, "data": data}),
        Err(e) => serde_json::json!({"id": id, "ok": false, "error": e}),
    };
    v.to_string()
}

/// The `pty_title` push for the title a `pty_attach` answer carries, or `None` when it has none. A
/// program that titles its terminal once, before a client attached (PowerShell does at start), would
/// otherwise never reach that client: the push went out while nobody listened.
pub fn title_event(pty: &str, attach: &Value) -> Option<Value> {
    let title = attach["title"].as_str().filter(|t| !t.is_empty())?;
    Some(serde_json::json!({"ev": "pty_title", "pty": pty, "title": title}))
}

/// The first client line: `{"hello":{token,client,proto}}`.
pub fn hello_line(token: &str, client: &str) -> String {
    serde_json::json!({"hello": {"token": token, "client": client, "proto": PROTO}}).to_string()
}

/// Write each queued line (plus `\n`) to `w` until the queue closes or the peer is gone;
/// returns the writer so a connection can be taken over afterwards.
pub(crate) async fn write_lines<W: tokio::io::AsyncWrite + Unpin>(
    mut rx: crate::pty::OutRx,
    mut w: W,
) -> W {
    use tokio::io::AsyncWriteExt;
    while let Some(mut l) = rx.recv().await {
        l.push('\n');
        if w.write_all(l.as_bytes()).await.is_err() {
            break;
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_commands_parse_under_their_wire_names() {
        let status: Request =
            serde_json::from_str(r#"{"id":1,"cmd":"crew_github_status","args":{}}"#).unwrap();
        assert!(matches!(status.cmd, HostCmd::CrewGithubStatus {}));
        let start: Request =
            serde_json::from_str(r#"{"id":2,"cmd":"crew_github_login_start","args":{}}"#).unwrap();
        assert!(matches!(start.cmd, HostCmd::CrewGithubLoginStart {}));
        let poll: Request =
            serde_json::from_str(r#"{"id":3,"cmd":"crew_github_login_poll","args":{}}"#).unwrap();
        assert!(matches!(poll.cmd, HostCmd::CrewGithubLoginPoll {}));
        let logout: Request =
            serde_json::from_str(r#"{"id":4,"cmd":"crew_github_logout","args":{}}"#).unwrap();
        assert!(matches!(logout.cmd, HostCmd::CrewGithubLogout {}));
    }

    #[test]
    fn crewmate_action_answer_and_resume_parse_under_wire_names() {
        let answer: Request = serde_json::from_str(
            r#"{"id":10,"cmd":"crewmate_action_answer","args":{"id":"bot1","task":"t1","value":"secret"}}"#,
        ).unwrap();
        assert!(matches!(answer.cmd, HostCmd::CrewmateActionAnswer { .. }));
        if let HostCmd::CrewmateActionAnswer { id, task, value } = answer.cmd {
            assert_eq!(id, "bot1");
            assert_eq!(task, "t1");
            assert_eq!(value, "secret");
        }
        let resume: Request =
            serde_json::from_str(r#"{"id":11,"cmd":"crew_github_connect_resume","args":{}}"#)
                .unwrap();
        assert!(matches!(resume.cmd, HostCmd::CrewGithubConnectResume {}));
    }

    #[test]
    fn parses_tagged_requests_with_defaults() {
        let r: Request = serde_json::from_str(
            r#"{"id":7,"cmd":"pty_create","args":{"cwd":"/tmp","argv":["sh"]}}"#,
        )
        .unwrap();
        assert_eq!(r.id, 7);
        match r.cmd {
            HostCmd::PtyCreate(c) => assert_eq!((c.cols, c.rows), (80, 24)),
            other => panic!("{other:?}"),
        }
        let r: Request = serde_json::from_str(r#"{"id":1,"cmd":"pty_list","args":{}}"#).unwrap();
        assert!(matches!(r.cmd, HostCmd::PtyList {}));
        assert!(serde_json::from_str::<Request>(r#"{"id":1,"cmd":"nope","args":{}}"#).is_err());
    }

    /// `root` is the worktree that opened the document (dependency files are served from it). It is
    /// optional on the wire: a desktop that predates it must keep working.
    #[test]
    fn lsp_open_carries_the_worktree_root_and_stays_optional() {
        let with: Request = serde_json::from_str(
            r#"{"id":1,"cmd":"lsp_open","args":{"path":"/p/a.rs","language_id":"rust","text":"","root":"/p"}}"#,
        )
        .unwrap();
        match with.cmd {
            HostCmd::LspOpen { root, .. } => assert_eq!(root.as_deref(), Some("/p")),
            other => panic!("{other:?}"),
        }
        let without: Request = serde_json::from_str(
            r#"{"id":2,"cmd":"lsp_open","args":{"path":"/p/a.rs","language_id":"rust","text":""}}"#,
        )
        .unwrap();
        match without.cmd {
            HostCmd::LspOpen { root, .. } => assert!(root.is_none()),
            other => panic!("{other:?}"),
        }
    }
}
