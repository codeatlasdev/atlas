//! Whether WebKit can play audio on this machine (Linux).
//!
//! WebKitGTK plays through GStreamer's `autoaudiosink` (the `autodetect` plugin, in gst-plugins-good). Without it
//! the web process prints `GStreamer element autoaudiosink not found` and, for every `AudioContext.resume()`,
//! two `GLib-GObject-CRITICAL` lines. The packages depend on gst-plugins-good and the AppImage bundles it, so this only
//! happens on a hand-built or `--nodeps` install; the app then stays silent instead of starting an audio device that
//! cannot exist.

use std::path::{Path, PathBuf};

/// The plugin every GStreamer install has (`fakesink`, `queue`, ...): its presence proves a directory is a real plugin
/// directory, so a machine where we simply did not find GStreamer is not mistaken for one without a sink.
const CORE: &str = "libgstcoreelements.so";
/// `autoaudiosink` lives here.
const AUTODETECT: &str = "libgstautodetect.so";

/// Whether the plugin directories `dirs` leave audio playable. `true` when no directory is a GStreamer plugin
/// directory (unknown: do not silence the app on a guess), `false` when some are but none holds the sink.
fn sink_available(dirs: &[PathBuf]) -> bool {
    let real: Vec<&PathBuf> = dirs.iter().filter(|d| d.join(CORE).is_file()).collect();
    real.is_empty() || real.iter().any(|d| d.join(AUTODETECT).is_file())
}

/// Where this process's GStreamer looks for plugins: the variables it honors (the AppImage's hook sets them to its own
/// plugins), else the distro directories.
fn plugin_dirs(env: impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["GST_PLUGIN_PATH_1_0", "GST_PLUGIN_PATH"] {
        dirs.extend(split(env(var)));
    }
    let system = split(env("GST_PLUGIN_SYSTEM_PATH_1_0").or_else(|| env("GST_PLUGIN_SYSTEM_PATH")));
    if system.is_empty() {
        dirs.extend(
            [
                "/usr/lib/gstreamer-1.0",
                "/usr/lib64/gstreamer-1.0",
                "/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
                "/usr/lib/aarch64-linux-gnu/gstreamer-1.0",
                "/usr/lib/arm-linux-gnueabihf/gstreamer-1.0",
            ]
            .map(PathBuf::from),
        );
    } else {
        dirs.extend(system);
    }
    dirs
}

fn split(v: Option<String>) -> Vec<PathBuf> {
    v.map(|v| {
        v.split(':')
            .filter(|p| !p.is_empty())
            .map(Path::new)
            .map(Path::to_path_buf)
            .collect()
    })
    .unwrap_or_default()
}

/// Whether the app may start Web Audio. Read once at start by the frontend. Only Linux can lack the sink.
#[tauri::command]
pub fn audio_output_available() -> bool {
    !cfg!(target_os = "linux") || sink_available(&plugin_dirs(|k| std::env::var(k).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plugin directory holding `files`.
    fn dir_with(files: &[&str]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for f in files {
            std::fs::write(d.path().join(f), "").unwrap();
        }
        d
    }

    // Incident (LinuxFixes item 3): an Arch machine without gst-plugins-good: every first click logged two
    // GLib-GObject-CRITICAL from the web process (`autoaudiosink not found`).
    #[test]
    fn a_plugin_directory_without_the_autodetect_plugin_has_no_audio() {
        let d = dir_with(&[CORE, "libgstapp.so"]);
        assert!(!sink_available(&[d.path().to_path_buf()]));
    }

    #[test]
    fn the_autodetect_plugin_in_any_real_directory_means_audio() {
        let base = dir_with(&[CORE]);
        let good = dir_with(&[CORE, AUTODETECT]);
        assert!(sink_available(&[
            base.path().to_path_buf(),
            good.path().to_path_buf()
        ]));
    }

    #[test]
    fn no_gstreamer_found_is_unknown_not_silence() {
        assert!(sink_available(&[]));
        let empty = dir_with(&[]);
        assert!(sink_available(&[
            empty.path().to_path_buf(),
            "/nonexistent".into()
        ]));
        // A stray autodetect file in a directory that is not a plugin directory changes nothing either.
        let stray = dir_with(&[AUTODETECT]);
        assert!(sink_available(&[stray.path().to_path_buf()]));
    }

    #[test]
    fn the_variables_gstreamer_honors_decide_where_to_look() {
        let env = |k: &str| match k {
            "GST_PLUGIN_SYSTEM_PATH_1_0" => Some("/opt/app/gst:/opt/more".to_string()),
            "GST_PLUGIN_PATH" => Some("/extra".to_string()),
            _ => None,
        };
        let dirs = plugin_dirs(env);
        assert_eq!(
            dirs,
            ["/extra", "/opt/app/gst", "/opt/more"].map(PathBuf::from)
        );
        // Unset: the distro directories.
        assert!(plugin_dirs(|_| None).contains(&PathBuf::from("/usr/lib/gstreamer-1.0")));
        assert!(
            plugin_dirs(|_| None)
                .contains(&PathBuf::from("/usr/lib/x86_64-linux-gnu/gstreamer-1.0"))
        );
    }
}
