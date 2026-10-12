use anyhow::Result;
use atlas_core::crash;
use clap::{Parser, Subcommand};
use commands::AtlasCommand;

mod commands;

#[derive(Debug, Parser)]
#[command(
    name = "atlas",
    about = "Atlas — dev environment orchestrator",
    version = env!("CARGO_PKG_VERSION"),
    arg_required_else_help = true,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start the development environment TUI
    Dev(commands::dev::DevArgs),

    /// Stop the running atlas (runner or TUI) and every service it started
    Down(commands::down::DownArgs),

    /// Initialize atlas.yaml for the current project
    Init(commands::init::InitArgs),

    /// Validate atlas.yaml or print its JSON Schema
    Config {
        #[command(subcommand)]
        command: commands::config::ConfigCommands,
    },

    #[command(next_help_heading = "Deploy")]
    /// Ship a release to a server: atomic swap, health-gated restarts, auto-rollback
    Deploy(commands::deploy::DeployArgs),

    #[command(next_help_heading = "Maintenance")]
    /// Update atlas to the latest version
    #[command(alias = "up")]
    Update(commands::self_update::UpdateArgs),

    #[command(next_help_heading = "Skills")]
    /// Manage CodeAtlas AI skills
    Skills {
        #[command(subcommand)]
        command: commands::skills::SkillsCommands,
    },

    /// Wire atlas into your AI agent (omp, claude-code, codex): MCP server, skills, hooks
    Setup(commands::setup::SetupArgs),

    /// Agent hook handler (event JSON on stdin, response JSON on stdout); run by the agent, not by hand
    #[command(hide = true)]
    Hook(commands::hook::HookArgs),

    /// Execution daemon behind Atlas Desktop (terminals, exec, fs); started by the app
    #[command(hide = true)]
    Hostd(commands::hostd::HostdArgs),

    /// Inspect and drive Atlas Desktop terminals: ls, read, send, kill, wait
    Term(commands::hostd::TermArgs),

    /// Manage Atlas Desktop worktrees: create, list, remove, set
    Wt {
        #[command(subcommand)]
        command: commands::wt::WtCommands,
        #[arg(long, global = true)]
        json: bool,
    },

    /// Control the Atlas Desktop embedded browser: goto, snapshot, click, fill, screenshot
    Browser(commands::wt::BrowserArgs),

    /// Open files and diffs in Atlas Desktop
    File(commands::wt::FileArgs),

    /// Run the MCP server (stdio) so AI agents can read logs/status and control services
    Mcp(commands::mcp::McpArgs),

    /// Open Atlas Desktop (installs the matching release on first run)
    Desktop(commands::desktop::DesktopArgs),

    /// Print version information
    Version(commands::version::VersionArgs),

    /// Remove atlas from this machine
    Uninstall(commands::uninstall::UninstallArgs),

    /// Install atlas into ~/.atlas/bin and configure PATH
    #[command(hide = true)]
    Install(commands::install::InstallArgs),

    /// Alias for update
    #[command(name = "self-update", hide = true)]
    SelfUpdate(commands::self_update::UpdateArgs),

    #[command(next_help_heading = "Development")]
    /// Symlink the installed atlas to the local debug build for in-repo testing
    Link(commands::link::LinkArgs),

    /// Restore the release binary after `atlas link`
    Unlink(commands::link::UnlinkArgs),
}

#[tokio::main]
async fn main() {
    // Install panic hook in release builds so crashes produce a readable report
    // with a ready-to-run gh issue command. Skipped when RUST_BACKTRACE is set.
    crash::install_panic_hook();

    if let Err(e) = run().await {
        eprintln!("\x1b[1;31merror:\x1b[0m {e}");
        for cause in e.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    let is_update = matches!(cli.command, Commands::Update(_) | Commands::SelfUpdate(_));
    // MCP is a long-lived stdio protocol and hooks are machine-to-machine (stdout is parsed
    // by the agent): no update check, no notices.
    let is_mcp = matches!(
        cli.command,
        Commands::Mcp(_)
            | Commands::Hook(_)
            | Commands::Hostd(_)
            | Commands::Term(_)
            | Commands::Wt { .. }
            | Commands::Browser(_)
            | Commands::File(_)
    );

    if !is_update && !is_mcp {
        commands::self_update::spawn_background_check();
    }

    match cli.command {
        Commands::Dev(args) => args.run().await?,
        Commands::Down(args) => args.run().await?,
        Commands::Init(args) => args.run().await?,
        Commands::Config { command } => command.run()?,
        Commands::Deploy(args) => args.run().await?,
        Commands::Update(args) => args.run().await?,
        Commands::SelfUpdate(args) => args.run().await?,
        Commands::Install(args) => args.run().await?,
        Commands::Uninstall(args) => args.run().await?,
        Commands::Setup(args) => args.run().await?,
        Commands::Mcp(args) => args.run().await?,
        Commands::Hook(args) => args.run().await?,
        Commands::Hostd(args) => args.run().await?,
        Commands::Term(args) => args.run().await?,
        Commands::Wt { command, json } => (commands::wt::WtArgs { command, json }).run().await?,
        Commands::Browser(args) => args.run().await?,
        Commands::File(args) => args.run().await?,
        Commands::Desktop(args) => args.run().await?,
        Commands::Version(args) => args.run().await?,
        Commands::Skills { command } => command.run().await?,
        Commands::Link(args) => args.run().await?,
        Commands::Unlink(args) => args.run().await?,
    }

    // The TUI is gone and its services are stopped: install an update the user accepted
    // in the "update when you quit" modal. Otherwise fall back to the exit banner.
    if atlas_tui::update_notice::on_exit_requested().is_some() {
        commands::update_prompt::run_on_exit_update().await;
    } else if !is_update && !is_mcp && std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        commands::self_update::print_update_notice();
    }

    Ok(())
}
