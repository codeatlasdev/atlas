//! Client of a running Atlas Desktop's control socket (`~/.atlas/desktop.sock`): the CLI's
//! `atlas wt|term|browser|file` and hostd (agents started from a phone) talk to it.

use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use serde_json::Value;

pub const NOT_RUNNING: &str = "Atlas Desktop is not running; launch it with `atlas desktop`";

/// True when an Atlas Desktop window owns the control socket.
pub fn running() -> bool {
    crate::ipc::connect_sync(&crate::paths::desktop_socket()).is_ok()
}

fn token() -> Result<String> {
    let p = crate::paths::desktop_token();
    let t = std::fs::read_to_string(&p)
        .with_context(|| format!("{}: {}", p.display(), NOT_RUNNING))?
        .trim()
        .to_string();
    anyhow::ensure!(!t.is_empty(), NOT_RUNNING);
    Ok(t)
}

/// One request; `data` of an `ok` reply, the desktop's `error` otherwise.
pub fn call(cmd: &str, args: &Value, timeout: Duration) -> Result<Value> {
    let sock = crate::paths::desktop_socket();
    let mut s = crate::ipc::connect_sync(&sock).map_err(|_| anyhow!(NOT_RUNNING))?;
    s.set_write_timeout(Some(Duration::from_secs(5)))?;
    s.set_read_timeout(Some(timeout.saturating_add(Duration::from_secs(5))))?;
    let token = token()?;
    writeln!(s, r#"{{"token":"{token}"}}"#)?;
    writeln!(
        s,
        r#"{{"cmd":{c},"args":{a}}}"#,
        c = serde_json::to_string(cmd)?,
        a = args
    )?;
    let reply: Value = serde_json::from_str(
        BufReader::new(s)
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
}
