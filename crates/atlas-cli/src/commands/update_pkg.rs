//! `atlas update` for an Atlas that a Linux package manager owns (#144): how to install the
//! release asset (checksum first, then one escalated package-manager command). Who owns the
//! binary and which asset matches are `atlas_core::pkgowner`; the orchestration lives in
//! `self_update::run_update`.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use atlas_core::pkgowner::PackageManager;

/// This process runs as root (no escalation needed). Package installs only exist on Linux.
pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// What escalates the install command to root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Escalation {
    /// Already root: run it directly.
    None,
    /// `sudo`, only with a terminal for its password prompt.
    Sudo,
    /// `pkexec`: a graphical polkit prompt, usable without a terminal (the desktop app).
    Pkexec,
}

/// What `atlas update` does with a package install, decided before anything is downloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// Install it, escalating as given.
    Install(Escalation),
    /// Nothing here can install it (nobody confirmed, or nothing can ask for the password
    /// without a terminal): show the package manager's command instead. `installable` = running
    /// again with `--yes` would install it (the app offers that after its own confirmation).
    Show { installable: bool },
}

/// Pick how to become root. A terminal gets `sudo` (it prompts there). Without one (the
/// desktop app) only `pkexec`'s graphical prompt works, and only after a confirmation: `sudo`
/// would hang or fail on its password prompt, so it is never tried without a terminal.
pub fn plan(is_root: bool, tty: bool, yes: bool, has: impl Fn(&str) -> bool) -> Result<Plan> {
    if tty {
        return match (is_root, has("sudo"), has("pkexec")) {
            (true, _, _) => Ok(Plan::Install(Escalation::None)),
            (false, true, _) => Ok(Plan::Install(Escalation::Sudo)),
            (false, false, true) => Ok(Plan::Install(Escalation::Pkexec)),
            _ => bail!("neither sudo nor pkexec is available to install the package as root"),
        };
    }
    let escalation = if is_root {
        Some(Escalation::None)
    } else {
        has("pkexec").then_some(Escalation::Pkexec)
    };
    Ok(match escalation {
        Some(e) if yes => Plan::Install(e),
        e => Plan::Show {
            installable: e.is_some(),
        },
    })
}

/// The full command line: escalation prefix + package manager + file.
pub fn command_line(esc: Escalation, pm: PackageManager, file: &Path) -> Vec<String> {
    escalated(esc, pm.install_argv(file))
}

/// `argv` behind the escalation prefix (`sudo`, `pkexec`, or nothing for root).
pub fn escalated(esc: Escalation, mut argv: Vec<String>) -> Vec<String> {
    let prefix = match esc {
        Escalation::None => None,
        Escalation::Sudo => Some("sudo"),
        Escalation::Pkexec => Some("pkexec"),
    };
    if let Some(p) = prefix {
        argv.insert(0, p.to_string());
    }
    argv
}

/// Run `argv` attached to this terminal (sudo must be able to prompt) and fail on a non-zero
/// exit. Nothing has been escalated before this point: the checksum is already verified.
pub fn run_install(argv: &[String]) -> Result<()> {
    let (program, args) = argv.split_first().context("empty install command")?;
    let st = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("cannot run {program}"))?;
    if st.success() {
        Ok(())
    } else {
        bail!(failure(argv, st.code()))
    }
}

/// Why `argv` failed. `pkexec` says 126 when the user dismissed its prompt and 127 when it
/// could not authenticate (no polkit agent running): neither is a package manager error.
fn failure(argv: &[String], code: Option<i32>) -> String {
    let line = argv.join(" ");
    match (argv.first().map(String::as_str), code) {
        (Some("pkexec"), Some(126)) => {
            "Authentication was cancelled; nothing was installed".to_string()
        }
        (Some("pkexec"), Some(127)) => {
            "pkexec could not authenticate (is a polkit authentication agent running?); nothing was installed".to_string()
        }
        (_, Some(c)) => format!("`{line}` failed (exit status: {c})"),
        (_, None) => format!("`{line}` was killed by a signal"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_commands_use_the_right_manager() {
        let f = Path::new("/tmp/x/atlas.pkg");
        assert_eq!(
            command_line(Escalation::Sudo, PackageManager::Pacman, f),
            ["sudo", "pacman", "-U", "--noconfirm", "/tmp/x/atlas.pkg"]
        );
        assert_eq!(
            command_line(Escalation::Pkexec, PackageManager::Apt, f)[..3],
            ["pkexec", "apt-get", "install"]
        );
        assert_eq!(
            command_line(Escalation::None, PackageManager::Dnf, f)[..2],
            ["dnf", "install"]
        );
        let z = command_line(Escalation::Sudo, PackageManager::Zypper, f);
        assert!(z.contains(&"zypper".to_string()) && z.contains(&"install".to_string()));
    }

    // #278: the app has no terminal. `sudo` needs one for its password, so without a tty the
    // only way to become root is `pkexec`; with neither (or no confirmation) the user is shown
    // the command instead of an update that hangs or fails.
    #[test]
    fn plan_never_uses_sudo_without_a_terminal() {
        let both = |n: &str| n == "sudo" || n == "pkexec";
        let sudo_only = |n: &str| n == "sudo";
        let pkexec_only = |n: &str| n == "pkexec";
        let none = |_: &str| false;
        let plan = |root, tty, yes, has: &dyn Fn(&str) -> bool| plan(root, tty, yes, has).unwrap();

        // A terminal can ask for the password and the confirmation itself.
        assert_eq!(
            plan(false, true, false, &both),
            Plan::Install(Escalation::Sudo)
        );
        assert_eq!(
            plan(false, true, true, &both),
            Plan::Install(Escalation::Sudo)
        );
        assert_eq!(
            plan(false, true, false, &pkexec_only),
            Plan::Install(Escalation::Pkexec)
        );
        assert_eq!(
            plan(true, true, false, &none),
            Plan::Install(Escalation::None)
        );
        assert!(
            super::plan(false, true, false, none).is_err(),
            "no sudo, no pkexec, a tty"
        );

        // No terminal, no `--yes`: nobody confirmed. Show the command; `--yes` would work
        // only where something can ask for the password (root, or pkexec).
        assert_eq!(
            plan(false, false, false, &both),
            Plan::Show { installable: true }
        );
        assert_eq!(
            plan(false, false, false, &sudo_only),
            Plan::Show { installable: false }
        );
        assert_eq!(
            plan(true, false, false, &none),
            Plan::Show { installable: true }
        );

        // No terminal, confirmed: pkexec's graphical prompt, never sudo.
        assert_eq!(
            plan(false, false, true, &both),
            Plan::Install(Escalation::Pkexec)
        );
        assert_eq!(
            plan(false, false, true, &sudo_only),
            Plan::Show { installable: false }
        );
        assert_eq!(
            plan(false, false, true, &none),
            Plan::Show { installable: false }
        );
        assert_eq!(
            plan(true, false, true, &none),
            Plan::Install(Escalation::None)
        );
    }

    #[test]
    fn a_dismissed_pkexec_prompt_is_not_a_raw_failure() {
        let argv = |p: &str| vec![p.to_string(), "apt-get".into(), "install".into()];
        assert!(failure(&argv("pkexec"), Some(126)).contains("cancelled"));
        assert!(failure(&argv("pkexec"), Some(127)).contains("polkit"));
        // Any other failure (and any other program) keeps the command and the exit status.
        let other = failure(&argv("pkexec"), Some(100));
        assert!(
            other.contains("pkexec apt-get install") && other.contains("100"),
            "{other}"
        );
        assert!(failure(&argv("sudo"), Some(126)).contains("sudo apt-get install"));
    }
}
