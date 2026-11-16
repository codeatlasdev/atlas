//! `atlas crew` — manage Atlas Crew servers (Linux machines that run crewmates).
//! (`atlas bot …` is the pre-rename name: a hidden alias, kept for scripts.)
//!
//! Subcommands:
//! - `install`   turn a Linux server into a crew server
//! - `update`    bring a crew server's atlas to a release, rolling back if hostd does not come back
//! - `uninstall` remove the crew service (offers an export first; never silent about the data)
//! - `export`    back up crewmates, memory, tasks and automations as one archive; `import` restores it
//! - `status`    show systemd and hostd status
//! - `logs`      stream journalctl output
//! - `pause`     stop every crewmate's work and hold the queues; `resume` starts them again
//! - `login`     open an interactive agent sign-in session as the crew user
//! - `pair`      pair a phone directly with a crew server (QR + code, no desktop or Mac needed)
//!
//! `layout` has the names on the server and the migration of one installed as "Atlas Bot".

mod apt;
mod binary;
mod browsers;
pub mod install;
#[cfg(all(test, unix))]
mod install_tests;
mod isolation;
mod layout;
mod pair;
mod privileged;
#[cfg(all(test, unix))]
mod server_sim;
mod transfer;
mod update;

use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::path::PathBuf;

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
    /// Turn a Linux x86_64 or aarch64 server into a crew server (installs atlas hostd as a service)
    Install(install::InstallArgs),
    /// Update atlas on a crew server (keeps the previous binary, rolls back if hostd does not come back)
    Update(update::UpdateArgs),
    /// Remove the crew service, user and binary from a server
    Uninstall(UninstallArgs),
    /// Back up a server's crewmates (memory, tasks, automations, usage) as one archive file
    Export(ExportArgs),
    /// Restore an archive made by `atlas crew export` into a server
    Import(ImportArgs),
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
    /// Pair a phone with a crew server: prints a QR code and a code; the phone then reaches the server directly
    Pair(pair::PairArgs),
    /// Install or check the browsers behind the crewmates' `capture` tool (Playwright chromium + webkit)
    Browsers(browsers::BrowsersArgs),
}

impl AtlasCommand for CrewArgs {
    async fn run(self) -> Result<()> {
        match self.command {
            CrewCommands::Install(a) => a.run().await,
            CrewCommands::Update(a) => a.run().await,
            CrewCommands::Uninstall(a) => a.run().await,
            CrewCommands::Export(a) => a.run().await,
            CrewCommands::Import(a) => a.run().await,
            CrewCommands::Status(a) => a.run().await,
            CrewCommands::Logs(a) => a.run().await,
            CrewCommands::Pause(a) => a.run(true).await,
            CrewCommands::Resume(a) => a.run(false).await,
            CrewCommands::Login(a) => a.run().await,
            CrewCommands::Pair(a) => a.run().await,
            CrewCommands::Browsers(a) => a.run().await,
        }
    }
}

// ── uninstall ─────────────────────────────────────────────────────────────────

/// The port every crew command reaches its server on, flattened into each command's arguments.
#[derive(Debug, Clone, Copy, Args)]
pub struct SshPort {
    /// SSH port (default 22). Used when the server listens on a non-standard port.
    #[arg(long, default_value_t = 22)]
    pub port: u16,
}

impl SshPort {
    /// The crew server `target` over ssh: the one constructor every command here uses, so the port
    /// and the agents' `PATH` apply to all of them (issue #355).
    fn remote(self, target: &str) -> Remote {
        Remote::crew(target, self.port)
    }
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
    #[command(flatten)]
    pub ssh: SshPort,
    /// Delete without asking (for scripts). The data is still listed before and after, and is gone
    #[arg(short, long)]
    pub yes: bool,
}

impl AtlasCommand for UninstallArgs {
    async fn run(self) -> Result<()> {
        let r = self.ssh.remote(&self.ssh_target);
        // What will be deleted is always printed first, whatever the flags.
        let inv = match r.exec(transfer::INVENTORY_SCRIPT).await {
            Ok(o) if o.code == 255 => bail!(
                "could not connect to {} over SSH: {}",
                self.ssh_target,
                o.stderr.trim()
            ),
            Ok(o) => transfer::parse_inventory(&o.stdout),
            Err(e) => return Err(e),
        };
        eprint!("{}", transfer::uninstall_notice(&self.ssh_target, &inv));
        let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
        match transfer::flow(self.yes, interactive) {
            transfer::Flow::Refuse => bail!(
                "uninstall deletes data and needs a decision: run it in a terminal to confirm, or pass --yes (take a copy first: atlas crew export {})",
                self.ssh_target
            ),
            transfer::Flow::Ask => {
                if inv.home.is_some()
                    && inv.crewmates > 0
                    && ask_yes_no("Export a copy first? [Y/n] ", true)?
                {
                    // A failed export stops the uninstall: never delete after a backup that did not happen.
                    ExportArgs::for_uninstall(&self.ssh_target, self.ssh)
                        .run()
                        .await?;
                }
                eprint!(
                    "Type the server name ({}) to delete it: ",
                    transfer::server_name(&self.ssh_target)
                );
                if !transfer::confirms_server_name(&self.ssh_target, &read_line()?) {
                    eprintln!("Not the server name. Nothing was deleted.");
                    return Ok(());
                }
            }
            transfer::Flow::Unattended => {}
        }
        // userdel refuses while the crew user has processes (the desktop's SSH sessions). Report it instead of
        // killing them: a `pkill -KILL -u <user>` / `loginctl terminate-user` here coincided with the test server's
        // sshd going down, so uninstall never kills processes on someone's server.
        // Crewmate users and the helpers first: `KillMode=process` leaves their agents running when
        // a service stops, so this kills them itself, before the crew user (their group member) goes.
        let o = r.exec(&isolation::uninstall_script()).await?;
        if o.code != 0 {
            bail!("removing the crewmates' users failed: {}", o.stderr.trim());
        }
        let o = r.exec(&layout::uninstall_script()).await?;
        if o.code != 0 {
            bail!("uninstall failed: {}", o.stderr.trim());
        }
        println!(
            "\x1b[32m✓\x1b[0m crew server removed from {}.",
            self.ssh_target
        );
        println!("{}", transfer::gone_notice(&self.ssh_target, &inv));
        Ok(())
    }
}

fn read_line() -> Result<String> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line)
}

/// A yes/no question on stderr; an empty answer is `default`.
fn ask_yes_no(question: &str, default: bool) -> Result<bool> {
    eprint!("{question}");
    std::io::stderr().flush().ok();
    Ok(match read_line()?.trim().to_ascii_lowercase().as_str() {
        "" => default,
        a => matches!(a, "y" | "yes"),
    })
}

// ── export / import ───────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
    #[command(flatten)]
    pub ssh: SshPort,
    /// File to write (default: atlas-crew-<host>-<date>.tar.gz here). Never overwritten
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// Only this crewmate id (repeat for several); default every crewmate
    #[arg(long = "crewmate", value_name = "ID")]
    pub crewmates: Vec<String>,
    /// Include per-crewmate secrets and webhook tokens (the file is then written 0600: guard it)
    #[arg(long)]
    pub with_secrets: bool,
}

impl ExportArgs {
    /// The export an interactive uninstall offers: everything, secrets included only when asked there.
    fn for_uninstall(target: &str, ssh: SshPort) -> Self {
        let with_secrets = ask_yes_no("Include secrets (API keys, webhook tokens)? [y/N] ", false)
            .unwrap_or(false);
        Self {
            ssh_target: target.to_string(),
            ssh,
            output: None,
            crewmates: vec![],
            with_secrets,
        }
    }
}

impl AtlasCommand for ExportArgs {
    async fn run(self) -> Result<()> {
        let dest = self.output.clone().unwrap_or_else(|| {
            PathBuf::from(transfer::default_export_name(
                &self.ssh_target,
                chrono::Utc::now(),
            ))
        });
        transfer::check_output_free(&dest)?;
        let (code, stderr) = self
            .ssh
            .remote(&self.ssh_target)
            .exec_to_file(
                &transfer::export_script(&self.crewmates, self.with_secrets),
                &dest,
            )
            .await?;
        if code != 0 {
            bail!(transfer::explain_failure(
                &self.ssh_target,
                "export",
                code,
                &stderr
            ));
        }
        // Read it back: an archive that does not open is not a backup.
        let m =
            atlas_host::crew::archive::read_manifest(std::fs::File::open(&dest)?).map_err(|e| {
                anyhow::anyhow!(
                    "the export on {} is unreadable ({e}); {} was kept for inspection",
                    self.ssh_target,
                    dest.display()
                )
            })?;
        println!(
            "\x1b[32m✓\x1b[0m exported {} crewmate(s) from {} to {}",
            m.crewmates.len(),
            self.ssh_target,
            dest.display()
        );
        for c in &m.crewmates {
            println!("    {} ({})", c.name, c.id);
        }
        if m.secrets {
            println!(
                "\x1b[33m!\x1b[0m this file contains secrets and webhook tokens (mode 0600). Keep it private and delete it when done."
            );
        } else {
            println!("  secrets and webhook tokens were left out (--with-secrets includes them)");
        }
        println!(
            "  agent sign-ins and project checkouts are never exported: sign in again with `atlas crew login`"
        );
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
    #[command(flatten)]
    pub ssh: SshPort,
    /// Archive made by `atlas crew export`
    pub file: PathBuf,
    /// A crewmate whose id already exists gets a new id instead of refusing the import
    #[arg(long)]
    pub rename: bool,
}

impl AtlasCommand for ImportArgs {
    async fn run(self) -> Result<()> {
        // Checked here first, so a wrong file or format fails before anything is sent.
        let m = atlas_host::crew::archive::read_manifest(
            std::fs::File::open(&self.file)
                .with_context(|| format!("opening {}", self.file.display()))?,
        )?;
        eprintln!(
            "{}: {} crewmate(s), written by atlas {}{}",
            self.file.display(),
            m.crewmates.len(),
            m.atlas_version,
            if m.secrets { ", with secrets" } else { "" }
        );
        let o = self
            .ssh
            .remote(&self.ssh_target)
            .exec_with_file(&transfer::import_script(self.rename), &self.file)
            .await?;
        if o.code != 0 {
            bail!(transfer::explain_failure(
                &self.ssh_target,
                "import",
                o.code,
                &o.stderr
            ));
        }
        print!("{}", o.stdout);
        println!(
            "\x1b[32m✓\x1b[0m imported into {}. Sign agents in with `atlas crew login` if this server has none.",
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
    #[command(flatten)]
    pub ssh: SshPort,
}

impl AtlasCommand for StatusArgs {
    async fn run(self) -> Result<()> {
        let r = self.ssh.remote(&self.ssh_target);

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
        // The server's atlas against this CLI's: the cue to run `atlas crew update`.
        let server = binary::version_from_status(hostd_out);
        println!(
            "\n{}",
            binary::version_line(server.as_deref(), env!("CARGO_PKG_VERSION"))
        );
        Ok(())
    }
}

// ── pause / resume ────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct PauseArgs {
    /// SSH target for the host (e.g. user@hostname or hostname)
    pub ssh_target: String,
    #[command(flatten)]
    pub ssh: SshPort,
}

impl PauseArgs {
    async fn run(self, pause: bool) -> Result<()> {
        let o = self
            .ssh
            .remote(&self.ssh_target)
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
    #[command(flatten)]
    pub ssh: SshPort,
    /// Follow new log entries (like journalctl -f)
    #[arg(short, long)]
    pub follow: bool,
}

impl AtlasCommand for LogsArgs {
    async fn run(self) -> Result<()> {
        let r = self.ssh.remote(&self.ssh_target);
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
    #[command(flatten)]
    pub ssh: SshPort,
    /// Agent to sign in: omp, claude, codex, opencode
    pub agent: String,
    /// Sign in as this crewmate's own Linux user (its id). On a server where crewmates run as their
    /// own users each one has its own sign-in: this is the way to give it one
    #[arg(long, value_name = "ID")]
    pub crewmate: Option<String>,
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
        if let Some(id) = &self.crewmate {
            return self.login_as_crewmate(id);
        }
        let host = host_from_target(&self.ssh_target).to_owned();
        // The crew user differs on a server installed as "Atlas Bot": ask the host which one it has.
        let user = self
            .ssh
            .remote(&self.ssh_target)
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

        // The same ssh helper as every other crew call: the port, and the agents' PATH (a bare
        // `ssh host claude` runs in a shell that does not have ~/.local/bin).
        let crew = atlas_core::ssh::SshTarget::new(format!("{user}@{host}"), Some(self.ssh.port));
        let argv = crew.terminal_argv(bin);
        println!(
            "\x1b[36m▸\x1b[0m Opening sign-in session: {}",
            argv.join(" ")
        );
        let status = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .status()
            .context("could not launch ssh")?;

        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
        Ok(())
    }
}

impl LoginArgs {
    /// The crewmate's own sign-in: ssh to the server as the person's admin user (the crew user is not in sudoers),
    /// where the crew user's hostd is asked for the engine's login terminal, the request the phone sends
    /// (`crew_engine_login`). The helper starts it as the crewmate under the confinement of its agents; this
    /// command only carries the terminal (`atlas hostd --crew-login`).
    fn login_as_crewmate(&self, id: &str) -> Result<()> {
        let script = layout::login_script(&self.agent, id).with_context(|| {
            format!(
                "'{id}' is not a crewmate id (see `atlas crew status`: 36 characters, lowercase), or '{}' is not an agent",
                self.agent
            )
        })?;
        // The same ssh helper as every other crew call: the port, and the quoting of the script.
        let argv = atlas_core::ssh::SshTarget::new(self.ssh_target.clone(), Some(self.ssh.port))
            .terminal_argv(&script);
        println!(
            "\x1b[36m▸\x1b[0m Opening sign-in session for crewmate {id}: ssh -t {} (atlas hostd --crew-login {})",
            self.ssh_target, self.agent
        );
        let status = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .status()
            .context("could not launch ssh")?;
        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }
        Ok(())
    }
}
