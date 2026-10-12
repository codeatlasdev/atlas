//! Wire protocol: NDJSON over a unix socket, one multiplexed connection per client.
//!
//! - First client line: `{"hello":{"token":"<hex>","client":"desktop|cli|test","proto":2}}`;
//!   reply `{"ok":true,"data":{..info}}` or `{"ok":false,"error":"unauthorized"}` + close.
//! - Request `{"id":n,"cmd":"..","args":{..}}` → `{"id":n,"ok":true,"data":{..}}` |
//!   `{"id":n,"ok":false,"error":".."}`.
//! - Push (no `id`): `{"ev":"pty_output"|"pty_replay"|"pty_title"|"pty_exit"|"fs_event",..}`.
//! - `pty_attach_raw` (first and only command on a connection): after its reply line the
//!   connection carries the pty's raw output bytes (replay first), no framing, until the pty
//!   exits (hostd closes) or the client closes. Terminal renderers use it: no JSON, no base64.
//! - `info` (and the handshake reply) reports `{version, proto, caps, pid, ..}`. `caps` lists
//!   what this daemon can do (`pty_attach_raw`, `upgrade`); clients check it instead of
//!   guessing from error strings. A proto 1 daemon has no `caps` and cannot `upgrade`.
//! - `upgrade {exe}`: live takeover. hostd starts `exe hostd --takeover`, hands it every live pty
//!   master fd over a unix socket (`SCM_RIGHTS`) plus session state, and exits. Children keep
//!   running; clients reconnect and reattach. See `handoff`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTO: u32 = 2;
/// Capabilities reported by `info`; the desktop gates features on these.
pub const CAPS: &[&str] = &["pty_attach_raw", "upgrade"];
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
}

/// Build a response line.
pub fn reply(id: u64, res: Result<Value, String>) -> String {
    let v = match res {
        Ok(data) => serde_json::json!({"id": id, "ok": true, "data": data}),
        Err(e) => serde_json::json!({"id": id, "ok": false, "error": e}),
    };
    v.to_string()
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
}
