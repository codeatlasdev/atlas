//! `atlas update` for an Atlas that a Linux package manager owns (#144): who owns the binary,
//! which release asset matches, and how to install it (checksum first, then one escalated
//! package-manager command). The orchestration lives in `self_update::run_update`.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

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

/// The package manager that owns `/usr/bin/atlas`, i.e. the asset format to install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    /// Arch: `pacman -U codeatlas-atlas-bin-*.pkg.tar.zst`.
    Pacman,
    /// Debian, Ubuntu: `apt install ./atlas_*_amd64.deb`.
    Apt,
    /// Fedora and friends: `dnf install ./atlas-*.x86_64.rpm`.
    Dnf,
    /// openSUSE: `zypper install ./atlas-*.x86_64.rpm`.
    Zypper,
}

impl PackageManager {
    /// The release asset for `version` (SemVer, no `v`). Names match `release.yml` /
    /// `scripts/generate-manifest.py`; pacman drops the `-` of a prerelease (`1.6.0-beta.2` is
    /// `1.6.0beta.2`, see `packaging/arch/render.sh`).
    pub fn asset(self, version: &str) -> String {
        match self {
            Self::Pacman => format!(
                "codeatlas-atlas-bin-{}-1-x86_64.pkg.tar.zst",
                version.replacen('-', "", 1)
            ),
            Self::Apt => format!("atlas_{version}_amd64.deb"),
            Self::Dnf | Self::Zypper => format!("atlas-{version}-1.x86_64.rpm"),
        }
    }

    /// Program and arguments that install the downloaded `file` (run under `sudo`/`pkexec`).
    pub fn install_argv(self, file: &Path) -> Vec<String> {
        let f = file.display().to_string();
        match self {
            Self::Pacman => ["pacman", "-U", "--noconfirm", &f]
                .map(String::from)
                .to_vec(),
            Self::Apt => ["apt-get", "install", "-y", &f].map(String::from).to_vec(),
            Self::Dnf => ["dnf", "install", "-y", &f].map(String::from).to_vec(),
            Self::Zypper => [
                "zypper",
                "--non-interactive",
                "install",
                "--allow-unsigned-rpm",
                &f,
            ]
            .map(String::from)
            .to_vec(),
        }
    }

    /// The command a human runs by hand (printed by the notice and `--check`).
    pub fn manual_command(self, version: &str) -> String {
        let asset = self.asset(version);
        match self {
            Self::Pacman => format!("sudo pacman -U {asset}"),
            Self::Apt => format!("sudo apt install ./{asset}"),
            Self::Dnf => format!("sudo dnf install ./{asset}"),
            Self::Zypper => format!("sudo zypper install ./{asset}"),
        }
    }

    /// Short name for messages and the `--json` report.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Zypper => "zypper",
        }
    }
}

/// How a package-owned binary is found out: by asking each package manager's database, in the
/// order that makes a wrong answer impossible (a file is owned by at most one database).
/// `probe` runs `program args…` and returns its stdout on exit 0, so tests stub it with the
/// real tools' output shapes.
pub fn owner_with(
    path: &Path,
    mut probe: impl FnMut(&str, &[&str]) -> Option<String>,
) -> Option<PackageManager> {
    let p = path.to_str()?;
    if let Some(out) = probe("pacman", &["-Qo", p]) {
        // `/usr/bin/atlas is owned by codeatlas-atlas-bin 1.5.0alpha.13-2`
        if out.contains(" is owned by ") {
            return Some(PackageManager::Pacman);
        }
    }
    if let Some(out) = probe("dpkg", &["-S", p]) {
        // `atlas: /usr/bin/atlas`
        if out
            .lines()
            .any(|l| l.rsplit_once(": ").is_some_and(|(_, f)| f == p))
        {
            return Some(PackageManager::Apt);
        }
    }
    if let Some(out) = probe("rpm", &["-qf", p]) {
        // `atlas-1.5.0-1.x86_64`; `file … is not owned by any package` exits 1 and never lands here.
        if !out.trim().is_empty() && !out.contains("not owned by") {
            return Some(rpm_manager());
        }
    }
    None
}

/// An rpm system updates through `zypper` on openSUSE and `dnf` elsewhere.
fn rpm_manager() -> PackageManager {
    rpm_manager_on(|name| atlas_core::paths::which(name).is_some())
}

fn rpm_manager_on(has: impl Fn(&str) -> bool) -> PackageManager {
    if has("dnf") {
        PackageManager::Dnf
    } else if has("zypper") {
        PackageManager::Zypper
    } else {
        PackageManager::Dnf
    }
}

/// [`owner_with`] against the real package managers on `PATH`.
pub fn owner(path: &Path) -> Option<PackageManager> {
    owner_with(path, |program, args| {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    })
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

    fn probe_table<'a>(
        answers: &'a [(&'a str, Option<&'a str>)],
    ) -> impl FnMut(&str, &[&str]) -> Option<String> + 'a {
        move |program, _| {
            answers
                .iter()
                .find(|(p, _)| *p == program)
                .and_then(|(_, out)| out.map(String::from))
        }
    }

    #[test]
    fn ownership_detection_table() {
        let bin = Path::new("/usr/bin/atlas");
        let arch = [(
            "pacman",
            Some("/usr/bin/atlas is owned by codeatlas-atlas-bin 1.5.0alpha.13-2\n"),
        )];
        assert_eq!(
            owner_with(bin, probe_table(&arch)),
            Some(PackageManager::Pacman)
        );
        let deb = [("pacman", None), ("dpkg", Some("atlas: /usr/bin/atlas\n"))];
        assert_eq!(
            owner_with(bin, probe_table(&deb)),
            Some(PackageManager::Apt)
        );
        let rpm = [
            ("pacman", None),
            ("dpkg", None),
            ("rpm", Some("atlas-1.5.0-1.x86_64\n")),
        ];
        let got = owner_with(bin, probe_table(&rpm));
        assert!(
            matches!(got, Some(PackageManager::Dnf | PackageManager::Zypper)),
            "{got:?}"
        );
        // Not owned by any database (a hand-copied /usr/bin/atlas): fall back to the path rule.
        let none = [("pacman", None), ("dpkg", None), ("rpm", None)];
        assert_eq!(owner_with(bin, probe_table(&none)), None);
    }

    /// `dpkg -S` also prints diversions and prefix matches for other files: only a
    /// `<package>: <exactly our path>` line counts.
    #[test]
    fn dpkg_output_must_name_the_binary() {
        let bin = Path::new("/usr/bin/atlas");
        let other = [(
            "dpkg",
            Some("diversion by foo from: /usr/bin/atlas-x\natlas-x: /usr/bin/atlas-x\n"),
        )];
        assert_eq!(owner_with(bin, probe_table(&other)), None);
    }

    #[test]
    fn rpm_manager_prefers_dnf_then_zypper() {
        assert_eq!(
            rpm_manager_on(|n| n == "dnf" || n == "zypper"),
            PackageManager::Dnf
        );
        assert_eq!(rpm_manager_on(|n| n == "zypper"), PackageManager::Zypper);
        assert_eq!(rpm_manager_on(|_| false), PackageManager::Dnf);
    }

    #[test]
    fn asset_names_match_the_release_workflow() {
        assert_eq!(PackageManager::Apt.asset("1.6.0"), "atlas_1.6.0_amd64.deb");
        assert_eq!(
            PackageManager::Dnf.asset("1.6.0"),
            "atlas-1.6.0-1.x86_64.rpm"
        );
        assert_eq!(
            PackageManager::Zypper.asset("1.6.0-beta.2"),
            "atlas-1.6.0-beta.2-1.x86_64.rpm"
        );
        assert_eq!(
            PackageManager::Pacman.asset("1.6.0-beta.2"),
            "codeatlas-atlas-bin-1.6.0beta.2-1-x86_64.pkg.tar.zst"
        );
        assert_eq!(
            PackageManager::Pacman.asset("1.5.0"),
            "codeatlas-atlas-bin-1.5.0-1-x86_64.pkg.tar.zst"
        );
    }

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

    #[test]
    fn manual_command_is_what_a_human_runs() {
        assert_eq!(
            PackageManager::Apt.manual_command("1.6.0"),
            "sudo apt install ./atlas_1.6.0_amd64.deb"
        );
        assert_eq!(
            PackageManager::Pacman.manual_command("1.6.0"),
            "sudo pacman -U codeatlas-atlas-bin-1.6.0-1-x86_64.pkg.tar.zst"
        );
    }
}
