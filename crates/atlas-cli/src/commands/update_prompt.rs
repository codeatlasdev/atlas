//! Pre-flight update prompt for `atlas dev` (before the TUI takes the terminal) and the
//! post-exit update requested from the TUI's "update when you quit" modal.

use std::io::{BufRead, IsTerminal, Write};
use std::os::unix::process::CommandExt;
use std::time::Duration;

use anyhow::Result;

use super::self_update::{
    self, UpdateCache, UpdateOutcome, current_version, load_cache, newer_than, now_secs, save_cache,
};

/// Never block `atlas dev` longer than this on a live version check.
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
const CHANGELOG_URL: &str = "https://github.com/codeatlasdev/atlas/releases/tag";

/// Facts the prompt decision depends on (all gathered by the caller, so the decision is pure).
pub(crate) struct PromptEnv<'a> {
    /// The interactive TUI path: no `--status/--ci/--headless/--runner/--detach`.
    pub interactive_path: bool,
    pub stdin_tty: bool,
    pub stdout_tty: bool,
    /// `ATLAS_NO_UPDATE_PROMPT=1`.
    pub disabled_by_env: bool,
    /// `atlas link` symlink or a debug build.
    pub linked: bool,
    pub current: &'a str,
    /// Newest version known (cache or live check), unfiltered.
    pub newest: Option<&'a str>,
    pub cache: &'a UpdateCache,
    pub now: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Say nothing.
    Skip,
    /// Linked/debug build: a one-line hint, never a prompt (never replaces a dev binary).
    Hint(String),
    /// Ask "Update now?" for this version.
    Prompt(String),
}

pub(crate) fn decide(env: &PromptEnv) -> Decision {
    if !env.interactive_path || env.disabled_by_env || !env.stdin_tty || !env.stdout_tty {
        return Decision::Skip;
    }
    let Some(newer) = env.newest.and_then(|n| newer_than(env.current, n)) else {
        return Decision::Skip;
    };
    if env.cache.is_dismissed(&newer, env.now) {
        return Decision::Skip;
    }
    if env.linked {
        return Decision::Hint(newer);
    }
    Decision::Prompt(newer)
}

/// Binary is an `atlas link` symlink or a debug build. `ATLAS_UPDATE_PROMPT=force` lifts the
/// debug-build half only (used by the pty integration test, which runs a debug binary).
fn is_linked() -> bool {
    let symlinked = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::read_link(p).ok())
        .is_some();
    let forced = std::env::var("ATLAS_UPDATE_PROMPT").is_ok_and(|v| v == "force");
    symlinked || (cfg!(debug_assertions) && !forced)
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// Parse a `[Y/n]` answer: Enter / y / yes → true, anything else → false.
pub(crate) fn parse_yes_default(line: &str) -> bool {
    matches!(line.trim().to_ascii_lowercase().as_str(), "" | "y" | "yes")
}

fn ask(question: &str, input: &mut impl BufRead) -> bool {
    eprint!("{question}");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    match input.read_line(&mut line) {
        // EOF (stdin closed): do not guess Yes for a destructive-ish action.
        Ok(0) | Err(_) => false,
        Ok(_) => parse_yes_default(&line),
    }
}

fn print_box(current: &str, new: &str) {
    let url = format!("{CHANGELOG_URL}/v{new}");
    let lines = [
        format!("⬆ Atlas update available: v{current} → v{new}"),
        format!("Changelog: {url}"),
    ];
    let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    eprintln!();
    eprintln!("\x1b[1;32m╭─{}─╮\x1b[0m", "─".repeat(w));
    for l in &lines {
        eprintln!(
            "\x1b[1;32m│\x1b[0m {l}{} \x1b[1;32m│\x1b[0m",
            " ".repeat(w - l.chars().count())
        );
    }
    eprintln!("\x1b[1;32m╰─{}─╯\x1b[0m", "─".repeat(w));
}

/// Re-exec the (just replaced) binary with the exact argv. Only returns on failure.
fn reexec() -> std::io::Error {
    let exe = std::env::current_exe().unwrap_or_else(|_| {
        std::env::args_os()
            .next()
            .map(Into::into)
            .unwrap_or_else(|| "atlas".into())
    });
    let mut cmd = std::process::Command::new(exe);
    cmd.args(std::env::args_os().skip(1));
    // The re-exec'd process must not prompt again if the update somehow did not stick.
    cmd.env("ATLAS_NO_UPDATE_PROMPT", "1");
    cmd.exec()
}

/// What the prompt flow did, for the caller to continue or stop.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Flow {
    /// Carry on starting `atlas dev`.
    Continue,
    /// The user declined to continue after a failed update.
    Abort,
}

/// The "Yes" branch with the update step injected so it can be unit-tested without
/// touching the network or replacing a binary.
pub(crate) async fn on_accept<F, Fut, R>(
    current: &str,
    input: &mut impl BufRead,
    update: F,
    reexec: R,
) -> Flow
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<UpdateOutcome>>,
    R: FnOnce(),
{
    match update().await {
        Ok(UpdateOutcome::Installed) => {
            reexec();
            // `reexec` only returns when exec failed; keep running the old version.
            Flow::Continue
        }
        // Homebrew etc.: the update command told the user what to run; carry on.
        Ok(UpdateOutcome::Unchanged) => Flow::Continue,
        Err(e) => {
            eprintln!("\x1b[1;31merror:\x1b[0m update failed: {e}");
            for cause in e.chain().skip(1) {
                eprintln!("  caused by: {cause}");
            }
            if ask(&format!("Continue with v{current}? [Y/n] "), input) {
                Flow::Continue
            } else {
                Flow::Abort
            }
        }
    }
}

/// Pre-flight for `atlas dev`. `interactive_path` = the plain TUI invocation.
/// Returns `Flow::Abort` only when the update failed and the user chose not to continue.
pub(crate) async fn preflight(interactive_path: bool) -> Flow {
    let stdin_tty = std::io::stdin().is_terminal();
    let stdout_tty = std::io::stdout().is_terminal();
    let disabled_by_env = env_flag("ATLAS_NO_UPDATE_PROMPT");
    // Cheap gates first: no network for the paths that can never prompt.
    if !interactive_path || disabled_by_env || !stdin_tty || !stdout_tty {
        return Flow::Continue;
    }

    let (cache, newest) = self_update::newest_for_preflight(CHECK_TIMEOUT).await;
    let now = now_secs();
    let decision = decide(&PromptEnv {
        interactive_path,
        stdin_tty,
        stdout_tty,
        disabled_by_env,
        linked: is_linked(),
        current: current_version(),
        newest: newest.as_deref(),
        cache: &cache,
        now,
    });

    match decision {
        Decision::Skip => Flow::Continue,
        Decision::Hint(v) => {
            eprintln!(
                "\x1b[2m⬆ atlas v{v} is available (running a linked/dev build; `atlas update` skipped)\x1b[0m"
            );
            Flow::Continue
        }
        Decision::Prompt(v) => {
            print_box(current_version(), &v);
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            if ask("Update now? [Y/n] ", &mut input) {
                on_accept(
                    current_version(),
                    &mut input,
                    || self_update::run_update(false, None),
                    || {
                        let e = reexec();
                        eprintln!("\x1b[1;31merror:\x1b[0m could not restart atlas: {e}");
                    },
                )
                .await
            } else {
                let mut cache = load_cache();
                cache.dismiss(&v, now_secs());
                save_cache(&cache);
                Flow::Continue
            }
        }
    }
}

/// After the TUI exited (terminal restored, services stopped): install the update the user
/// accepted in the modal. No re-exec — the session is over.
pub(crate) async fn run_on_exit_update() {
    let Some(version) = atlas_tui::update_notice::on_exit_requested() else {
        return;
    };
    eprintln!("\x1b[1;34m◆\x1b[0m Updating atlas to v{version}…");
    match self_update::run_update(false, None).await {
        Ok(UpdateOutcome::Installed) => {
            eprintln!("\x1b[1;32m✓\x1b[0m atlas updated to v{version}");
            if atlas_tui::update_notice::attached() {
                eprintln!(
                    "  The runner keeps the old version until: \x1b[1matlas down && atlas dev --detach\x1b[0m"
                );
            }
        }
        Ok(UpdateOutcome::Unchanged) => {}
        Err(e) => {
            eprintln!("\x1b[1;31merror:\x1b[0m update failed: {e}");
            for cause in e.chain().skip(1) {
                eprintln!("  caused by: {cause}");
            }
            eprintln!("  Run \x1b[1matlas update\x1b[0m to retry.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const NOW: u64 = 1_000_000;

    fn env<'a>(cache: &'a UpdateCache) -> PromptEnv<'a> {
        PromptEnv {
            interactive_path: true,
            stdin_tty: true,
            stdout_tty: true,
            disabled_by_env: false,
            linked: false,
            current: "0.4.1",
            newest: Some("0.5.0"),
            cache,
            now: NOW,
        }
    }

    #[test]
    fn prompts_when_newer_and_interactive() {
        let c = UpdateCache::default();
        assert_eq!(decide(&env(&c)), Decision::Prompt("0.5.0".into()));
    }

    #[test]
    fn skips_when_not_the_interactive_tui_path() {
        let c = UpdateCache::default();
        let mut e = env(&c);
        e.interactive_path = false; // --status/--ci/--headless/--runner/--detach
        assert_eq!(decide(&e), Decision::Skip);
    }

    #[test]
    fn skips_without_a_tty() {
        let c = UpdateCache::default();
        let mut e = env(&c);
        e.stdin_tty = false;
        assert_eq!(decide(&e), Decision::Skip);
        let mut e = env(&c);
        e.stdout_tty = false;
        assert_eq!(decide(&e), Decision::Skip);
    }

    #[test]
    fn skips_when_disabled_by_env() {
        let c = UpdateCache::default();
        let mut e = env(&c);
        e.disabled_by_env = true;
        assert_eq!(decide(&e), Decision::Skip);
    }

    #[test]
    fn linked_builds_only_get_a_hint() {
        let c = UpdateCache::default();
        let mut e = env(&c);
        e.linked = true;
        assert_eq!(decide(&e), Decision::Hint("0.5.0".into()));
    }

    #[test]
    fn linked_hint_respects_dismissal_and_flags() {
        let mut c = UpdateCache::default();
        c.dismiss("0.5.0", NOW);
        let mut e = env(&c);
        e.linked = true;
        assert_eq!(decide(&e), Decision::Skip);
        let c = UpdateCache::default();
        let mut e = env(&c);
        e.linked = true;
        e.interactive_path = false;
        assert_eq!(decide(&e), Decision::Skip);
    }

    #[test]
    fn skips_when_not_newer_or_unknown() {
        let c = UpdateCache::default();
        for newest in [Some("0.4.1"), Some("0.4.0"), Some("nonsense"), None] {
            let mut e = env(&c);
            e.newest = newest;
            assert_eq!(decide(&e), Decision::Skip, "{newest:?}");
        }
    }

    #[test]
    fn version_compare_is_semver() {
        let c = UpdateCache::default();
        let mut e = env(&c);
        e.newest = Some("0.10.0");
        assert_eq!(decide(&e), Decision::Prompt("0.10.0".into()));
    }

    #[test]
    fn dismissed_version_is_silent_for_24h_then_returns() {
        let mut c = UpdateCache::default();
        c.dismiss("0.5.0", NOW);
        let mut e = env(&c);
        assert_eq!(decide(&e), Decision::Skip);
        e.now = NOW + 24 * 3600 - 1;
        assert_eq!(decide(&e), Decision::Skip);
        e.now = NOW + 24 * 3600;
        assert_eq!(decide(&e), Decision::Prompt("0.5.0".into()));
    }

    #[test]
    fn dismissal_of_an_older_version_does_not_hide_a_newer_one() {
        let mut c = UpdateCache::default();
        c.dismiss("0.5.0", NOW);
        let mut e = env(&c);
        e.newest = Some("0.6.0");
        assert_eq!(decide(&e), Decision::Prompt("0.6.0".into()));
    }

    #[test]
    fn answer_parsing_defaults_to_yes() {
        for yes in ["", "\n", "y", "Y", "yes", " YES \n"] {
            assert!(parse_yes_default(yes), "{yes:?}");
        }
        for no in ["n", "N", "no", "nope", "q"] {
            assert!(!parse_yes_default(no), "{no:?}");
        }
    }

    #[tokio::test]
    async fn accept_then_installed_reexecs() {
        let mut execd = false;
        let mut input = Cursor::new(Vec::new());
        let flow = on_accept(
            "0.4.1",
            &mut input,
            || async { Ok(UpdateOutcome::Installed) },
            || execd = true,
        )
        .await;
        assert!(execd, "must re-exec the new binary");
        assert_eq!(flow, Flow::Continue);
    }

    #[tokio::test]
    async fn accept_but_nothing_replaced_does_not_reexec() {
        let mut execd = false;
        let mut input = Cursor::new(Vec::new());
        let flow = on_accept(
            "0.4.1",
            &mut input,
            || async { Ok(UpdateOutcome::Unchanged) },
            || execd = true,
        )
        .await;
        assert!(!execd);
        assert_eq!(flow, Flow::Continue);
    }

    #[tokio::test]
    async fn failed_update_asks_to_continue_default_yes() {
        let mut execd = false;
        for (answer, want) in [
            ("\n", Flow::Continue),
            ("y\n", Flow::Continue),
            ("n\n", Flow::Abort),
        ] {
            let mut input = Cursor::new(answer.as_bytes().to_vec());
            let flow = on_accept(
                "0.4.1",
                &mut input,
                || async { Err(anyhow::anyhow!("download failed")) },
                || execd = true,
            )
            .await;
            assert_eq!(flow, want, "answer {answer:?}");
        }
        assert!(!execd, "a failed update never re-execs");
    }

    #[tokio::test]
    async fn failed_update_with_closed_stdin_aborts() {
        let mut input = Cursor::new(Vec::new());
        let flow = on_accept(
            "0.4.1",
            &mut input,
            || async { Err(anyhow::anyhow!("boom")) },
            || {},
        )
        .await;
        assert_eq!(flow, Flow::Abort);
    }
}
