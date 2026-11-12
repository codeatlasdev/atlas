//! Where things are. Every function takes its environment as arguments so the tests need no real
//! profile; `Env::real()` reads the live one.

use std::path::{Path, PathBuf};

/// The product folder under `%LOCALAPPDATA%` (the NSIS installer's per-user `$INSTDIR`).
const PRODUCT_DIR: &str = "Atlas";
/// Main binary of Atlas Desktop (`atlas_core::paths::LINUX_APP_BIN` + `.exe`).
pub const APP_EXE: &str = "atlas-desktop.exe";
/// The Tauri NSIS uninstaller, next to the app.
pub const NSIS_UNINSTALLER: &str = "uninstall.exe";
/// This program, copied into the install folder by the NSIS post-install hook so Settings › Apps
/// can run it (`packaging/windows/installer-hooks.nsh`).
pub const OWN_COPY: &str = "Atlas Uninstall.exe";

#[derive(Clone, Debug)]
pub struct Env {
    pub local_app_data: PathBuf,
    pub home: PathBuf,
    pub temp: PathBuf,
}

impl Env {
    #[cfg(windows)]
    pub fn real() -> Self {
        let home = atlas_core::paths::home_dir();
        Self {
            local_app_data: atlas_core::paths::local_app_data(&home),
            temp: std::env::temp_dir(),
            home,
        }
    }

    /// `%LOCALAPPDATA%\Atlas`.
    pub fn install_dir(&self) -> PathBuf {
        self.local_app_data.join(PRODUCT_DIR)
    }

    pub fn app_exe(&self) -> PathBuf {
        self.install_dir().join(APP_EXE)
    }

    pub fn nsis_uninstaller(&self) -> PathBuf {
        self.install_dir().join(NSIS_UNINSTALLER)
    }

    /// The CLI's own install (`atlas install`): `~/.atlas/bin/atlas.exe`.
    pub fn cli_exe(&self) -> PathBuf {
        self.home.join(".atlas").join("bin").join("atlas.exe")
    }

    /// The user's data folder, the one `atlas uninstall` removes.
    pub fn data_dir(&self) -> PathBuf {
        self.home.join(".atlas")
    }

    /// `%TEMP%\atlas-setup.log`: the one place a failure is explained.
    pub fn log_file(&self) -> PathBuf {
        self.temp.join("atlas-setup.log")
    }

    /// Where the embedded installer is unpacked: a folder of our own in `%TEMP%`, named after our
    /// pid so two runs never share it.
    pub fn payload_dir(&self, pid: u32) -> PathBuf {
        self.temp.join(format!("atlas-setup-{pid}"))
    }
}

/// The file name the unpacked NSIS installer gets.
#[cfg(windows)]
pub fn payload_name() -> &'static str {
    "Atlas-installer.exe"
}

/// `/D=<dir>`: NSIS wants it last and unquoted, even with spaces (same rule as
/// `atlas desktop`'s silent run).
pub fn nsis_dir_arg(dir: &Path) -> String {
    format!("/D={}", dir.display())
}

/// Is `p` safe to delete recursively as "Atlas's own folder": the product folder directly under
/// `%LOCALAPPDATA%`, never a drive root, the profile, or anything else a bad env could produce.
pub fn is_product_dir(p: &Path, local_app_data: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    !norm(local_app_data).is_empty() && norm(p) == norm(&local_app_data.join(PRODUCT_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Env {
        Env {
            local_app_data: PathBuf::from(r"C:\Users\ana\AppData\Local"),
            home: PathBuf::from(r"C:\Users\ana"),
            temp: PathBuf::from(r"C:\Users\ana\AppData\Local\Temp"),
        }
    }

    #[test]
    fn the_install_layout_matches_the_nsis_per_user_install() {
        // The same layout `atlas desktop` and atlas_core::paths::windows_app assume.
        let e = env();
        assert_eq!(
            e.app_exe(),
            Path::new(r"C:\Users\ana\AppData\Local")
                .join("Atlas")
                .join("atlas-desktop.exe")
        );
        assert_eq!(
            e.app_exe(),
            atlas_core::paths::windows_app(&e.local_app_data),
            "must agree with atlas_core::paths"
        );
        assert_eq!(e.nsis_uninstaller().file_name().unwrap(), "uninstall.exe");
    }

    #[test]
    fn data_and_cli_live_under_dot_atlas() {
        let e = env();
        assert!(e.cli_exe().starts_with(e.data_dir()));
        assert_eq!(e.log_file().file_name().unwrap(), "atlas-setup.log");
    }

    #[test]
    fn nsis_dir_argument_is_unquoted_even_with_spaces() {
        assert_eq!(
            nsis_dir_arg(Path::new(r"C:\Users\Ana Maria\AppData\Local\Atlas")),
            r"/D=C:\Users\Ana Maria\AppData\Local\Atlas"
        );
    }

    #[test]
    fn only_the_product_folder_counts_as_atlas_own() {
        let lad = Path::new(r"C:\Users\ana\AppData\Local");
        assert!(is_product_dir(
            Path::new(r"C:\Users\ana\AppData\Local\Atlas"),
            lad
        ));
        assert!(is_product_dir(
            Path::new("c:/users/ana/appdata/local/atlas/"),
            lad
        ));
        for bad in [
            r"C:\",
            r"C:\Users\ana",
            r"C:\Users\ana\AppData\Local",
            r"C:\Users\ana\AppData\Local\Atlas\sub",
            r"C:\Users\ana\AppData\Local\AtlasX",
        ] {
            assert!(!is_product_dir(Path::new(bad), lad), "{bad}");
        }
        assert!(
            !is_product_dir(Path::new("Atlas"), Path::new("")),
            "an empty env must not match"
        );
    }

    #[test]
    fn payload_dirs_differ_per_process() {
        let e = env();
        assert_ne!(e.payload_dir(1), e.payload_dir(2));
    }
}
