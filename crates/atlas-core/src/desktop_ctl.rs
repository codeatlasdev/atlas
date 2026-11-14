//! Client of a running Atlas Desktop's control socket (`~/.atlas/desktop.sock`): the CLI's
//! `atlas wt|term|browser|file` and hostd (agents started from a phone) talk to it.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use serde_json::Value;

pub const NOT_RUNNING: &str = "Atlas Desktop is not running; launch it with `atlas desktop`";

/// Where a desktop's control socket and token live. [`Endpoint::default`] is the user's own (`~/.atlas`);
/// tests (and hostd's) can point at another.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub socket: PathBuf,
    pub token: PathBuf,
}

impl Default for Endpoint {
    fn default() -> Self {
        Self {
            socket: crate::paths::desktop_socket(),
            token: crate::paths::desktop_token(),
        }
    }
}

/// Why a [`Endpoint::call`] did not get an answer from the desktop.
#[derive(Debug)]
pub enum CallError {
    /// No desktop owns the socket (no socket file, or the connection was refused): safe to do the work elsewhere.
    NotRunning,
    /// A desktop answered or broke mid-call: whatever it said, the work may have started. Never retried elsewhere.
    Failed(anyhow::Error),
}

impl Endpoint {
    /// True when an Atlas Desktop window owns the control socket.
    pub fn running(&self) -> bool {
        crate::ipc::connect_sync(&self.socket).is_ok()
    }

    fn read_token(&self) -> Result<String> {
        let t = std::fs::read_to_string(&self.token)
            .with_context(|| format!("{}: {}", self.token.display(), NOT_RUNNING))?
            .trim()
            .to_string();
        anyhow::ensure!(!t.is_empty(), NOT_RUNNING);
        Ok(t)
    }

    /// One request; `data` of an `ok` reply. [`CallError::NotRunning`] only when nothing listens on the socket.
    pub fn call_checked(
        &self,
        cmd: &str,
        args: &Value,
        timeout: Duration,
    ) -> std::result::Result<Value, CallError> {
        let mut s = crate::ipc::connect_sync(&self.socket).map_err(|_| CallError::NotRunning)?;
        let mut go = || -> Result<Value> {
            s.set_write_timeout(Some(Duration::from_secs(5)))?;
            s.set_read_timeout(Some(timeout.saturating_add(Duration::from_secs(5))))?;
            let token = self.read_token()?;
            writeln!(s, r#"{{"token":"{token}"}}"#)?;
            writeln!(
                s,
                r#"{{"cmd":{c},"args":{a}}}"#,
                c = serde_json::to_string(cmd)?,
                a = args
            )?;
            let reply: Value = serde_json::from_str(
                BufReader::new(&mut s)
                    .lines()
                    .next()
                    .ok_or_else(|| anyhow!("empty reply"))??
                    .as_str(),
            )?;
            if reply["ok"] == true {
                Ok(reply["data"].clone())
            } else {
                Err(anyhow!(
                    "{}",
                    reply["error"].as_str().unwrap_or("desktop error")
                ))
            }
        };
        go().map_err(CallError::Failed)
    }

    /// [`call_checked`](Self::call_checked) for callers that only want the message.
    pub fn call(&self, cmd: &str, args: &Value, timeout: Duration) -> Result<Value> {
        self.call_checked(cmd, args, timeout).map_err(|e| match e {
            CallError::NotRunning => anyhow!(NOT_RUNNING),
            CallError::Failed(e) => e,
        })
    }
}

/// True when an Atlas Desktop window owns the control socket.
pub fn running() -> bool {
    Endpoint::default().running()
}

/// One request; `data` of an `ok` reply, the desktop's `error` otherwise.
pub fn call(cmd: &str, args: &Value, timeout: Duration) -> Result<Value> {
    Endpoint::default().call(cmd, args, timeout)
}
