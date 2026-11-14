use anyhow::Result;
use atlas_core::crash;
use clap::{Parser, Subcommand};
use commands::AtlasCommand;
use std::ffi::OsString;

mod commands;

#[derive(Debug, Parser)]
#[command(
    name = "atlas",
    about = "Atlas — dev environment orchestrator",
    version = env!("CARGO_PKG_VERSION"),
    after_help = "Open a folder or file in Atlas Desktop:  atlas <path>   (atlas .)\nRun a command as a service of this folder's stack:  atlas -- <cmd…>",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start the development environment TUI
    Dev(commands::dev::DevArgs),

    /// Stop the running atlas (runner or TUI) and every service it started
    Down(commands::down::DownArgs),

    /// Check this machine: `--hygiene` finds orphaned Atlas processes and the disk Atlas owns
    Doctor(commands::doctor::DoctorArgs),

    /// Run a command through the machine-wide queue of heavy jobs: `atlas run --heavy -- cargo test`
    Run(commands::heavy::RunArgs),

    /// The machine-wide queue of heavy jobs (builds, test suites): who runs, who waits
    Heavy {
        #[command(subcommand)]
        command: commands::heavy::HeavyCommands,
    },

    /// Initialize atlas.yaml for the current project
    Init(commands::init::InitArgs),

    /// Validate atlas.yaml or print its JSON Schema
    Config {
        #[command(subcommand)]
        command: commands::config::ConfigCommands,
    },

    #[command(next_help_heading = "Crew")]
    /// Manage Atlas Crew servers (where crewmates run): install, login, status, logs
    // `bot` is the pre-rename name: kept (hidden) for scripts and docs written before Atlas Crew.
    #[command(alias = "bot")]
    Crew(commands::crew::CrewArgs),

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

/// What the command line asked for, after the path/typo decision.
#[derive(Debug)]
enum Action {
    /// `atlas` alone: print help.
    Help,
    /// `atlas -- <cmd…>`
    Adhoc(Vec<String>),
    /// `atlas <path>`
    OpenPath(OsString),
    Command(Commands),
}

/// Parse `argv`. `atlas -- <cmd…>` (a leading `--`) is the ad-hoc command. A first word that is
/// not a subcommand opens as a path when it exists, is `.`/`..` or contains a path separator;
/// anything else falls through to clap, whose own "unrecognized subcommand" error (with its
/// "similar subcommand" tip) is the answer for a typo. Subcommands always win over a folder of
/// the same name: only a word clap does not know is ever tested as a path.
fn parse(argv: &[OsString]) -> Result<Action, clap::Error> {
    parse_with(argv, |w| std::path::Path::new(w).exists())
}

/// [`parse`] with the file-system check injected (`exists`), so the precedence is testable.
fn parse_with(
    argv: &[OsString],
    exists: impl Fn(&OsString) -> bool,
) -> Result<Action, clap::Error> {
    use clap::CommandFactory;
    match argv.get(1) {
        Some(a) if a == "--" => {
            let cmd: Vec<String> = argv[2..]
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            return Ok(if cmd.is_empty() {
                Action::Help
            } else {
                Action::Adhoc(cmd)
            });
        }
        Some(w)
            if !w.to_string_lossy().starts_with('-')
                && w != "help"
                && Cli::command().find_subcommand(w).is_none()
                && commands::open::is_path_word(w, exists) =>
        {
            return Ok(Action::OpenPath(w.clone()));
        }
        _ => {}
    }
    Ok(match Cli::try_parse_from(argv)?.command {
        None => Action::Help,
        Some(c) => Action::Command(c),
    })
}

async fn run() -> Result<()> {
    let command = match parse(&std::env::args_os().collect::<Vec<_>>()) {
        Ok(Action::Command(c)) => c,
        Ok(Action::Help) => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            return Ok(());
        }
        Ok(Action::Adhoc(argv)) => return commands::open::run_adhoc(&argv).await,
        Ok(Action::OpenPath(word)) => return commands::open::open_path(&word, None),
        Err(e) => e.exit(),
    };
    // Rust ignores SIGPIPE, so `atlas version | head` panics on the closed pipe. Short-lived
    // commands get the Unix default and end quietly; the socket servers (`dev`, `hostd`) keep
    // ignoring it so a client that hangs up is an EPIPE error, never a dead daemon.
    #[cfg(unix)]
    if !matches!(command, Commands::Dev(_) | Commands::Hostd(_)) {
        // SAFETY: before any thread is spawned or anything is written.
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    }

    let is_update = matches!(command, Commands::Update(_) | Commands::SelfUpdate(_));
    // MCP is a long-lived stdio protocol and hooks are machine-to-machine (stdout is parsed
    // by the agent): no update check, no notices.
    let is_mcp = matches!(
        command,
        Commands::Mcp(_)
            | Commands::Hook(_)
            | Commands::Hostd(_)
            | Commands::Term(_)
            | Commands::Wt { .. }
            | Commands::Browser(_)
            | Commands::File(_)
            | Commands::Run(_)
            | Commands::Heavy { .. }
    );

    if !is_update && !is_mcp {
        commands::self_update::spawn_background_check();
    }

    match command {
        Commands::Dev(args) => args.run().await?,
        Commands::Down(args) => args.run().await?,
        Commands::Doctor(args) => args.run().await?,
        Commands::Init(args) => args.run().await?,
        Commands::Run(args) => args.run().await?,
        Commands::Heavy { command } => command.run().await?,
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
        Commands::Crew(args) => args.run().await?,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<OsString> {
        std::iter::once("atlas")
            .chain(words.iter().copied())
            .map(Into::into)
            .collect()
    }

    // Decision: subcommands always win. A folder named like a subcommand opens as `atlas ./dev`.
    #[test]
    fn a_subcommand_wins_over_a_folder_of_the_same_name() {
        // Every word "exists on disk", so a bare `dev` or `down` WOULD be a path if tested first.
        let everything_exists = |_: &OsString| true;
        assert!(matches!(
            parse_with(&argv(&["dev", "--status"]), everything_exists),
            Ok(Action::Command(Commands::Dev(_)))
        ));
        assert!(matches!(
            parse_with(&argv(&["down"]), everything_exists),
            Ok(Action::Command(Commands::Down(_)))
        ));
        // The folder is reachable by spelling it as a path.
        assert!(matches!(
            parse_with(&argv(&["./dev"]), everything_exists),
            Ok(Action::OpenPath(_))
        ));
    }

    // Decision: a non-subcommand word opens as a path only if it is `.`/`..`, has a separator,
    // or exists on disk.
    #[test]
    fn dot_dotdot_separator_and_existing_names_open_as_paths() {
        for w in [".", "..", "some/where", r"C:\some\where"] {
            assert!(matches!(parse(&argv(&[w])), Ok(Action::OpenPath(_))), "{w}");
        }
        // A bare name that exists on disk is a path; one that does not is a typo.
        let only_notes = |w: &OsString| w == "notes";
        assert!(matches!(
            parse_with(&argv(&["notes"]), only_notes),
            Ok(Action::OpenPath(_))
        ));
        assert!(parse_with(&argv(&["nots"]), only_notes).is_err());
    }

    // Decision: a word that is neither errors with clap's usual "unrecognized subcommand" and
    // its suggestion, exactly as before this change.
    #[test]
    fn a_typo_errors_with_clap_unknown_command_and_a_suggestion() {
        let err = parse(&argv(&["devv"])).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
        let text = err.to_string();
        assert!(text.contains("unrecognized subcommand 'devv'"), "{text}");
        assert!(text.contains("'dev'"), "suggestion missing: {text}");
    }

    #[test]
    fn bare_atlas_prints_help_and_dash_dash_collects_the_command() {
        assert!(matches!(parse(&argv(&[])), Ok(Action::Help)));
        match parse(&argv(&["--", "node", "-e", "x"])).unwrap() {
            Action::Adhoc(v) => assert_eq!(v, ["node", "-e", "x"]),
            other => panic!("{other:?}"),
        }
    }

    // Crew review P0, PR #106 (no kill switch): `atlas crew pause|resume <ssh-target>` exist, take the same
    // target as `status`, and keep working under the hidden `atlas bot` alias.
    #[test]
    fn crew_pause_and_resume_take_an_ssh_target() {
        use commands::crew::CrewCommands as C;
        for w in ["crew", "bot"] {
            match parse(&argv(&[w, "pause", "me@server"])) {
                Ok(Action::Command(Commands::Crew(a))) => match a.command {
                    C::Pause(t) => assert_eq!(t.ssh_target, "me@server"),
                    other => panic!("{w} pause: {other:?}"),
                },
                other => panic!("{w} pause: {other:?}"),
            }
        }
        match parse(&argv(&["crew", "resume", "me@server"])) {
            Ok(Action::Command(Commands::Crew(a))) => match a.command {
                C::Resume(t) => assert_eq!(t.ssh_target, "me@server"),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        // The target is required, like for `status`.
        assert!(parse(&argv(&["crew", "pause"])).is_err());
        assert!(parse(&argv(&["crew", "resume"])).is_err());
    }

    // The server side of those commands: `atlas hostd --crew-pause|--crew-resume`, one at a time,
    // never together with the flags that stop or restart the daemon.
    #[test]
    fn hostd_crew_pause_and_resume_flags_are_exclusive() {
        let hostd = |flags: &[&str]| {
            let mut v = vec!["hostd"];
            v.extend_from_slice(flags);
            parse(&argv(&v))
        };
        match hostd(&["--crew-pause", "--dir", "/d"]) {
            Ok(Action::Command(Commands::Hostd(h))) => {
                assert!(h.crew_pause && !h.crew_resume)
            }
            other => panic!("{other:?}"),
        }
        match hostd(&["--crew-resume"]) {
            Ok(Action::Command(Commands::Hostd(h))) => {
                assert!(h.crew_resume && !h.crew_pause)
            }
            other => panic!("{other:?}"),
        }
        for bad in [
            &["--crew-pause", "--crew-resume"][..],
            &["--crew-pause", "--stop"],
            &["--crew-resume", "--status"],
            &["--crew-pause", "--restart"],
        ] {
            assert!(hostd(bad).is_err(), "{bad:?} should conflict");
        }
    }

    // Decision (Atlas Bot -> Atlas Crew rename): scripts and crewmates set up before it keep working.
    // `atlas bot …` is `atlas crew …`, `atlas mcp --bot <id>` is `--crewmate <id>`, and neither shows
    // in help (docs/crew.md documents them as hidden aliases).
    #[test]
    fn pre_rename_bot_spellings_still_parse_and_stay_out_of_help() {
        let status = |w: &str| match parse(&argv(&[w, "status", "me@server"])) {
            Ok(Action::Command(Commands::Crew(a))) => match a.command {
                commands::crew::CrewCommands::Status(s) => s.ssh_target,
                other => panic!("{other:?}"),
            },
            other => panic!("{w}: {other:?}"),
        };
        assert_eq!(status("crew"), "me@server");
        assert_eq!(status("bot"), "me@server");
        for flag in ["--crewmate", "--bot"] {
            match parse(&argv(&["mcp", flag, "c1"])) {
                Ok(Action::Command(Commands::Mcp(m))) => {
                    assert_eq!(m.crewmate.as_deref(), Some("c1"))
                }
                other => panic!("{flag}: {other:?}"),
            }
        }
        use clap::CommandFactory as _;
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("crew"), "{help}");
        assert!(
            !help.contains("bot"),
            "the old name leaked into help: {help}"
        );
    }
}
