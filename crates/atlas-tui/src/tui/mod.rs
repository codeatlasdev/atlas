pub mod app;
pub mod clicks;
pub mod keymap;
pub mod ui_state;
pub mod views;
pub mod widgets;

use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;
use atlas_core::crash;
use crossterm::{
    cursor,
    event::{DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::config;
use crate::event::EventHandler;
use crate::runtime::lock::SingletonLock;
use crate::runtime::{deps, manager::ServiceManager};

use self::app::App;

const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1002h\x1b[?1006h";
const MOUSE_OFF: &str = "\x1b[?1000l\x1b[?1002l\x1b[?1006l";

/// Turn on mouse reporting + bracketed paste (the modes that must be re-asserted after suspend).
fn enter_modes(out: &mut impl Write) -> io::Result<()> {
    execute!(out, EnableBracketedPaste, crossterm::style::Print(MOUSE_ON))
}

/// Toggle terminal mouse reporting. Off lets the terminal do native text selection.
pub(crate) fn set_mouse_capture(on: bool) {
    let seq = if on { MOUSE_ON } else { MOUSE_OFF };
    let _ = execute!(io::stdout(), crossterm::style::Print(seq));
}

/// Undo everything `run` enabled. Every step runs even if an earlier one fails.
fn restore_terminal() {
    let mut out = io::stdout();
    let _ = execute!(out, crossterm::style::Print(MOUSE_OFF));
    let _ = execute!(out, DisableBracketedPaste);
    let _ = execute!(out, DisableFocusChange);
    let _ = execute!(out, LeaveAlternateScreen);
    let _ = execute!(out, cursor::Show);
    let _ = terminal::disable_raw_mode();
}

/// Restores the terminal on drop (early `?` returns, panics, normal exit).
struct TerminalGuard;

impl TerminalGuard {
    fn restore(&self) {
        restore_terminal();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Ctrl+Z: hand the terminal back, stop the process, and re-enter TUI modes on resume (SIGCONT).
/// Raw mode swallows the tty's own SIGTSTP, so we raise it ourselves.
#[cfg(windows)]
pub(crate) fn suspend() {
    // No job control on Windows: there is no shell to return to while we are stopped.
}

#[cfg(unix)]
pub(crate) fn suspend() {
    restore_terminal();
    // SIGTSTP is stoppable and shell-friendly (job control shows "Stopped").
    let _ = nix::sys::signal::raise(nix::sys::signal::Signal::SIGTSTP);
    // Execution continues here after SIGCONT (`fg`).
    let mut out = io::stdout();
    let _ = terminal::enable_raw_mode();
    let _ = execute!(out, EnterAlternateScreen, EnableFocusChange);
    let _ = enter_modes(&mut out);
}

/// Install a panic hook that restores the terminal before crashing.
/// Always installed (debug builds / RUST_BACKTRACE included) — a panic must never
/// leave the shell in raw mode with the mouse captured.
/// Chains onto atlas-core's crash reporter (writes TOML report + gh command).
fn install_tui_panic_hook() {
    // Take whatever hook is currently installed (may already be the crash reporter)
    // and wrap it with a terminal restore step.
    let inner = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Restore terminal first — otherwise the crash message is invisible/garbled.
        restore_terminal();
        inner(info);
    }));
}

/// Raw mode + alternate screen + panic hooks. The guard restores the tty on any exit path.
fn enter_terminal() -> Result<(TerminalGuard, Terminal<CrosstermBackend<io::Stdout>>)> {
    terminal::enable_raw_mode()?;
    // Created right after raw mode so ANY later `?`/panic still restores the tty.
    let guard = TerminalGuard;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableFocusChange)?;
    enter_modes(&mut stdout)?;

    // Install crash reporter first, then wrap it with terminal restore.
    crash::install_panic_hook();
    install_tui_panic_hook();

    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok((guard, terminal))
}

/// `atlas dev` against a running runner: the same dashboard, fed over the control
/// socket. Services belong to the runner and survive this client.
/// A debug build, or a binary that is a symlink (`atlas link`): the app shows
/// its dev affordances.
fn is_dev_build() -> bool {
    cfg!(debug_assertions)
        || std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::read_link(&p).ok())
            .is_some()
}

/// SIGTERM / SIGHUP (terminal window closed) become a `Quit` event, so the run
/// loop shuts down through its normal path and restores the terminal.
fn quit_on_terminate(
    tx: tokio::sync::mpsc::UnboundedSender<crate::runtime::manager::ManagerEvent>,
) {
    tokio::spawn(async move {
        let mut sigterm = crate::sig::terminate().expect("sigterm");
        let mut sighup = crate::sig::hangup().expect("sighup");
        tokio::select! {
            _ = sigterm.recv() => {}
            _ = sighup.recv()  => {}
        }
        let _ = tx.send(crate::runtime::manager::ManagerEvent::Quit);
    });
}

async fn attach(root_dir: &Path, select: &config::SelectionRequest, runner_pid: i64) -> Result<()> {
    let cfg = config::load(root_dir)?;
    let mut sub = crate::control::subscribe(root_dir)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    // The runner's config is the truth; refuse to draw a different one.
    let snapshot = std::mem::take(&mut sub.snapshot);
    let runner_services: std::collections::BTreeSet<&str> = snapshot["services"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["name"].as_str())
        .collect();
    let local_services: std::collections::BTreeSet<&str> = cfg
        .services
        .iter()
        .filter(|(_, d)| d.is_enabled())
        .map(|(n, _)| n.as_str())
        .collect();
    if runner_services != local_services {
        anyhow::bail!(
            "atlas.yaml changed since the runner (pid {runner_pid}) started; \
             run `atlas down` and then `atlas dev` to pick up the new config"
        );
    }

    // Local mirror: same config-derived data as a real run, but it never spawns or
    // writes anything. Selection is the runner's (it arrives as `deselected` facts).
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let manager = ServiceManager::new(root_dir.to_path_buf(), &cfg, event_tx).into_mirror();

    let (guard, mut terminal) = enter_terminal()?;
    crate::update_notice::set_attached(true);
    let events = EventHandler::new(std::time::Duration::from_millis(100));
    let dev_mode = is_dev_build();
    let mut app = App::new(cfg, manager, event_rx, events, dev_mode);
    app.load_ui_state();
    app.apply_snapshot(&snapshot);
    app.attach(
        app::Remote::new(root_dir.to_path_buf(), runner_pid),
        app::spawn_reader(sub),
    );
    if select.is_narrowed() {
        app.toast_info("attached to the running runner; service selection arguments ignored");
    }
    // SIGTERM/SIGHUP (terminal closed) detach: never stop the runner's services.
    let signal_tx = app.manager_sender();
    quit_on_terminate(signal_tx);

    let result = app.run(&mut terminal).await;
    guard.restore();
    let _ = terminal.show_cursor();
    result?;

    match app.remote.as_ref().and_then(|r| r.leave) {
        Some(app::Leave::Stopped) => {
            // `StopRunner` already sent `shutdown`; just wait for the runner to finish.
            eprintln!("stopping runner…");
            match crate::runner::wait_gone(root_dir, runner_pid, std::time::Duration::from_secs(20))
                .await
            {
                crate::runner::DownOutcome::Timeout { pid } => {
                    eprintln!("runner (pid {pid}) is still stopping; check: atlas dev --status")
                }
                _ => eprintln!("atlas runner stopped"),
            }
        }
        Some(app::Leave::RunnerGone) => eprintln!("atlas runner stopped"),
        _ => eprintln!(
            "detached — services keep running · atlas dev to reattach · atlas down to stop"
        ),
    }
    Ok(())
}

pub async fn run(root_dir: &Path, select: &config::SelectionRequest) -> Result<()> {
    let cfg = config::load(root_dir)?;
    // Fail fast on a bad SERVICE/--profile, before the lock prompt or raw mode.
    let selection = config::resolve_selection(&cfg, select).map_err(|e| anyhow::anyhow!(e))?;

    let log_dir = root_dir.join(".logs");
    std::fs::create_dir_all(&log_dir)?;
    let lock_path = log_dir.join(".tui.lock");

    // A live runner (`atlas dev --detach`, `--headless`) is attached to, never killed.
    if let Some(prior_pid) = SingletonLock::existing_pid(&lock_path) {
        // The lock is held but the socket may still be coming up: give it a moment.
        let mut info = crate::control::info(root_dir).await;
        for _ in 0..15 {
            if info.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            info = crate::control::info(root_dir).await;
        }
        if info.as_ref().is_some_and(|i| i.mode == "runner") {
            return attach(root_dir, select, info.map_or(0, |i| i.pid)).await;
        }
        // An owner TUI (or an unresponsive atlas): the classic prompt.
        eprint!(
            "\x1b[1;33m◆\x1b[0m atlas dev already running (PID {prior_pid}).\n  Kill it and restart? [y/N] "
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
            SingletonLock::kill_prior(prior_pid).await;
            // Remove stale lock file so acquire can create a fresh one
            let _ = std::fs::remove_file(&lock_path);
        } else {
            eprintln!("\x1b[2m  Keeping existing session. Exiting.\x1b[0m");
            return Ok(());
        }
    }

    let _lock = SingletonLock::acquire(&lock_path)?;
    // Only after the lock: any existing atlas.sock is stale and gets replaced.
    let control = crate::control::ControlServer::bind(root_dir).ok();

    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let deps_tx = event_tx.clone();
    let signal_tx = event_tx.clone();
    let manager =
        ServiceManager::with_selection(root_dir.to_path_buf(), &cfg, event_tx, &selection);

    let (guard, mut terminal) = enter_terminal()?;

    let events = EventHandler::new(std::time::Duration::from_millis(100));
    let pre_command = cfg.pre_command.clone();
    let pre_command_when = cfg.pre_command_when;

    // Detect dev mode: binary is a symlink (atlas link) or a debug build.
    let dev_mode = is_dev_build();

    let mut app = App::new(cfg, manager, event_rx, events, dev_mode);
    app.load_ui_state();
    let _control_server = control.map(|(server, rx)| {
        app.control_rx = Some(rx);
        server
    });

    // Run dep install in background so TUI opens immediately. App services wait
    // for it (`deps_pending`) — they must not spawn against a half-written
    // node_modules. If node_modules is up to date this is a no-op (mtime check).
    let deps_root = root_dir.to_path_buf();
    let deps_pending = deps::install_pending(&deps_root, pre_command.as_deref(), pre_command_when);
    app.deps_pending = deps_pending;
    tokio::spawn(async move {
        let event =
            match deps::ensure_deps(&deps_root, pre_command.as_deref(), pre_command_when, false)
                .await
            {
                // Became fresh between the gate and now: still release the held start.
                deps::DepsOutcome::UpToDate if deps_pending => {
                    crate::runtime::manager::ManagerEvent::DepsInstalled
                }
                deps::DepsOutcome::UpToDate => return,
                deps::DepsOutcome::Installed => {
                    crate::runtime::manager::ManagerEvent::DepsInstalled
                }
                deps::DepsOutcome::Failed(reason) => {
                    crate::runtime::manager::ManagerEvent::DepsFailed { reason }
                }
            };
        let _ = deps_tx.send(event);
    });
    // Graceful shutdown on SIGTERM / SIGHUP (terminal window close).
    // Sends ManagerEvent::Quit into the existing channel — the run loop
    // calls stop_all() and restores the terminal through the normal quit path.
    quit_on_terminate(signal_tx);

    // App::run always calls manager.stop_all() — even when the loop errors.
    let result = app.run(&mut terminal).await;

    // Restore explicitly (best-effort, each step independent); the guard is the
    // backstop for early returns/panics before this point.
    guard.restore();
    let _ = terminal.show_cursor();

    result
}
