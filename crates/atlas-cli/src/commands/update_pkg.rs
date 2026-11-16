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
    /// `sudo` (needs a terminal for its password prompt unless it is passwordless).
    Sudo,
    /// `pkexec`: a graphical polkit prompt, usable without a terminal (the desktop app).
    Pkexec,
}

/// Pick how to become root. A terminal gets `sudo`; without one (the desktop app) `pkexec`
/// shows a graphical prompt, and `sudo` is the last resort (it works when passwordless).
pub fn escalation(is_root: bool, tty: bool, has: impl Fn(&str) -> bool) -> Result<Escalation> {
    if is_root {
        return Ok(Escalation::None);
    }
    match (tty, has("sudo"), has("pkexec")) {
        (true, true, _) => Ok(Escalation::Sudo),
        (false, _, true) => Ok(Escalation::Pkexec),
        (false, true, false) => Ok(Escalation::Sudo),
        (true, false, true) => Ok(Escalation::Pkexec),
        _ => bail!("neither sudo nor pkexec is available to install the package as root"),
    }
}

/// The full command line: escalation prefix + package manager + file.
pub fn command_line(esc: Escalation, pm: PackageManager, file: &Path) -> Vec<String> {
    let mut argv = match esc {
        Escalation::None => Vec::new(),
        Escalation::Sudo => vec!["sudo".to_string()],
        Escalation::Pkexec => vec!["pkexec".to_string()],
    };
    argv.extend(pm.install_argv(file));
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
        bail!("`{}` failed ({st})", argv.join(" "))
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

    #[test]
    fn escalation_choice() {
        let both = |n: &str| n == "sudo" || n == "pkexec";
        assert_eq!(
            escalation(true, false, |_| false).unwrap(),
            Escalation::None
        );
        assert_eq!(escalation(false, true, both).unwrap(), Escalation::Sudo);
        assert_eq!(escalation(false, false, both).unwrap(), Escalation::Pkexec);
        assert_eq!(
            escalation(false, false, |n| n == "sudo").unwrap(),
            Escalation::Sudo,
            "no tty and no pkexec: sudo may still be passwordless"
        );
        assert_eq!(
            escalation(false, true, |n| n == "pkexec").unwrap(),
            Escalation::Pkexec
        );
        assert!(escalation(false, true, |_| false).is_err());
    }
}
