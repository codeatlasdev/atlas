//! The app side of the diagnostics bundle behind "Report" and "Copy details": the app log
//! (written by [`log`] and by the panic hook [`install_panic_hook`]), the in-memory ring of host
//! incidents, and the call that gathers the bundle. What a bundle holds, how it is probed and
//! rendered, and the redaction are `atlas_host::bundle` and `atlas_core::probe`, shared with
//! `atlas doctor --bundle`.
//!
//! The frontend never assembles it by hand: it calls `host_diagnostics` and renders the
//! `Diagnostics` it gets (camelCase; the contract tests in `atlas_host::bundle` serialize the
//! real value). Reuses the existing error screen (`lib/errors/ErrorPanel.svelte`).

use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;
use std::sync::LazyLock;

use atlas_host::bundle::{Diagnostics, Incident, Versions};
use atlas_host::paths::app_log as app_log_path;
use parking_lot::Mutex;

/// Incidents kept in memory.
const MAX_INCIDENTS: usize = 20;
/// The app log is rotated past this size (one `.1` generation), so it cannot grow unbounded.
const MAX_LOG_BYTES: u64 = 512 * 1024;

pub use atlas_core::redact::redact;

static INCIDENTS: LazyLock<Mutex<VecDeque<Incident>>> = LazyLock::new(Mutex::default);

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Record a host incident: kept in memory for the next report and appended to the app log.
pub fn incident(kind: &str, detail: &str) {
    let detail = redact(detail);
    {
        let mut q = INCIDENTS.lock();
        if q.len() >= MAX_INCIDENTS {
            q.pop_front();
        }
        q.push_back(Incident {
            at: now(),
            kind: kind.to_string(),
            detail: detail.clone(),
        });
    }
    log(&format!("{kind}: {detail}"));
}

pub fn incidents() -> Vec<Incident> {
    INCIDENTS.lock().iter().cloned().collect()
}

/// Append one timestamped line to the app log of the default base dir.
pub fn log(line: &str) {
    log_in(&atlas_host::paths::base(), line);
}

/// [`log`] for the log under `base`; rotates once past [`MAX_LOG_BYTES`].
pub fn log_in(base: &Path, line: &str) {
    let path = app_log_path(base);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{} {}", now(), redact(line).replace('\n', "\\n"));
    }
}

/// A Rust panic of the app: the message and location go to the app log and the incident ring, and a
/// `crash-*.toml` with the backtrace goes to `atlas_core::crash::crash_dir()` (what the bundle's
/// `crashes.atlas` reads). Release Windows builds have no console (`windows_subsystem = "windows"`), so stderr is lost there:
/// these files are the only record. Chains to the previous hook, so debug-build behavior stays.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic payload");
        let at = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        incident("panic", &format!("{msg} ({at})"));
        let _ = atlas_core::crash::write_crash_report(msg, &at);
        prev(info);
    }));
}

/// A page error (window `error`, `unhandledrejection`, a crashed view) recorded in the app log on every build.
/// The console reaches Rust only through the debug bridge, so without this an installed app (no devtools)
/// loses every frontend failure. Bounded: one line, at most 2 KB, redacted by [`incident`].
#[tauri::command]
pub fn diag_frontend_error(place: String, message: String) {
    let m: String = message
        .chars()
        .take(2048)
        .collect::<String>()
        .replace(['\r', '\n'], " ");
    incident("frontend", &format!("{place}: {m}"));
}

/// `x.y.z` of a GTK-style library out of its `*_get_major/minor/micro_version` functions, looked up
/// in the libraries this process already loaded: an AppImage bundles its own WebKitGTK and GTK, so
/// no package database can name them.
#[cfg(target_os = "linux")]
fn loaded_version(names: [&str; 3]) -> Option<String> {
    let get = |name: &str| -> Option<u32> {
        let c = std::ffi::CString::new(name).ok()?;
        // SAFETY: a symbol lookup by name has no preconditions.
        let p = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c.as_ptr()) };
        if p.is_null() {
            return None;
        }
        // SAFETY: these functions are `guint f(void)` in GTK 3 and WebKitGTK 2.
        let f = unsafe { std::mem::transmute::<*mut libc::c_void, extern "C" fn() -> u32>(p) };
        Some(f())
    };
    Some(format!(
        "{}.{}.{}",
        get(names[0])?,
        get(names[1])?,
        get(names[2])?
    ))
}

/// What this process knows about its web view; empty off Linux (macOS and Windows ask the system).
fn runtime() -> atlas_core::probe::Runtime {
    #[cfg(target_os = "linux")]
    {
        atlas_core::probe::Runtime {
            webkitgtk: loaded_version([
                "webkit_get_major_version",
                "webkit_get_minor_version",
                "webkit_get_micro_version",
            ]),
            gtk: loaded_version([
                "gtk_get_major_version",
                "gtk_get_minor_version",
                "gtk_get_micro_version",
            ]),
        }
    }
    #[cfg(not(target_os = "linux"))]
    atlas_core::probe::Runtime::default()
}

/// The bundle for this app: probes the machine (bounded subprocess queries: call it off the async
/// runtime) and reads the logs under `base`.
pub fn build(base: &Path, versions: Versions) -> Diagnostics {
    atlas_host::bundle::gather(
        base,
        &atlas_core::crash::crash_dir(),
        versions,
        crate::cmds::atlas_bin().ok(),
        &runtime(),
        incidents(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library's version is readable from the process exactly when the library is loaded (what an
    /// AppImage, which bundles its own copies, needs: no package database names them). The app binary
    /// links both; this test binary may drop WebKitGTK as unused, and then must report none, not a guess.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_gtk_and_webkitgtk_versions_are_read_from_the_loaded_libraries() {
        let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
        let rt = runtime();
        for (lib, v) in [("libgtk-3", rt.gtk), ("libwebkit2gtk", rt.webkitgtk)] {
            if maps.contains(lib) {
                let v = v.unwrap_or_else(|| panic!("{lib} is loaded but its version was not read"));
                let parts: Vec<&str> = v.split('.').collect();
                assert!(
                    parts.len() == 3 && parts.iter().all(|p| p.parse::<u32>().is_ok()),
                    "{lib}: {v}"
                );
            } else {
                assert_eq!(v, None, "{lib} is not loaded");
            }
        }
        assert!(maps.contains("libgtk-3"), "GTK must be linked into the app");
    }

    #[test]
    fn the_app_log_rotates_instead_of_growing_forever() {
        let dir = tempfile::tempdir().unwrap();
        let p = app_log_path(dir.path());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, vec![b'a'; MAX_LOG_BYTES as usize + 1]).unwrap();
        log_in(dir.path(), "after rotation");
        assert!(p.with_extension("log.1").exists());
        assert!(
            std::fs::read_to_string(&p)
                .unwrap()
                .contains("after rotation")
        );
    }

    #[test]
    fn log_lines_are_redacted_and_single_line() {
        let dir = tempfile::tempdir().unwrap();
        log_in(dir.path(), "boom\nTOKEN=abc12345secret");
        let s = std::fs::read_to_string(app_log_path(dir.path())).unwrap();
        assert_eq!(s.lines().count(), 1, "{s}");
        assert!(!s.contains("abc12345secret"), "{s}");
    }

    #[test]
    fn incidents_are_bounded_redacted_and_ordered() {
        for i in 0..(MAX_INCIDENTS + 5) {
            incident("unreachable", &format!("n{i} TOKEN=hunter2hunter2"));
        }
        let all = incidents();
        assert!(all.len() <= MAX_INCIDENTS);
        assert!(all.iter().all(|i| !i.detail.contains("hunter2")));
        assert!(
            all.last()
                .unwrap()
                .detail
                .starts_with(&format!("n{}", MAX_INCIDENTS + 4))
        );
    }
}
