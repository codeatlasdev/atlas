//! The bundle id moved from `dev.codeatlas.atlas` to `br.com.codeatlas.atlas` (dev builds:
//! `br.com.codeatlas.atlas.dev`). The OS keys the webview's storage by that id, so without this
//! an update would come up with a fresh localStorage: theme, preset and every UI preference gone.
//! Before the first window, copy the old id's webview data to the new id once. A copy, not a
//! move: going back to an older build still finds its data.

use std::io;
use std::path::{Path, PathBuf};

pub const LEGACY_ID: &str = "dev.codeatlas.atlas";

/// Where the webview keeps its data for bundle id `id`, as the platform's webview lays it out.
pub fn webview_dir(home: &Path, id: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/WebKit").join(id) // WKWebView's default data store
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Local"))
            .join(id) // Tauri's WebView2 user data folder lives under here (EBWebView)
    } else {
        home.join(".local/share").join(id) // WebKitGTK data directory Tauri sets
    }
}

/// Copies `from` to `to` when `to` does not exist yet. Returns whether it copied.
pub fn migrate(from: &Path, to: &Path) -> io::Result<bool> {
    if to.exists() || !from.is_dir() {
        return Ok(false);
    }
    let staging = to.with_extension("migrating");
    let _ = std::fs::remove_dir_all(&staging);
    copy_dir(from, &staging)?;
    // Rename last, so a crash mid-copy never leaves a half store that looks migrated.
    std::fs::rename(&staging, to)?;
    Ok(true)
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        let t = e.file_type()?;
        if t.is_dir() {
            copy_dir(&src, &dst)?;
        } else if t.is_file() {
            std::fs::copy(&src, &dst)?;
        } // sockets and symlinks of a running webview are not data
    }
    Ok(())
}

/// Runs the migration for this build's `id`; never fails the launch.
pub fn run(id: &str) {
    if id == LEGACY_ID {
        return;
    }
    let Some(home) = dirs::home_dir() else { return };
    let (from, to) = (webview_dir(&home, LEGACY_ID), webview_dir(&home, id));
    match migrate(&from, &to) {
        Ok(true) => eprintln!(
            "atlas: moved webview data {} -> {}",
            from.display(),
            to.display()
        ),
        Ok(false) => {}
        Err(e) => eprintln!("atlas: webview data not migrated ({e}); starting with defaults"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    #[test]
    fn copies_the_old_store_once_and_keeps_the_old_one() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("old"), t.path().join("new"));
        write(&old.join("WebsiteData/LocalStorage/tauri.sqlite3"), "prefs");
        assert!(migrate(&old, &new).unwrap());
        assert_eq!(
            std::fs::read_to_string(new.join("WebsiteData/LocalStorage/tauri.sqlite3")).unwrap(),
            "prefs"
        );
        assert!(
            old.join("WebsiteData/LocalStorage/tauri.sqlite3").exists(),
            "an older build still finds its data"
        );
        assert!(!t.path().join("new.migrating").exists());
    }

    #[test]
    fn never_overwrites_a_store_the_new_id_already_has() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("old"), t.path().join("new"));
        write(&old.join("a"), "old");
        write(&new.join("a"), "newer");
        assert!(!migrate(&old, &new).unwrap());
        assert_eq!(std::fs::read_to_string(new.join("a")).unwrap(), "newer");
    }

    #[test]
    fn nothing_to_do_on_a_fresh_install() {
        let t = tempfile::tempdir().unwrap();
        assert!(!migrate(&t.path().join("none"), &t.path().join("new")).unwrap());
        assert!(!t.path().join("new").exists());
    }

    #[test]
    fn a_leftover_half_copy_from_a_crash_is_redone() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("old"), t.path().join("new"));
        write(&old.join("a"), "full");
        write(&t.path().join("new.migrating/partial"), "x");
        assert!(migrate(&old, &new).unwrap());
        assert!(!new.join("partial").exists());
        assert_eq!(std::fs::read_to_string(new.join("a")).unwrap(), "full");
    }
}
