//! `atlas <path>` / `atlas-desktop <path>`: open a folder or file in the running app (or the one starting up).
//!
//! The page owns selection and editors, so the path travels as an [`OpenReq`]: queued in [`Pending`] (the page may not be
//! mounted yet on a cold start) and announced with `open://request`; the page drains the queue with `open_pending` once
//! its stores are loaded and again on every event.

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// One path to open: the absolute path as given (a folder or a file); the page resolves its project.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct OpenReq {
    pub path: String,
}

#[derive(Default)]
pub struct Pending(Mutex<Vec<OpenReq>>);

/// The paths a launch asked to open: every argv entry after the program name that is not a flag, made absolute
/// against `cwd` (a second launch runs in its own directory, not the running app's).
pub fn paths_from_argv(argv: &[String], cwd: &Path) -> Vec<OpenReq> {
    argv.iter()
        .skip(1)
        .filter(|a| !a.starts_with('-') && !a.is_empty())
        .map(|a| {
            let p = Path::new(a);
            let abs: PathBuf = if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            };
            OpenReq {
                path: atlas_core::paths::normalize(&abs)
                    .to_string_lossy()
                    .into_owned(),
            }
        })
        .collect()
}

/// The paths of the `file://` URLs macOS hands the app when the user picks "Open With" (or drops a file on the Dock
/// icon): `RunEvent::Opened`. Other schemes are not ours to open. A folder arrives with a trailing slash
/// (`file:///n/Notes/`), which is dropped so it is the same path `atlas /n/Notes` queues.
// Its tests read real `file://` URLs, which only parse to paths on Unix; on Windows the function is unused.
#[cfg(any(target_os = "macos", all(test, unix)))]
pub fn paths_from_urls(urls: &[tauri::Url]) -> Vec<OpenReq> {
    urls.iter()
        .filter_map(|u| u.to_file_path().ok())
        .map(|p| OpenReq {
            path: atlas_core::paths::normalize(&p.components().collect::<PathBuf>())
                .to_string_lossy()
                .into_owned(),
        })
        .collect()
}

/// Queue `reqs`, tell the page, and bring the window forward.
pub fn open(app: &AppHandle, reqs: Vec<OpenReq>) {
    if reqs.is_empty() {
        return;
    }
    if let Some(p) = app.try_state::<Pending>() {
        p.0.lock().extend(reqs);
    }
    let _ = app.emit("open://request", ());
}

/// What the page has not opened yet (drains the queue).
#[tauri::command]
pub fn open_pending(pending: tauri::State<'_, Pending>) -> Vec<OpenReq> {
    std::mem::take(&mut *pending.0.lock())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn argv_paths_skip_the_program_and_flags_and_resolve_against_the_launch_dir() {
        let cwd = Path::new(if cfg!(windows) { "C:/work" } else { "/work" });
        let abs = if cfg!(windows) { "C:/p/app" } else { "/p/app" };
        let got = paths_from_argv(&argv(&["atlas-desktop", "--flag", "sub", abs, ""]), cwd);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got[0].path.ends_with("work/sub"), "{}", got[0].path);
        assert_eq!(got[1].path, abs);
        assert!(paths_from_argv(&argv(&["atlas-desktop"]), cwd).is_empty());
    }

    // Finder hands the app percent-encoded `file://` URLs; a path with a space or an accent must arrive decoded, and a
    // URL that is not a file (a link some app sent) must not become a path.
    #[cfg(unix)]
    #[test]
    fn opened_urls_become_decoded_paths_and_other_schemes_are_dropped() {
        let urls = [
            "file:///Users/m/My%20Notes/caf%C3%A9.md",
            "https://example.com/a.md",
            "file:///tmp/a/../b.txt",
        ]
        .map(|u| tauri::Url::parse(u).unwrap());
        let got = paths_from_urls(&urls);
        let paths: Vec<_> = got.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["/Users/m/My Notes/café.md", "/tmp/b.txt"]);
    }

    // "Open With" with several files selected, and a folder (Windows/Linux pass every path as an argument, macOS as a
    // URL each): all of them reach the queue, in order, as the page opens them one by one.
    #[test]
    fn several_files_and_a_folder_all_reach_the_queue_in_order() {
        let cwd = Path::new(if cfg!(windows) { "C:/work" } else { "/work" });
        let (a, dir, b) = if cfg!(windows) {
            ("C:/n/a.md", "C:/n/Notes folder", "C:/n/b file.txt")
        } else {
            ("/n/a.md", "/n/Notes folder", "/n/b file.txt")
        };
        let got = paths_from_argv(&argv(&["Atlas.exe", a, dir, b]), cwd);
        let paths: Vec<_> = got.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, [a, dir, b]);
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_url_and_several_file_urls_keep_their_order() {
        let urls = [
            "file:///n/a.md",
            "file:///n/Notes%20folder/",
            "file:///n/b.txt",
        ]
        .map(|u| tauri::Url::parse(u).unwrap());
        let got = paths_from_urls(&urls);
        let paths: Vec<_> = got.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["/n/a.md", "/n/Notes folder", "/n/b.txt"]);
    }

    // A second launch ("Open With" while Atlas runs) is a new process with its own cwd: a relative path is made
    // absolute against THAT cwd, not the running app's, before it is forwarded to the window.
    #[test]
    fn a_second_launch_forwards_relative_paths_against_its_own_directory() {
        let cwd = Path::new(if cfg!(windows) { "C:/other" } else { "/other" });
        let got = paths_from_argv(&argv(&["atlas-desktop", "notes.md"]), cwd);
        assert!(got[0].path.ends_with("other/notes.md"), "{}", got[0].path);
    }
    #[test]
    fn a_launch_without_a_path_queues_nothing() {
        let p = Pending::default();
        assert!(p.0.lock().is_empty());
    }
}
