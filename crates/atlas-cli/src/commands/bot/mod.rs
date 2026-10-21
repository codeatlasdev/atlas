//! `atlas bot` — manage atlas bot computers.
//!
//! Subcommands:
//! - `install`   turn a Linux server into a bot computer
//! - `uninstall` remove the bot service
//! - `status`    show systemd and hostd status
//! - `logs`      stream journalctl output
//! - `login`     open an interactive agent sign-in session as atlas-bot

pub mod install;

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use super::AtlasCommand;
use super::deploy::remote::Remote;
use install::host_from_target;

#[derive(Debug, Args)]
pub struct BotArgs {
    #[command(subcommand)]
    pub command: BotCommands,
}

#[derive(Debug, Subcommand)]
pub enum BotCommands {
    /// Turn a Linux x86_64 server into a bot computer (installs atlas hostd as a service)
    Install(install::InstallArgs),
    /// Remove the atlas-bot service, user and binary from a server
    Uninstall(UninstallArgs),
    /// Show the bot service status on a server
    Status(StatusArgs),
    /// Stream the bot service logs from a server
    Logs(LogsArgs),
    /// Open an interactive agent sign-in session as atlas-bot on a server
    Login(LoginArgs),
}

impl AtlasCommand for BotArgs {
    async fn run(self) -> Result<()> {
        match self.command {
            BotCommands::Install(a) => a.run().await,
            BotCommands::Uninstall(a) => a.run().await,
            BotCommands::Status(a) => a.run().await,
            BotCommands::Logs(a) => a.run().await,
            BotCommands::Login(a) => a.run().await,
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
                "This will stop and remove the atlas-bot service, user and binary on {}.",
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
        let script = "\
            sudo systemctl stop atlas-bot 2>/dev/null || true; \
            sudo systemctl disable atlas-bot 2>/dev/null || true; \
            sudo rm -f /etc/systemd/system/atlas-bot.service; \
            sudo systemctl daemon-reload 2>/dev/null || true; \
            sudo userdel -r atlas-bot 2>/dev/null || true; \
            sudo rm -rf /usr/local/lib/atlas-bot";
        let o = r.exec(script).await?;
        if o.code != 0 {
            bail!("uninstall failed: {}", o.stderr.trim());
        }
        println!(
            "\x1b[32m✓\x1b[0m atlas-bot removed from {}.",
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
        let o = r
            .exec("systemctl is-active atlas-bot; systemctl status atlas-bot --no-pager -l 2>&1 || true")
            .await?;
        print!("{}", o.stdout);
        if !o.stderr.is_empty() {
            eprint!("{}", o.stderr);
        }

        // hostd status (as atlas-bot, non-fatal if not reachable)
        let o2 = r
            .exec(
                "sudo -u atlas-bot /usr/local/lib/atlas-bot/atlas hostd \
                 --status --dir /var/lib/atlas-bot/.atlas 2>&1 || true",
            )
            .await?;
        let hostd_out = o2.stdout.trim();
        if !hostd_out.is_empty() {
            println!("\nhostd:");
            println!("{hostd_out}");
        }
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
        let cmd = if self.follow {
            "journalctl -u atlas-bot --no-pager -f".to_string()
        } else {
            "journalctl -u atlas-bot -n 200 --no-pager".to_string()
        };
        let code = r.stream(&cmd).await?;
        if code != 0 {
            std::process::exit(code);
        }
        Ok(())
    }
}

// ── login ─────────────────────────────────────────────────────────────────────

/// Official one-liner to install each agent on the bot computer.
/// These match the install lines documented in docs/bots.md.
fn agent_install_hint(id: &str) -> Option<&'static str> {
    match id {
        "omp" => Some("curl -fsSL https://omp.sh/install | sh"),
        "claude" => Some("npm install -g @anthropic-ai/claude-code"),
        "codex" => Some("npm install -g @openai/codex"),
        "opencode" => Some("curl -fsSL https://opencode.ai/install | sh"),
        _ => None,
    }
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// SSH target for the bot host (e.g. user@hostname or hostname)
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

        if let Some(hint) = agent_install_hint(&self.agent) {
            println!(
                "\x1b[36m◆\x1b[0m Install {} on the bot computer first if needed:",
                self.agent
            );
            println!("    {hint}");
            println!();
        }

        println!("\x1b[36m▸\x1b[0m Opening sign-in session: ssh -t atlas-bot@{host} {bin}");
        let status = std::process::Command::new("ssh")
            .args(["-t", &format!("atlas-bot@{host}"), bin])
            .status()
            .context("could not launch ssh")?;

        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
        Ok(())
    }
}
