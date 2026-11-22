//! The words `atlas` prints around a message for a person: `error:`, `warning:`, `caused by:` and the `tip:` of a usage
//! error, in the display language (`cli_chrome`). The message after them is the command's own text.
//!
//! Human output only: a program never parses these lines (`--json`, MCP, `--progress json` and the `hint:` / `error:`
//! lines the desktop reads from `crew install` stay English; `tests/machine_output.rs`).

use atlas_i18n::{t, t_in};
use std::io::IsTerminal;

const RED_BOLD: &str = "\x1b[1;31m";
const RESET: &str = "\x1b[0m";

/// `error:` in red, in the display language.
pub fn error() -> String {
    format!("{RED_BOLD}{}{RESET}", t!("cli_chrome_error"))
}

/// `warning:` in the display language.
pub fn warning() -> String {
    t!("cli_chrome_warning")
}

/// `caused by:` in the display language: the prefix of each underlying cause under an error.
pub fn caused_by() -> String {
    t!("cli_chrome_caused_by")
}

/// The words of one error of a chain: the catalog's for a failed update check (`atlas-core` writes its English, for logs
/// and `--json`), the error's own text for anything else.
pub fn cause_text(e: &(dyn std::error::Error + 'static)) -> String {
    match e.downcast_ref::<atlas_core::github::GithubError>() {
        Some(github) => atlas_i18n::github_error(github),
        None => e.to_string(),
    }
}

/// [`cause_text`] of the outermost error of `e`, the line after `error:`.
pub fn error_text(e: &anyhow::Error) -> String {
    e.chain().next().map_or_else(|| e.to_string(), cause_text)
}

/// How a finished child ended, worded for a sentence ("the app exited with {…}"): its exit code, or on Unix the
/// signal that ended it. `ExitStatus`'s own `Display` is English; this one is in the display language.
pub fn exit_text(status: std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return t!("cli_chrome_exit_signal", signal = signal);
        }
    }
    t!("cli_chrome_exit_code", code = status.code().unwrap_or(-1))
}

/// The text clap renders for a usage error, with its `error:` and `tip:` words in `tag`. clap prints them in English
/// and has no hook for them; the first of each is the prefix of the first line and of the suggestion.
fn localize(rendered: &str, tag: &str) -> String {
    let swap = |text: &str, english: &str, key: &str| match text.find(english) {
        Some(at) => format!("{}{}{}", &text[..at], key, &text[at + english.len()..]),
        None => text.to_owned(),
    };
    let text = swap(rendered, "error:", &t_in!(tag, "cli_chrome_error"));
    swap(&text, "tip:", &t_in!(tag, "cli_chrome_tip"))
}

/// Print a clap error and exit like `clap::Error::exit` (code 2 for a usage error; help and version go to stdout, code
/// 0), with the prefixes of a usage error in the display language. English runs clap's own path untouched.
pub fn exit_clap(e: clap::Error, tag: &str) -> ! {
    if tag == atlas_core::lang::DEFAULT || !e.use_stderr() {
        e.exit()
    }
    let rendered = e.render();
    let text = if std::io::stderr().is_terminal() {
        rendered.ansi().to_string()
    } else {
        rendered.to_string()
    };
    eprint!("{}", localize(&text, tag));
    std::process::exit(e.exit_code())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_usage_error_gets_the_prefixes_of_the_language_and_keeps_the_rest() {
        let clap_text = "error: unrecognized subcommand 'confg'\n\n  tip: a similar subcommand exists: 'config'\n\nUsage: atlas <COMMAND>\n";
        assert_eq!(
            localize(clap_text, "pt-BR"),
            "erro: unrecognized subcommand 'confg'\n\n  dica: a similar subcommand exists: 'config'\n\nUsage: atlas <COMMAND>\n"
        );
        assert_eq!(localize(clap_text, "en"), clap_text);
    }

    #[test]
    fn only_the_first_error_word_is_swapped() {
        assert_eq!(
            localize("error: bad 'error:' value", "pt-BR"),
            "erro: bad 'error:' value"
        );
    }

    #[test]
    fn the_prefixes_come_from_the_catalog_in_each_language() {
        assert_eq!(t_in!("en", "cli_chrome_warning"), "warning:");
        assert_eq!(t_in!("pt-BR", "cli_chrome_warning"), "aviso:");
        assert_eq!(t_in!("pt-BR", "cli_chrome_caused_by"), "causado por:");
    }

    /// A real child that exits with a code and one that a signal ends: the words come from the catalog, never from
    /// `ExitStatus`'s English `Display`. Unknown shapes cannot reach the sentence raw.
    #[cfg(unix)]
    #[test]
    fn a_finished_child_is_worded_by_its_code_or_its_signal() {
        let code = std::process::Command::new("sh")
            .args(["-c", "exit 3"])
            .status()
            .unwrap();
        assert_eq!(
            exit_text(code),
            t_in!("en", "cli_chrome_exit_code", code = 3)
        );
        assert!(!exit_text(code).contains("exit status"), "{code}");
        let killed = std::process::Command::new("sh")
            .args(["-c", "kill -9 $$"])
            .status()
            .unwrap();
        assert_eq!(
            exit_text(killed),
            t_in!("en", "cli_chrome_exit_signal", signal = 9)
        );
        let pt = t_in!("pt-BR", "cli_chrome_exit_signal", signal = 9);
        assert!(pt.contains('9') && pt != exit_text(killed), "{pt}");
    }

    /// `atlas-core` words a failed update check in English (`curl not found …`); what a person reads is the catalog's,
    /// wherever the failure sits in the chain, and any other error keeps its own text.
    #[test]
    fn a_failed_update_check_is_worded_by_the_catalog_in_every_place_of_the_chain() {
        use atlas_core::github::GithubError;
        let missing = GithubError::Missing;
        let en = atlas_i18n::github_error_in("en", &missing);
        assert_ne!(en, missing.to_string());

        let bare = anyhow::Error::from(GithubError::Missing);
        assert_eq!(error_text(&bare), en);

        let wrapped =
            anyhow::Error::from(GithubError::Missing).context("Couldn't check for updates");
        assert_eq!(error_text(&wrapped), "Couldn't check for updates");
        let causes: Vec<String> = wrapped.chain().skip(1).map(cause_text).collect();
        assert_eq!(causes, [en]);

        assert_eq!(error_text(&anyhow::anyhow!("plain")), "plain");
    }
}
