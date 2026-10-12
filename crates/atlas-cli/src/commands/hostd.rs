//! `atlas hostd` (the execution daemon behind Atlas Desktop) and `atlas term`
//! (inspect/drive its terminals from a shell or an agent).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, anyhow};
use atlas_host::{HostClient, paths};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct HostdArgs {
    /// Base dir for socket/token/scrollback (default ~/.atlas, or $ATLAS_HOSTD_DIR)
    #[arg(long)]
    pub dir: Option<PathBuf>,
    /// Ask the running daemon to stop (kills its terminals)
    #[arg(long, conflicts_with = "status")]
    pub stop: bool,
    /// Print the running daemon's info and exit (1 when not running)
    #[arg(long)]
    pub status: bool,
    /// Start as the successor of the running daemon, receiving its terminals from this
    /// handoff socket (used by `upgrade`; not meant to be run by hand)
    #[arg(long, value_name = "SOCK", hide = true, conflicts_with_all = ["stop", "status", "upgrade"])]
    pub takeover: Option<PathBuf>,
    /// Replace the running daemon with this binary, keeping every terminal alive
    #[arg(long, conflicts_with_all = ["stop", "status"])]
    pub upgrade: bool,
}

impl AtlasCommand for HostdArgs {
    async fn run(self) -> Result<()> {
        let dir = self.dir.unwrap_or_else(paths::base);
        if let Some(sock) = self.takeover {
            return atlas_host::server::serve_with(
                &dir,
                atlas_host::server::ServeOpts {
                    takeover: Some(sock),
                },
            )
            .await;
        }
        if self.stop || self.status || self.upgrade {
            let c = HostClient::connect_dir(&dir, "cli")
                .await
                .map_err(|_| anyhow!("atlas hostd is not running ({})", dir.display()))?;
            if self.stop {
                c.call("shutdown", json!({})).await?;
                println!("atlas hostd stopped");
            } else if self.upgrade {
                let exe = std::env::current_exe()?;
                let r = c
                    .call("upgrade", json!({"exe": exe.to_string_lossy()}))
                    .await?;
                println!("atlas hostd upgraded; {} terminal(s) kept", r["handed_off"]);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&c.call("info", json!({})).await?)?
                );
            }
            return Ok(());
        }
        atlas_host::server::serve(&dir).await
    }
}

#[derive(Debug, Subcommand)]
pub enum TermCommands {
    /// List terminals
    Ls,
    /// Print the last lines of a terminal (ANSI stripped)
    Read {
        pty: String,
        #[arg(long, default_value_t = 200)]
        lines: u32,
    },
    /// Type text into a terminal
    Send {
        pty: String,
        text: String,
        /// Press Enter after the text
        #[arg(long)]
        enter: bool,
    },
    /// Stop a terminal (TERM, then KILL after 3 s) and drop its scrollback
    Kill { pty: String },
    /// Block until a terminal exits or goes idle
    Wait {
        pty: String,
        #[arg(long = "for", value_parser = ["exit", "idle"], default_value = "exit")]
        until: String,
        /// Give up after this many seconds (exit code 2)
        #[arg(long)]
        timeout: Option<u64>,
    },
}

#[derive(Debug, Args)]
pub struct TermArgs {
    #[command(subcommand)]
    pub command: TermCommands,
    /// Machine-readable output
    #[arg(long, global = true)]
    pub json: bool,
}

/// Quiet period after which a terminal counts as idle.
const IDLE_QUIET: Duration = Duration::from_secs(2);

impl AtlasCommand for TermArgs {
    async fn run(self) -> Result<()> {
        // With the app open, go through it: it routes to SSH hosts and knows agent state.
        if super::wt::desktop_running() {
            return self.via_desktop();
        }
        let c = HostClient::connect_dir(&paths::base(), "cli")
            .await
            .map_err(|_| anyhow!("atlas hostd is not running; open Atlas Desktop"))?;
        let out = |v: &Value, human: &dyn Fn(&Value)| {
            if self.json {
                println!("{v}");
            } else {
                human(v);
            }
        };
        match &self.command {
            TermCommands::Ls => {
                let v = c.call("pty_list", json!({})).await?;
                out(&v["ptys"], &|ptys| {
                    for p in ptys.as_array().into_iter().flatten() {
                        let state = match &p["exited"] {
                            Value::Null => "running".to_string(),
                            e => format!("exited {}", e["code"]),
                        };
                        let argv: Vec<&str> = p["argv"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect();
                        println!(
                            "{}  {:<10}  {}  {}  {}",
                            p["pty"].as_str().unwrap_or(""),
                            state,
                            p["cwd"].as_str().unwrap_or(""),
                            argv.join(" "),
                            p["title"].as_str().unwrap_or("")
                        );
                    }
                });
            }
            TermCommands::Read { pty, lines } => {
                let v = c
                    .call("pty_read", json!({"pty": pty, "lines": lines}))
                    .await?;
                out(&v, &|v| println!("{}", v["text"].as_str().unwrap_or("")));
            }
            TermCommands::Send { pty, text, enter } => {
                let mut s = text.clone();
                if *enter {
                    s.push('\r');
                }
                c.call("pty_write", json!({"pty": pty, "data": B64.encode(s)}))
                    .await?;
                out(&json!({}), &|_| {});
            }
            TermCommands::Kill { pty } => {
                c.call("pty_kill", json!({"pty": pty})).await?;
                out(&json!({}), &|_| {});
            }
            TermCommands::Wait {
                pty,
                until,
                timeout,
            } => {
                let fut = wait(&c, pty, until == "idle");
                let res = match timeout {
                    Some(s) => tokio::time::timeout(Duration::from_secs(*s), fut)
                        .await
                        .ok(),
                    None => Some(fut.await),
                };
                match res {
                    Some(r) => {
                        let v = r?;
                        out(&v, &|v| println!("{}", v["state"].as_str().unwrap_or("")));
                    }
                    None => {
                        out(&json!({"state": "timeout"}), &|_| eprintln!("timeout"));
                        std::process::exit(2);
                    }
                }
            }
        }
        Ok(())
    }
}

impl TermArgs {
    fn via_desktop(self) -> Result<()> {
        use super::wt::call;
        let t = Duration::from_secs(15);
        let print = |v: &Value, human: &dyn Fn(&Value)| {
            if self.json {
                println!("{v}");
            } else {
                human(v);
            }
        };
        match &self.command {
            TermCommands::Ls => {
                let v = call("terminal_list", json!({}), t)?;
                print(&v, &|v| {
                    for x in v.as_array().into_iter().flatten() {
                        println!(
                            "{}  {:8}  {}  {}",
                            x["pty"].as_str().unwrap_or(""),
                            x["state"].as_str().unwrap_or("-"),
                            x["worktree"].as_str().unwrap_or(""),
                            x["label"].as_str().unwrap_or("")
                        );
                    }
                });
            }
            TermCommands::Read { pty, lines } => {
                let v = call("terminal_read", json!({"pty": pty, "lines": lines}), t)?;
                print(&v, &|v| println!("{}", v["text"].as_str().unwrap_or("")));
            }
            TermCommands::Send { pty, text, enter } => {
                call(
                    "terminal_send",
                    json!({"pty": pty, "text": text, "enter": enter}),
                    t,
                )?;
                print(&json!({}), &|_| {});
            }
            TermCommands::Kill { pty } => {
                call("terminal_kill", json!({"pty": pty}), t)?;
                print(&json!({}), &|_| {});
            }
            TermCommands::Wait {
                pty,
                until,
                timeout,
            } => {
                let secs = timeout.unwrap_or(600);
                let v = call(
                    "terminal_wait",
                    json!({"pty": pty, "for": until, "timeout_s": secs}),
                    Duration::from_secs(secs + 10),
                );
                match v {
                    Ok(v) => print(&v, &|v| println!("{}", v["state"].as_str().unwrap_or(""))),
                    Err(e) if e.to_string().contains("timeout") => {
                        print(&json!({"state": "timeout"}), &|_| eprintln!("timeout"));
                        std::process::exit(2);
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(())
    }
}

async fn wait(c: &HostClient, pty: &str, idle: bool) -> Result<Value> {
    let mut ev = c.events();
    let info = c
        .call("pty_attach", json!({"pty": pty, "replay": false}))
        .await?;
    if let Some(code) = info["exited"].as_object() {
        return Ok(json!({"state": "exit", "code": code.get("code")}));
    }
    loop {
        let next = if idle {
            match tokio::time::timeout(IDLE_QUIET, ev.recv()).await {
                Ok(r) => r,
                Err(_) => return Ok(json!({"state": "idle"})),
            }
        } else {
            ev.recv().await
        };
        let Ok(e) = next else {
            return Err(anyhow!("hostd disconnected"));
        };
        if e["pty"] != pty && e["ev"] != "disconnected" {
            continue;
        }
        match e["ev"].as_str() {
            Some("pty_exit") => return Ok(json!({"state": "exit", "code": e["code"]})),
            Some("disconnected") => return Err(anyhow!("hostd disconnected")),
            _ => {}
        }
    }
}
