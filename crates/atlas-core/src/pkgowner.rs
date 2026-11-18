//! Which Linux package manager owns a binary, and the release asset and install command that
//! match it. Shared by `atlas update` (which installs the asset) and the diagnostics probe
//! (`probe`, which only names the owner), so the two cannot disagree about who owns `/usr/bin/atlas`.

use std::path::Path;
use std::process::{Command, Stdio};

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

/// An installed package that owns a file: who removes it, and under which name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub manager: PackageManager,
    /// The name the package manager knows it by (`codeatlas-atlas-bin` on Arch, `atlas` elsewhere).
    pub name: String,
}

impl Package {
    /// Program and arguments that remove the package without prompting (run under `sudo`/`pkexec`).
    pub fn remove_argv(&self) -> Vec<String> {
        let n = self.name.as_str();
        let argv: &[&str] = match self.manager {
            PackageManager::Pacman => &["pacman", "-R", "--noconfirm", n],
            PackageManager::Apt => &["apt-get", "remove", "-y", n],
            PackageManager::Dnf => &["dnf", "remove", "-y", n],
            PackageManager::Zypper => &["zypper", "--non-interactive", "remove", n],
        };
        argv.iter().map(|s| s.to_string()).collect()
    }

    /// The command a human runs by hand.
    pub fn manual_remove_command(&self) -> String {
        let n = &self.name;
        match self.manager {
            PackageManager::Pacman => format!("sudo pacman -R {n}"),
            PackageManager::Apt => format!("sudo apt remove {n}"),
            PackageManager::Dnf => format!("sudo dnf remove {n}"),
            PackageManager::Zypper => format!("sudo zypper remove {n}"),
        }
    }
}

/// How a package-owned file is found out: by asking each package manager's database, in the
/// order that makes a wrong answer impossible (a file is owned by at most one database).
/// `probe` runs `program args…` and returns its stdout on exit 0, so tests stub it with the
/// real tools' output shapes.
pub fn package_with(
    path: &Path,
    mut probe: impl FnMut(&str, &[&str]) -> Option<String>,
) -> Option<Package> {
    let p = path.to_str()?;
    let own = |manager, name: &str| {
        let name = name.trim();
        (!name.is_empty()).then(|| Package {
            manager,
            name: name.to_string(),
        })
    };
    if let Some(out) = probe("pacman", &["-Qo", p]) {
        // `/usr/bin/atlas is owned by codeatlas-atlas-bin 1.5.0alpha.13-2`
        if let Some((_, rest)) = out.split_once(" is owned by ")
            && let Some(found) = own(
                PackageManager::Pacman,
                rest.split_whitespace().next().unwrap_or(""),
            )
        {
            return Some(found);
        }
    }
    if let Some(out) = probe("dpkg", &["-S", p]) {
        // `atlas: /usr/bin/atlas`; a file two packages share lists `a, b: /path`, and a
        // multiarch package is `atlas:amd64: /path`.
        let owner = out
            .lines()
            .filter_map(|l| l.rsplit_once(": "))
            .find(|(_, f)| *f == p);
        if let Some((names, _)) = owner {
            let first = names.split(',').next().unwrap_or(names);
            if let Some(found) = own(
                PackageManager::Apt,
                first.split(':').next().unwrap_or(first),
            ) {
                return Some(found);
            }
        }
    }
    // `%{NAME}` alone: `file … is not owned by any package` exits 1 and never lands here.
    if let Some(out) = probe("rpm", &["-qf", "--qf", "%{NAME}\n", p])
        && !out.contains("not owned by")
        && let Some(found) = own(rpm_manager(), out.lines().next().unwrap_or(""))
    {
        return Some(found);
    }
    None
}

/// The package manager that owns `path`, if any: [`package_with`] without the name.
pub fn owner_with(
    path: &Path,
    probe: impl FnMut(&str, &[&str]) -> Option<String>,
) -> Option<PackageManager> {
    package_with(path, probe).map(|p| p.manager)
}

/// An rpm system updates through `zypper` on openSUSE and `dnf` elsewhere.
fn rpm_manager() -> PackageManager {
    rpm_manager_on(|name| crate::paths::which(name).is_some())
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

/// The real package managers on `PATH`: run `program args…`, stdout on exit 0.
fn run_probe(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// [`owner_with`] against the real package managers on `PATH`.
pub fn owner(path: &Path) -> Option<PackageManager> {
    owner_with(path, run_probe)
}

/// [`package_with`] against the real package managers on `PATH`.
pub fn package(path: &Path) -> Option<Package> {
    package_with(path, run_probe)
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

    /// #277: `atlas uninstall` has to name the package to remove, not guess `atlas`: Arch's is
    /// `codeatlas-atlas-bin`, and `dpkg -S` / `rpm -qf` print their own shapes around the name.
    #[test]
    fn the_package_name_comes_out_of_each_managers_own_answer() {
        let bin = Path::new("/usr/bin/atlas");
        let name = |answers: &[(&str, Option<&str>)]| {
            package_with(bin, probe_table(answers)).map(|p| (p.manager, p.name))
        };
        assert_eq!(
            name(&[(
                "pacman",
                Some("/usr/bin/atlas is owned by codeatlas-atlas-bin 1.5.0alpha.13-2\n")
            )]),
            Some((PackageManager::Pacman, "codeatlas-atlas-bin".into()))
        );
        assert_eq!(
            name(&[("dpkg", Some("atlas: /usr/bin/atlas\n"))]),
            Some((PackageManager::Apt, "atlas".into()))
        );
        // Multiarch and shared-file spellings of the same answer.
        assert_eq!(
            name(&[("dpkg", Some("atlas:amd64: /usr/bin/atlas\n"))]),
            Some((PackageManager::Apt, "atlas".into()))
        );
        assert_eq!(
            name(&[("dpkg", Some("atlas, atlas-extra: /usr/bin/atlas\n"))]),
            Some((PackageManager::Apt, "atlas".into()))
        );
        // `rpm -qf --qf %{NAME}\n`: the name alone.
        let rpm = name(&[("rpm", Some("atlas\n"))]).unwrap();
        assert_eq!(rpm.1, "atlas");
        assert!(matches!(
            rpm.0,
            PackageManager::Dnf | PackageManager::Zypper
        ));
        assert_eq!(name(&[("pacman", Some("")), ("rpm", Some("\n"))]), None);
    }

    #[test]
    fn removal_commands_name_the_package_for_each_manager() {
        let pkg = |manager, name: &str| Package {
            manager,
            name: name.into(),
        };
        let arch = pkg(PackageManager::Pacman, "codeatlas-atlas-bin");
        assert_eq!(
            arch.remove_argv(),
            ["pacman", "-R", "--noconfirm", "codeatlas-atlas-bin"]
        );
        assert_eq!(
            arch.manual_remove_command(),
            "sudo pacman -R codeatlas-atlas-bin"
        );
        let deb = pkg(PackageManager::Apt, "atlas");
        assert_eq!(deb.remove_argv(), ["apt-get", "remove", "-y", "atlas"]);
        assert_eq!(deb.manual_remove_command(), "sudo apt remove atlas");
        let rpm = pkg(PackageManager::Dnf, "atlas");
        assert_eq!(rpm.remove_argv(), ["dnf", "remove", "-y", "atlas"]);
        assert_eq!(rpm.manual_remove_command(), "sudo dnf remove atlas");
        let zyp = pkg(PackageManager::Zypper, "atlas");
        assert_eq!(
            zyp.remove_argv(),
            ["zypper", "--non-interactive", "remove", "atlas"]
        );
        assert_eq!(zyp.manual_remove_command(), "sudo zypper remove atlas");
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
