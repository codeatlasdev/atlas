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
use super::hostd_reconcile::{
    check_report, interactive, parse_choice, reconcile_host, restart_host, snapshot,
};

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
    /// Report whether the running daemon is current (exit 0 current, 3 stale, 1 not running)
    #[arg(long, conflicts_with_all = ["stop", "status", "upgrade", "reconcile", "restart"])]
    pub check: bool,
    /// Bring the running daemon to this binary's version: live handoff when it can, a silent
    /// restart when no terminal runs, otherwise ask (never ends a terminal without consent)
    #[arg(long, conflicts_with_all = ["stop", "status", "upgrade", "restart"])]
    pub reconcile: bool,
    /// What `--reconcile` does when terminals would be lost and you are not asked
    #[arg(long, value_parser = ["ask", "idle", "later"], default_value = "ask", requires = "reconcile")]
    pub choice: String,
    /// Restart the daemon now, ending its terminals, and verify the new one
    #[arg(long, conflicts_with_all = ["stop", "status", "upgrade"])]
    pub restart: bool,
    /// Pause every crewmate: running tasks are cancelled, queues are held (`atlas crew pause` runs this on a server)
    #[arg(long, conflicts_with_all = ["stop", "status", "upgrade", "check", "reconcile", "restart", "takeover", "lab", "crew_resume", "mobile_pair"])]
    pub crew_pause: bool,
    /// Resume the crewmates after `--crew-pause`
    #[arg(long, conflicts_with_all = ["stop", "status", "upgrade", "check", "reconcile", "restart", "takeover", "lab", "crew_pause", "mobile_pair"])]
    pub crew_resume: bool,
    /// Issue a pairing invite for a phone (code, addresses, relay) and print it as one JSON object; waits for the
    /// relay address so the link works off the LAN (`atlas crew pair` runs this on a crew server)
    #[arg(long, conflicts_with_all = ["stop", "status", "upgrade", "check", "reconcile", "restart", "takeover", "lab", "crew_pause", "crew_resume", "crew_export", "crew_import"])]
    pub mobile_pair: bool,
    /// Write the crew archive (a .tar.gz of crewmates, memory, tasks, automations, usage) to stdout, reading the
    /// files directly (no daemon needed). `atlas crew export` runs this on the server
    #[arg(long, conflicts_with_all = ["stop", "status", "upgrade", "check", "reconcile", "restart", "takeover", "lab", "crew_pause", "crew_resume", "crew_import"])]
    pub crew_export: bool,
    /// Read a crew archive from stdin into this dir (`atlas crew import` runs this on the server)
    #[arg(long, conflicts_with_all = ["stop", "status", "upgrade", "check", "reconcile", "restart", "takeover", "lab", "crew_pause", "crew_resume", "crew_export"])]
    pub crew_import: bool,
    /// With `--crew-export`: only these crewmate ids (repeat); default every crewmate
    #[arg(long = "crewmate", value_name = "ID", requires = "crew_export")]
    pub crewmates: Vec<String>,
    /// With `--crew-export`: include secrets and webhook tokens
    #[arg(long, requires = "crew_export")]
    pub with_secrets: bool,
    /// With `--crew-import`: give crewmates whose id exists new ids instead of refusing
    #[arg(long, requires = "crew_import")]
    pub rename: bool,
    /// Sign this engine in as a crewmate, through the running hostd's helper (`atlas crew login --crewmate` runs this on
    /// a crew server over `ssh -t`): the same request the phone sends, the terminal carried to this one
    #[arg(long, value_name = "ENGINE", hide = true, requires = "login_as", conflicts_with_all = ["stop", "status", "upgrade", "check", "reconcile", "restart", "takeover", "lab", "crew_pause", "crew_resume", "mobile_pair", "crew_export", "crew_import"])]
    pub crew_login: Option<String>,
    /// With `--crew-login`: the crewmate's id
    #[arg(long, value_name = "ID", hide = true, requires = "crew_login")]
    pub login_as: Option<String>,
    /// Lab lane: run a SHADOW hostd that follows your real one (read-only) so a branch can be
    /// validated, with its own pairing and a Lab phone app, without touching your installation. Its
    /// files live in `<main dir>/lab` (override with --dir); combine with --stop/--status to manage it
    #[arg(long, conflicts_with_all = ["takeover", "upgrade", "check", "reconcile", "restart"])]
    pub lab: bool,
}

/// A pairing invite from the running daemon. The endpoint binds in the background on the first request and its
/// home relay is chosen a moment later, so ask again (each ask is a fresh code, only the last is returned)
/// until the invite carries the relay a phone off this network needs, for at most [`RELAY_WAIT`]; a daemon
/// without relays (`ATLAS_MOBILE_RELAY=0`) answers with its direct addresses once that time is up. An
/// invite with neither is an error: nothing could dial it.
async fn pair_invite(c: &HostClient) -> Result<Value> {
    const RELAY_WAIT: Duration = Duration::from_secs(8);
    let t0 = std::time::Instant::now();
    loop {
        let v = c.call("mobile_pair_start", json!({})).await?;
        let relay = v["relay_url"].as_str().is_some();
        let direct = v["addrs"].as_array().is_some_and(|a| !a.is_empty());
        if relay || (direct && t0.elapsed() >= RELAY_WAIT) {
            return Ok(v);
        }
        if t0.elapsed() >= RELAY_WAIT * 2 {
            return Err(anyhow!(
                "the pairing endpoint has no address a phone could dial (no relay, no LAN address): is this machine online?"
            ));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// The lab's own directory under the main hostd dir, unless `--dir` names one.
fn lab_dir(main: &std::path::Path, explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(|| main.join(atlas_host::lab::DIR_NAME))
}

impl AtlasCommand for HostdArgs {
    async fn run(self) -> Result<()> {
        let explicit_dir = self.dir.is_some();
        let main_dir = paths::base();
        // `--lab`: this command's dir is the lab's (`<main>/lab`); the main one is `$ATLAS_HOSTD_DIR`/`~/.atlas`.
        let dir = if self.lab {
            lab_dir(&main_dir, self.dir)
        } else {
            self.dir.unwrap_or(main_dir.clone())
        };
        // `ATLAS_MOBILE_RELAY=0`: Atlas Mobile on direct addresses only (LAN, tests).
        let no_mobile_relay = std::env::var("ATLAS_MOBILE_RELAY").is_ok_and(|v| v == "0");
        let crew_host = std::env::var("ATLAS_CREW_HOST").as_deref() == Ok("1");
        let (isolation, isolation_fault) = atlas_host::server::detect_isolation(crew_host);
        if let Some(sock) = self.takeover {
            return atlas_host::server::serve_with(
                &dir,
                atlas_host::server::ServeOpts {
                    takeover: Some(sock),
                    lab: None,
                    no_mobile_relay,
                    crew_host,
                    isolation,
                    isolation_fault,
                    registry: Some(atlas_host::server::desktop_registry()),
                    ..Default::default()
                },
            )
            .await;
        }
        let bundled = env!("CARGO_PKG_VERSION");
        let hint = if explicit_dir {
            format!(" --dir {}", dir.display())
        } else {
            String::new()
        };
        if self.check {
            return match snapshot(&dir).await {
                None => {
                    println!("Terminal host is not running ({}).", dir.display());
                    std::process::exit(1);
                }
                Some((_, s)) => match check_report(&s, bundled, &hint) {
                    None => {
                        println!("Terminal host {bundled} is current.");
                        Ok(())
                    }
                    Some(r) => {
                        println!("{r}");
                        std::process::exit(3);
                    }
                },
            };
        }
        if self.reconcile {
            reconcile_host(&dir, bundled, parse_choice(&self.choice), interactive()).await?;
            return Ok(());
        }
        if self.restart {
            restart_host(&dir, bundled).await?;
            return Ok(());
        }
        if self.crew_export {
            let mut out = std::io::stdout().lock();
            let m = atlas_host::crew::export_archive(
                &dir,
                &self.crewmates,
                self.with_secrets,
                &mut out,
            )?;
            // Stdout is the archive: the summary goes to stderr.
            eprintln!("exported {} crewmate(s)", m.crewmates.len());
            return Ok(());
        }
        if self.crew_import {
            let r = atlas_host::crew::import_archive(&dir, std::io::stdin().lock(), self.rename)?;
            for i in &r.imported {
                println!(
                    "imported {} ({}){}",
                    i.name,
                    i.new_id,
                    if i.old_id != i.new_id {
                        format!(" was {}", i.old_id)
                    } else {
                        String::new()
                    }
                );
            }
            return Ok(());
        }
        if let Some(engine) = &self.crew_login {
            let code = super::hostd_login::run(&dir, engine, self.login_as.as_deref()).await?;
            if code != 0 {
                std::process::exit(code);
            }
            return Ok(());
        }
        if self.mobile_pair {
            let c = HostClient::connect_dir(&dir, "cli")
                .await
                .map_err(|_| anyhow!("atlas hostd is not running ({})", dir.display()))?;
            println!("{}", pair_invite(&c).await?);
            return Ok(());
        }
        if self.crew_pause || self.crew_resume {
            let c = HostClient::connect_dir(&dir, "cli")
                .await
                .map_err(|_| anyhow!("atlas hostd is not running ({})", dir.display()))?;
            c.call(
                if self.crew_pause {
                    "crew_pause"
                } else {
                    "crew_resume"
                },
                json!({}),
            )
            .await?;
            println!(
                "crew {}",
                if self.crew_pause { "paused" } else { "resumed" }
            );
            return Ok(());
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
        atlas_host::server::serve_with(
            &dir,
            atlas_host::server::ServeOpts {
                takeover: None,
                no_mobile_relay,
                crew_host,
                isolation,
                isolation_fault,
                lab: self.lab.then_some(main_dir),
                registry: Some(atlas_host::server::desktop_registry()),
                ..Default::default()
            },
        )
        .await
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
