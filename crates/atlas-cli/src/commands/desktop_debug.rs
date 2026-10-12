//! `atlas desktop debug …`: client for the debug-build bridge in
//! `crates/atlas-desktop/src/debug.rs` (one JSON line per request over a unix socket).
//! Lets an agent read the webview console, evaluate JS and screenshot the window
//! without driving the user's mouse or keyboard.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use serde_json::{Value, json};

#[derive(Debug, Subcommand)]
pub enum DebugCmd {
    /// pid, current URL, visibility and native window id of the running dev app
    Info,
    /// Webview console, uncaught errors and CSP violations (level "csp")
    Logs {
        /// Only entries with seq >= this (print `seq` from a previous call to follow)
        #[arg(long, default_value_t = 0)]
        since: u64,
        /// Only this level: log|info|warn|error|debug|csp
        #[arg(long)]
        level: Option<String>,
    },
    /// Evaluate JS in the page and print the JSON result.
    /// An expression (`document.title`) or a body with `return` (`const n = 1; return n`).
    Eval { js: String },
    /// Screenshot the app window (no input is sent) and print the PNG path
    Shot {
        #[arg(default_value = "/tmp/atlas-desktop.png")]
        path: String,
    },
}

pub fn run(cmd: DebugCmd) -> Result<()> {
    let req = match &cmd {
        DebugCmd::Info => json!({ "cmd": "info" }),
        DebugCmd::Logs { since, .. } => json!({ "cmd": "logs", "since": since }),
        DebugCmd::Eval { js } => json!({ "cmd": "eval", "js": js }),
        // The app usually lacks Screen Recording permission; the terminal has it.
        DebugCmd::Shot { .. } => json!({ "cmd": "info" }),
    };
    let result = request(&req)?;
    match cmd {
        DebugCmd::Logs { level, .. } => {
            for l in result.as_array().into_iter().flatten() {
                if level.as_deref().is_some_and(|want| l["level"] != want) {
                    continue;
                }
                println!(
                    "{} {} [{}] {}",
                    l["seq"],
                    l["ts"].as_str().unwrap_or(""),
                    l["level"].as_str().unwrap_or(""),
                    l["msg"].as_str().unwrap_or("")
                );
            }
        }
        DebugCmd::Shot { path } => {
            let id = result["window_id"]
                .as_i64()
                .context("app reported no window id")?;
            if result["visible"] != true {
                bail!("the Atlas window is hidden; open it first (atlas desktop)");
            }
            let ok = std::process::Command::new("screencapture")
                .args(["-x", "-o", "-l", &id.to_string(), &path])
                .status()?
                .success();
            if !ok {
                bail!("screencapture failed — grant Screen Recording to this terminal");
            }
            println!("{path}");
        }
        _ => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}

fn request(req: &Value) -> Result<Value> {
    let path = atlas_core::paths::atlas_home().join("desktop-debug.sock");
    let stream = UnixStream::connect(&path).with_context(|| {
        format!(
            "no debug bridge at {} — start a debug build (`bun run desktop:dev` or `atlas link --desktop`)",
            path.display()
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    (&stream).write_all(format!("{req}\n").as_bytes())?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let reply: Value = serde_json::from_str(&line).context("bad reply from debug bridge")?;
    if reply["ok"] != true {
        bail!(
            "{}",
            reply["error"].as_str().unwrap_or("debug bridge error")
        );
    }
    Ok(reply["result"].clone())
}
