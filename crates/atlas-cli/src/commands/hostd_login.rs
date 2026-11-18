//! `atlas hostd --crew-login ENGINE --login-as ID`: a crewmate's engine sign-in from a person's terminal.
//!
//! It asks the running hostd for the engine's login terminal, the request the phone sends (`crew_engine_login`),
//! and carries that terminal to this one: the helper starts the engine as the crewmate, confined like its agents.
//! `atlas crew login --crewmate` runs this on the server over `ssh -t`; the CLI never starts the engine itself.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use atlas_host::HostClient;
use atlas_i18n::t;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The hostd command that opens the sign-in terminal, and its arguments: exactly what the phone sends.
pub(crate) fn login_request(engine: &str, crewmate: Option<&str>) -> (&'static str, Value) {
    (
        "crew_engine_login",
        json!({"engine": engine, "crewmate": crewmate}),
    )
}

/// The local terminal in raw mode for as long as this lives (only when stdin is one: a pipe has none).
struct RawMode(bool);

impl RawMode {
    fn enter() -> Self {
        use std::io::IsTerminal;
        Self(std::io::stdin().is_terminal() && crossterm::terminal::enable_raw_mode().is_ok())
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if self.0 {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

/// Open the sign-in terminal and carry it until the engine ends; the exit code is the engine's.
pub(crate) async fn run(dir: &Path, engine: &str, crewmate: Option<&str>) -> Result<i32> {
    let c = HostClient::connect_dir(dir, "cli")
        .await
        .map_err(|_| super::hostd::not_running(dir))?;
    let (cmd, args) = login_request(engine, crewmate);
    let opened = c.call(cmd, args).await?;
    let pty = opened["pty"]
        .as_str()
        .with_context(|| t!("cmd_hostd_login_no_terminal"))?
        .to_string();
    if let Ok((cols, rows)) = crossterm::terminal::size() {
        let _ = c
            .call(
                "pty_resize",
                json!({"pty": pty, "cols": cols, "rows": rows}),
            )
            .await;
    }
    let mut raw = atlas_host::client::attach_raw(dir, &pty, true).await?;
    let _raw_mode = RawMode::enter();
    let input = tokio::spawn({
        let (c, pty) = (c.clone(), pty.clone());
        async move {
            let mut stdin = tokio::io::stdin();
            let mut buf = [0u8; 1024];
            while let Ok(n) = stdin.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let data = B64.encode(&buf[..n]);
                if c.call("pty_write", json!({"pty": pty, "data": data}))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    });
    let mut out = tokio::io::stdout();
    let mut chunk = [0u8; 8192];
    // EOF when the engine exits (or hostd goes away).
    while let Ok(n) = raw.reader.read(&mut chunk).await {
        if n == 0 {
            break;
        }
        out.write_all(&chunk[..n]).await?;
        out.flush().await?;
    }
    input.abort();
    exit_code(&c, &pty).await
}

/// The engine's exit code. A terminal still running (the connection broke) is ended: nobody is left to answer it.
async fn exit_code(c: &HostClient, pty: &str) -> Result<i32> {
    for _ in 0..20 {
        let info = c.call("pty_info", json!({"pty": pty})).await?;
        if let Some(code) = info["exited"].as_object() {
            return Ok(code.get("code").and_then(Value::as_i64).unwrap_or(1) as i32);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = c.call("pty_kill", json!({"pty": pty})).await;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_host::protocol::HostCmd;

    const ID: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";

    // Security review 4, C2: the CLI's sign-in must be the request hostd already serves the phone with, parsed by the
    // real consumer, with the crewmate in it and never a fallback to the crew user's own sign-in.
    #[test]
    fn the_login_request_is_the_one_hostd_serves_the_phone_with() {
        let parse = |(cmd, args): (&str, Value)| {
            serde_json::from_value::<HostCmd>(json!({"cmd": cmd, "args": args})).unwrap()
        };
        assert!(matches!(
            parse(login_request("codex", Some(ID))),
            HostCmd::CrewEngineLogin { engine, crewmate } if engine == "codex" && crewmate.as_deref() == Some(ID)
        ));
        assert!(matches!(
            parse(login_request("claude", None)),
            HostCmd::CrewEngineLogin { crewmate: None, .. }
        ));
    }
}
