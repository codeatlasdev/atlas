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

    #[test]
    fn a_launch_without_a_path_queues_nothing() {
        let p = Pending::default();
        assert!(p.0.lock().is_empty());
    }
}
