//! `atlas wt`, `atlas term`, `atlas browser`, `atlas file` — control a running
//! Atlas Desktop window over the control socket (`~/.atlas/desktop.sock`).

use std::io::{BufRead, BufReader, Write};
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use clap::{Args, Subcommand};
use serde_json::{Value, json};

use super::AtlasCommand;

pub const NOT_RUNNING: &str = "Atlas Desktop is not running; launch it with `atlas desktop`";

/// True when an Atlas Desktop window owns the control socket.
pub fn desktop_running() -> bool {
    atlas_core::ipc::connect_sync(&atlas_core::paths::desktop_socket()).is_ok()
}

fn token() -> Result<String> {
    let p = atlas_core::paths::desktop_token();
    let t = std::fs::read_to_string(&p)
        .with_context(|| format!("{}: {}", p.display(), NOT_RUNNING))?
        .trim()
        .to_string();
    anyhow::ensure!(!t.is_empty(), NOT_RUNNING);
    Ok(t)
}

pub fn call(cmd: &str, args: Value, timeout: Duration) -> Result<Value> {
    let sock = atlas_core::paths::desktop_socket();
    let mut s = atlas_core::ipc::connect_sync(&sock).map_err(|_| anyhow!(NOT_RUNNING))?;
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

fn jprint(v: &Value, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
        );
    } else {
        match v {
            Value::String(s) => println!("{s}"),
            Value::Array(a) => a.iter().for_each(|x| println!("{x}")),
            other => println!("{other}"),
        }
    }
}

// ---------- atlas wt ----------

#[derive(Debug, Subcommand)]
pub enum WtCommands {
    /// List worktrees
    Ls {
        #[arg(long)]
        repo: Option<String>,
    },
    /// Create a worktree and open it in Atlas Desktop
    Create {
        /// Project (name, id or local path; added automatically if absent)
        #[arg(long)]
        repo: String,
        #[arg(long, default_value = "")]
        name: String,
        #[arg(long)]
        base: Option<String>,
        /// Agent to launch (`omp`, `claude`, `codex`, `blank`)
        #[arg(long, default_value = "blank")]
        agent: String,
        #[arg(long)]
        prompt: Option<String>,
        /// Skip running the `worktree.setup` script
        #[arg(long)]
        no_setup: bool,
        /// Link to a GitHub PR number
        #[arg(long)]
        pr: Option<u64>,
    },
    /// Delete a worktree (by id, name, branch or path)
    Rm {
        id: String,
        #[arg(long)]
        force: bool,
    },
    /// Update name, unread or comment
    Set {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        unread: Option<bool>,
    },
    /// Resolve the worktree containing a path
    Current { path: Option<String> },
}

#[derive(Debug, Args)]
pub struct WtArgs {
    #[command(subcommand)]
    pub command: WtCommands,
    #[arg(long, global = true)]
    pub json: bool,
}

impl AtlasCommand for WtArgs {
    async fn run(self) -> Result<()> {
        let j = self.json;
        let timeout = Duration::from_secs(900);
        let v = match self.command {
            WtCommands::Ls { repo } => call(
                "worktree_list",
                json!({"repo": repo}),
                Duration::from_secs(10),
            )?,
            WtCommands::Create {
                repo,
                name,
                base,
                agent,
                prompt,
                no_setup,
                pr,
            } => {
                eprintln!("Creating worktree (this may take a minute)…");
                call(
                    "worktree_create",
                    json!({"repo": repo, "name": name, "base": base, "agent": agent,
                           "prompt": prompt, "setup": if no_setup {"skip"} else {"run"}, "pr": pr}),
                    timeout,
                )?
            }
            WtCommands::Rm { id, force } => call(
                "worktree_remove",
                json!({"id": id, "force": force}),
                Duration::from_secs(30),
            )?,
            WtCommands::Set {
                id,
                name,
                comment,
                unread,
            } => call(
                "worktree_set",
                json!({"id": id, "name": name, "comment": comment, "unread": unread}),
                Duration::from_secs(10),
            )?,
            WtCommands::Current { path } => {
                let cwd = path.unwrap_or_else(|| {
                    std::env::current_dir()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                call(
                    "worktree_current",
                    json!({"cwd": cwd}),
                    Duration::from_secs(10),
                )?
            }
        };
        jprint(&v, j);
        Ok(())
    }
}

// Atlas Desktop-side `atlas term` is implemented in hostd.rs; the CLI dispatcher
// re-exports that impl unchanged. `TermArgs` is pub there; just re-export it.
// ---------- atlas browser ----------

#[derive(Debug, Subcommand)]
pub enum BrowserCommands {
    /// Navigate the embedded browser (or open a new tab)
    Goto {
        url: String,
        #[arg(long)]
        tab: Option<String>,
        #[arg(long)]
        worktree: Option<String>,
    },
    /// Dump an accessibility-ish snapshot of the current page
    Snapshot {
        #[arg(long)]
        tab: Option<String>,
    },
    /// Click a page element by snapshot ref (@e1, @e2, …)
    Click {
        r#ref: String,
        #[arg(long)]
        tab: Option<String>,
    },
    /// Fill a form input by snapshot ref
    Fill {
        r#ref: String,
        text: String,
        #[arg(long)]
        tab: Option<String>,
    },
    /// Screenshot the visible page area (returns the file path)
    Screenshot {
        #[arg(long)]
        tab: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct BrowserArgs {
    #[command(subcommand)]
    pub command: BrowserCommands,
    #[arg(long, global = true)]
    pub json: bool,
}

impl AtlasCommand for BrowserArgs {
    async fn run(self) -> Result<()> {
        let j = self.json;
        let v = match self.command {
            BrowserCommands::Goto { url, tab, worktree } => call(
                "browser_goto",
                json!({"url": url, "tab": tab, "worktree": worktree}),
                Duration::from_secs(30),
            )?,
            BrowserCommands::Snapshot { tab } => call(
                "browser_snapshot",
                json!({"tab": tab}),
                Duration::from_secs(15),
            )?,
            BrowserCommands::Click { r#ref, tab } => call(
                "browser_click",
                json!({"ref": r#ref, "tab": tab}),
                Duration::from_secs(10),
            )?,
            BrowserCommands::Fill { r#ref, text, tab } => call(
                "browser_fill",
                json!({"ref": r#ref, "text": text, "tab": tab}),
                Duration::from_secs(10),
            )?,
            BrowserCommands::Screenshot { tab } => call(
                "browser_screenshot",
                json!({"tab": tab}),
                Duration::from_secs(15),
            )?,
        };
        jprint(&v, j);
        Ok(())
    }
}

// ---------- atlas file ----------

#[derive(Debug, Subcommand)]
pub enum FileCommands {
    /// Open a file in the editor tab
    Open {
        path: String,
        #[arg(long)]
        worktree: Option<String>,
    },
    /// Open the Diff tab for a worktree
    Diff {
        #[arg(long)]
        worktree: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct FileArgs {
    #[command(subcommand)]
    pub command: FileCommands,
    #[arg(long, global = true)]
    pub json: bool,
}

impl AtlasCommand for FileArgs {
    async fn run(self) -> Result<()> {
        let j = self.json;
        let v = match self.command {
            FileCommands::Open { path, worktree } => {
                let cwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let wt = worktree.unwrap_or(cwd);
                call(
                    "file_open",
                    json!({"path": path, "worktree": wt}),
                    Duration::from_secs(10),
                )?
            }
            FileCommands::Diff { worktree } => {
                let cwd = std::env::current_dir()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let wt = worktree.unwrap_or(cwd);
                call(
                    "diff_open",
                    json!({"worktree": wt}),
                    Duration::from_secs(10),
                )?
            }
        };
        jprint(&v, j);
        Ok(())
    }
}
