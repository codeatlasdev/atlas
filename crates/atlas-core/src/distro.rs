//! Which of the release's Linux packages the distro this machine runs takes (#267): read from
//! `/etc/os-release` (`ID`, `ID_LIKE`), never guessed from which files exist. `None` means the
//! AppImage, the format that runs anywhere.

use crate::pkgowner::PackageManager;

/// The package manager whose release asset matches the distro described by `os_release` (the text
/// of `/etc/os-release`), when this machine also has that package manager (`has`).
///
/// Debian and Ubuntu and their derivatives take the `.deb`, Fedora and the RHEL family the `.rpm`
/// through `dnf`, openSUSE the `.rpm` through `zypper`, Arch and its derivatives the pacman
/// package. Manjaro is Arch-like but holds its libraries back from Arch's, and the pacman package
/// is built against Arch's, so it gets the AppImage like every distro not named here.
pub fn native_manager(os_release: &str, has: impl Fn(&str) -> bool) -> Option<PackageManager> {
    let ids = ids(os_release);
    let is = |names: &[&str]| ids.iter().any(|i| names.contains(&i.as_str()));
    if is(&["manjaro"]) {
        None
    } else if is(&["arch"]) && has("pacman") {
        Some(PackageManager::Pacman)
    } else if is(&["debian", "ubuntu"]) && has("apt-get") && has("dpkg") {
        Some(PackageManager::Apt)
    } else if is(&["fedora", "rhel"]) && has("dnf") {
        Some(PackageManager::Dnf)
    } else if is(&["suse", "opensuse"]) && has("zypper") {
        Some(PackageManager::Zypper)
    } else {
        None
    }
}

/// `ID` and the words of `ID_LIKE`, lower case, quotes removed.
fn ids(os_release: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in os_release.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        if key == "ID" || key == "ID_LIKE" {
            let value = value.trim().trim_matches(['"', '\'']);
            out.extend(value.split_whitespace().map(str::to_ascii_lowercase));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: fn(&str) -> bool = |_| true;

    /// `/etc/os-release` of each distro, as shipped (ID and ID_LIKE lines are the real ones).
    #[test]
    fn each_distro_family_gets_its_own_package_format() {
        let cases = [
            (
                "NAME=\"Arch Linux\"\nID=arch\n",
                Some(PackageManager::Pacman),
            ),
            (
                "NAME=\"Omarchy\"\nID=omarchy\nID_LIKE=arch\n",
                Some(PackageManager::Pacman),
            ),
            (
                "PRETTY_NAME=\"Debian GNU/Linux 13\"\nID=debian\n",
                Some(PackageManager::Apt),
            ),
            (
                "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n",
                Some(PackageManager::Apt),
            ),
            (
                "NAME=\"Linux Mint\"\nID=linuxmint\nID_LIKE=\"ubuntu debian\"\n",
                Some(PackageManager::Apt),
            ),
            (
                "NAME=\"Fedora Linux\"\nID=fedora\n",
                Some(PackageManager::Dnf),
            ),
            (
                "NAME=\"Rocky Linux\"\nID=\"rocky\"\nID_LIKE=\"rhel centos fedora\"\n",
                Some(PackageManager::Dnf),
            ),
            (
                "NAME=\"openSUSE Tumbleweed\"\nID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n",
                Some(PackageManager::Zypper),
            ),
            // No package for these: the AppImage.
            ("NAME=\"Alpine Linux\"\nID=alpine\n", None),
            ("NAME=NixOS\nID=nixos\n", None),
            ("", None),
        ];
        for (os_release, want) in cases {
            assert_eq!(native_manager(os_release, ALL), want, "{os_release}");
        }
    }

    /// Manjaro says `ID_LIKE=arch`, but its libraries trail Arch's: the Arch-built package would not
    /// load there. It must not be sent the pacman package.
    #[test]
    fn manjaro_is_not_sent_the_arch_package() {
        let os = "NAME=\"Manjaro Linux\"\nID=manjaro\nID_LIKE=arch\n";
        assert_eq!(native_manager(os, ALL), None);
    }

    /// A distro whose package manager is not on this machine (a container image without `apt-get`,
    /// a derivative that dropped dnf) cannot install the package, so it gets the AppImage.
    #[test]
    fn a_missing_package_manager_means_the_appimage() {
        let ubuntu = "ID=ubuntu\nID_LIKE=debian\n";
        assert_eq!(native_manager(ubuntu, |n| n != "apt-get"), None);
        assert_eq!(native_manager(ubuntu, |n| n != "dpkg"), None);
        assert_eq!(native_manager("ID=fedora\n", |n| n != "dnf"), None);
        assert_eq!(native_manager("ID=arch\n", |n| n != "pacman"), None);
    }

    /// Only `ID` and `ID_LIKE` count: a `NAME` or `HOME_URL` that mentions a distro does not.
    #[test]
    fn only_id_and_id_like_decide() {
        let os =
            "NAME=\"Not Arch\"\nHOME_URL=\"https://arch.example\"\nID=void\nVERSION_ID=\"arch\"\n";
        assert_eq!(native_manager(os, ALL), None);
    }
}
