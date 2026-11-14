//! `atlas crew` — manage Atlas Crew servers (Linux machines that run crewmates).
//! (`atlas bot …` is the pre-rename name: a hidden alias, kept for scripts.)
//!
//! Subcommands:
//! - `install`   turn a Linux server into a crew server
//! - `uninstall` remove the crew service
//! - `status`    show systemd and hostd status
//! - `logs`      stream journalctl output
//! - `pause`     stop every crewmate's work and hold the queues; `resume` starts them again
//! - `login`     open an interactive agent sign-in session as the crew user
//!
//! `layout` has the names on the server and the migration of one installed as "Atlas Bot".

pub mod install;
mod layout;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use super::AtlasCommand;
use super::deploy::remote::Remote;
use install::host_from_target;

#[derive(Debug, Args)]
pub struct CrewArgs {
    #[command(subcommand)]
    pub command: CrewCommands,
}

#[derive(Debug, Subcommand)]
pub enum CrewCommands {
    /// Turn a Linux x86_64 server into a crew server (installs atlas hostd as a service)
    Install(install::InstallArgs),
    /// Remove the crew service, user and binary from a server
    Uninstall(UninstallArgs),
    /// Show the crew service status on a server
    Status(StatusArgs),
    /// Stream the crew service logs from a server
    Logs(LogsArgs),
    /// Pause every crewmate on a server: running tasks are cancelled, queues are held, automations skip
    Pause(PauseArgs),
    /// Resume the crewmates on a server after a pause
    Resume(PauseArgs),
    /// Open an interactive agent sign-in session as the crew user on a server
    Login(LoginArgs),
}

impl AtlasCommand for CrewArgs {
    async fn run(self) -> Result<()> {
        match self.command {
            CrewCommands::Install(a) => a.run().await,
            CrewCommands::Uninstall(a) => a.run().await,
            CrewCommands::Status(a) => a.run().await,
            CrewCommands::Logs(a) => a.run().await,
            CrewCommands::Pause(a) => a.run(true).await,
            CrewCommands::Resume(a) => a.run(false).await,
            CrewCommands::Login(a) => a.run().await,
        }
    }
}

// ── uninstall ─────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
    /// Skip the confirmation prompt
    #[arg(short, long)]
    pub yes: bool,
}

impl AtlasCommand for UninstallArgs {
    async fn run(self) -> Result<()> {
        if !self.yes {
            eprintln!(
                "This will stop and remove the crew service, user and binary on {}.",
                self.ssh_target
            );
            eprint!("Continue? [y/N] ");
            use std::io::BufRead as _;
            let line = std::io::BufReader::new(std::io::stdin())
                .lines()
                .next()
                .unwrap_or(Ok(String::new()))?;
            if !matches!(line.trim(), "y" | "Y" | "yes") {
                eprintln!("Aborted.");
                return Ok(());
            }
        }
        let r = Remote::new(&self.ssh_target);
        // userdel refuses while the crew user has processes (the desktop's SSH sessions). Report it instead of
        // killing them: a `pkill -KILL -u <user>` / `loginctl terminate-user` here coincided with the test server's
        // sshd going down, so uninstall never kills processes on someone's server.
        let o = r.exec(&layout::uninstall_script()).await?;
        if o.code != 0 {
            bail!("uninstall failed: {}", o.stderr.trim());
        }
        println!(
            "\x1b[32m✓\x1b[0m crew server removed from {}.",
            self.ssh_target
        );
        Ok(())
    }
}

// ── status ────────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
}

impl AtlasCommand for StatusArgs {
    async fn run(self) -> Result<()> {
        let r = Remote::new(&self.ssh_target);

        // systemd status
        let o = r.exec(&layout::status_script()).await?;
        print!("{}", o.stdout);
        if !o.stderr.is_empty() {
            eprint!("{}", o.stderr);
        }

        // hostd status (as the crew user; non-fatal if not reachable). The data dir is the user's home.
        let o2 = r
            .exec(&format!(
                "u=$({}); sudo -u \"$u\" {}/atlas hostd --status --dir \"/var/lib/$u/.atlas\" 2>&1 || true",
                layout::WHICH_USER,
                layout::INSTALL_DIR
            ))
            .await?;
        let hostd_out = o2.stdout.trim();
        if !hostd_out.is_empty() {
            println!("\nhostd:");
            println!("{hostd_out}");
        }
        Ok(())
    }
}

// ── pause / resume ────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct PauseArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
}

impl PauseArgs {
    async fn run(self, pause: bool) -> Result<()> {
        let o = Remote::new(&self.ssh_target)
            .exec(&layout::pause_script(pause))
            .await?;
        if o.code != 0 {
            let why = if o.stderr.trim().is_empty() {
                o.stdout.trim()
            } else {
                o.stderr.trim()
            };
            bail!(
                "could not {} the crew on {}: {why}",
                if pause { "pause" } else { "resume" },
                self.ssh_target
            );
        }
        println!(
            "\x1b[32m✓\x1b[0m crew on {} {}.",
            self.ssh_target,
            if pause { "paused" } else { "resumed" }
        );
        Ok(())
    }
}

// ── logs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct LogsArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
    /// Follow new log entries (like journalctl -f)
    #[arg(short, long)]
    pub follow: bool,
}

impl AtlasCommand for LogsArgs {
    async fn run(self) -> Result<()> {
        let r = Remote::new(&self.ssh_target);
        let code = r.stream(&layout::logs_script(self.follow)).await?;
        if code != 0 {
            std::process::exit(code);
        }
        Ok(())
    }
}

// ── login ─────────────────────────────────────────────────────────────────────

/// Official one-liner to install each agent on the crew server.
/// These match the install lines documented in docs/crew.md.
fn agent_install_hint(id: &str) -> Option<&'static str> {
    match id {
        "omp" => Some("curl -fsSL https://omp.sh/install | sh"),
        "claude" => Some("curl -fsSL https://claude.ai/install.sh | bash"),
        "codex" => Some(r#"npm_config_prefix="$HOME/.npm-global" npm install -g @openai/codex"#),
        "opencode" => Some("curl -fsSL https://opencode.ai/install | bash"),
        _ => None,
    }
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// SSH target for the crew server (e.g. user@hostname or hostname)
    pub ssh_target: String,
    /// Agent to sign in: omp, claude, codex, opencode
    pub agent: String,
}

impl AtlasCommand for LoginArgs {
    async fn run(self) -> Result<()> {
        let bin = atlas_core::agents::get(&self.agent)
            .map(|a| a.bin)
            .with_context(|| {
                format!(
                    "unknown agent '{}'; supported: omp, claude, codex, opencode",
                    self.agent
                )
            })?;
        let host = host_from_target(&self.ssh_target).to_owned();
        // The crew user differs on a server installed as "Atlas Bot": ask the host which one it has.
        let user = Remote::new(&self.ssh_target)
            .exec(layout::WHICH_USER)
            .await
            .ok()
            .filter(|o| o.code == 0)
            .map(|o| o.stdout.trim().to_owned())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| atlas_core::crew::USER.to_owned());

        if let Some(hint) = agent_install_hint(&self.agent) {
            println!(
                "\x1b[36m◆\x1b[0m Install {} on the crew server first if needed:",
                self.agent
            );
            println!("    {hint}");
            println!();
        }

        println!("\x1b[36m▸\x1b[0m Opening sign-in session: ssh -t {user}@{host} {bin}");
        let status = std::process::Command::new("ssh")
            .args(["-t", &format!("{user}@{host}"), bin])
            .status()
            .context("could not launch ssh")?;

        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
        Ok(())
    }
}
