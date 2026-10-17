//! Wire protocol: NDJSON over a unix socket, one multiplexed connection per client.
//!
//! - First client line: `{"hello":{"token":"<hex>","client":"desktop|cli|test","proto":2}}`;
//!   reply `{"ok":true,"data":{..info}}` or `{"ok":false,"error":"unauthorized"}` + close.
//! - Request `{"id":n,"cmd":"..","args":{..}}` → `{"id":n,"ok":true,"data":{..}}` |
//!   `{"id":n,"ok":false,"error":".."}`.
//! - Push (no `id`): `{"ev":"pty_output"|"pty_replay"|"pty_title"|"pty_activity"|"pty_exit"|"fs_event",..}`.
//!   `pty_activity {pty}` goes to every attached connection (listeners too) at most once a second while the
//!   program prints; an older daemon simply never sends it.
//! - `pty_attach_raw` (first and only command on a connection): after its reply line the
//!   connection carries the pty's raw output bytes (replay first), no framing, until the pty
//!   exits (hostd closes) or the client closes. Terminal renderers use it: no JSON, no base64.
//! - `info` (and the handshake reply) reports `{version, proto, caps, pid, ..}`. `caps` lists
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

pub const PROTO: u32 = 2;
/// Capabilities reported by `info`; the desktop gates features on these.
pub const CAPS: &[&str] = &["pty_attach_raw", "upgrade", "lsp"];
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
        /// Remove Atlas's managed copy first (Settings → Languages → Update).
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
}

/// Build a response line.
pub fn reply(id: u64, res: Result<Value, String>) -> String {
    let v = match res {
        Ok(data) => serde_json::json!({"id": id, "ok": true, "data": data}),
        Err(e) => serde_json::json!({"id": id, "ok": false, "error": e}),
    };
    v.to_string()
}

/// The first client line: `{"hello":{token,client,proto}}`.
pub fn hello_line(token: &str, client: &str) -> String {
    serde_json::json!({"hello": {"token": token, "client": client, "proto": PROTO}}).to_string()
}

/// Write each queued line (plus `\n`) to `w` until the queue closes or the peer is gone;
/// returns the writer so a connection can be taken over afterwards.
pub async fn write_lines<W: tokio::io::AsyncWrite + Unpin>(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<String>,
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
