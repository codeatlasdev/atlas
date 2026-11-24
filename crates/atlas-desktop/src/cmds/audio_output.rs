//! Whether WebKit can play audio on this machine (Linux).
//!
//! WebKitGTK plays through GStreamer's `autoaudiosink` (the `autodetect` plugin, in gst-plugins-good). Without it
//! the web process prints `GStreamer element autoaudiosink not found` and, for every `AudioContext.resume()`,
//! two `GLib-GObject-CRITICAL` lines. The packages depend on gst-plugins-good and the AppImage bundles it, so this only
//! happens on a hand-built or `--nodeps` install; the app then stays silent instead of starting an audio device that
//! cannot exist.
//!
//! A server that never routes a stream (PipeWire without a session manager, #755) is the other way Web Audio breaks: the
//! web process then blocks its main thread inside GStreamer for good, so a stream is opened first with a deadline.

use std::path::{Path, PathBuf};
use std::time::Duration;

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

/// How long the playback probe may take before the machine counts as having no usable output. A healthy server answers in
/// milliseconds (20 ms measured against PipeWire with WirePlumber); a stuck one answers after 30 s.
const ROUTE_DEADLINE: Duration = Duration::from_secs(3);

/// Runs `f` on its own thread and reports whether it came back within `deadline` (a panic is a return, not a hang). A
/// thread that overruns is left to finish on its own: nothing can interrupt a blocked FFI call.
fn completes_within(deadline: Duration, f: impl FnOnce() + Send + 'static) -> bool {
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let _ = done.send(());
    });
    !matches!(
        wait.recv_timeout(deadline),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    )
}

/// Opens and closes one playback stream through libpulse-simple, the call WebKit's `pulsesink` makes first. PipeWire
/// with no session manager (no WirePlumber) never routes a stream, and then this call, like the sink, blocks for 30 s
/// (#755). Returns at once when libpulse is not installed or no server answers: those are not hangs and WebKit falls back
/// to another sink.
#[cfg(target_os = "linux")]
fn open_playback_stream() {
    use std::ffi::{c_char, c_int, c_void};
    /// `pa_sample_spec`.
    #[repr(C)]
    struct SampleSpec {
        format: c_int,
        rate: u32,
        channels: u8,
    }
    type New = unsafe extern "C" fn(
        server: *const c_char,
        name: *const c_char,
        dir: c_int,
        dev: *const c_char,
        stream_name: *const c_char,
        spec: *const SampleSpec,
        map: *const c_void,
        attr: *const c_void,
        error: *mut c_int,
    ) -> *mut c_void;
    type Free = unsafe extern "C" fn(*mut c_void);
    const PA_STREAM_PLAYBACK: c_int = 1;
    const PA_SAMPLE_S16LE: c_int = 3;
    // SAFETY: the symbols are looked up by name and cast to the signatures libpulse-simple documents
    // (`pa_simple_new`, `pa_simple_free`); the handle is never closed, so they stay valid.
    unsafe {
        let lib = libc::dlopen(
            c"libpulse-simple.so.0".as_ptr(),
            libc::RTLD_NOW | libc::RTLD_LOCAL,
        );
        if lib.is_null() {
            return;
        }
        let new = libc::dlsym(lib, c"pa_simple_new".as_ptr());
        let free = libc::dlsym(lib, c"pa_simple_free".as_ptr());
        if new.is_null() || free.is_null() {
            return;
        }
        let (new, free) = (
            std::mem::transmute::<*mut c_void, New>(new),
            std::mem::transmute::<*mut c_void, Free>(free),
        );
        let spec = SampleSpec {
            format: PA_SAMPLE_S16LE,
            rate: 44_100,
            channels: 2,
        };
        let mut error: c_int = 0;
        let stream = new(
            std::ptr::null(),
            c"Atlas".as_ptr(),
            PA_STREAM_PLAYBACK,
            std::ptr::null(),
            c"output probe".as_ptr(),
            &spec,
            std::ptr::null(),
            std::ptr::null(),
            &mut error,
        );
        if !stream.is_null() {
            free(stream);
        }
    }
}

/// Only Linux has a PulseAudio/PipeWire route to wait for.
#[cfg(not(target_os = "linux"))]
fn open_playback_stream() {}

/// The sink exists (plugin check) and the route to it answers (`route_answers`, only asked when the sink exists).
fn output_available(dirs: &[PathBuf], route_answers: impl FnOnce() -> bool) -> bool {
    sink_available(dirs) && route_answers()
}

/// Whether the app may start Web Audio. Read once at start by the frontend. Only Linux can lack the sink, or have a
/// server that never routes a stream: Web Audio then blocks the page's main thread for good, so the app stays silent.
#[tauri::command]
pub async fn audio_output_available() -> bool {
    if !cfg!(target_os = "linux") {
        return true;
    }
    tauri::async_runtime::spawn_blocking(|| {
        output_available(&plugin_dirs(|k| std::env::var(k).ok()), || {
            completes_within(ROUTE_DEADLINE, open_playback_stream)
        })
    })
    .await
    .unwrap_or(true)
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

    // Incident #755: PipeWire without a session manager gives every playback stream no route; `pa_simple_new` (and
    // WebKit's pulsesink with it) then blocks for 30 s and again, and the page's main thread, inside Web Audio, never
    // answers. The probe must tell "returns" from "hangs" without waiting for the hang.
    #[test]
    fn a_probe_that_hangs_is_reported_within_the_deadline_not_waited_for() {
        let (release, hold) = std::sync::mpsc::channel::<()>();
        let t = std::time::Instant::now();
        assert!(!completes_within(Duration::from_millis(150), move || {
            let _ = hold.recv();
        }));
        assert!(t.elapsed() < Duration::from_secs(2), "waited for the hang");
        drop(release);
    }

    #[test]
    fn a_probe_that_returns_counts_even_when_it_failed_fast() {
        assert!(completes_within(Duration::from_secs(5), || {}));
        // A fast failure (no server at all) is not a hang: WebKit falls back to another sink, so the app does not silence itself.
        assert!(completes_within(Duration::from_secs(5), || panic!(
            "no server"
        )));
    }

    #[test]
    fn a_hanging_route_silences_the_app_and_a_missing_sink_never_reaches_the_probe() {
        let good = dir_with(&[CORE, AUTODETECT]);
        let bad = dir_with(&[CORE]);
        assert!(output_available(&[good.path().to_path_buf()], || true));
        assert!(!output_available(&[good.path().to_path_buf()], || false));
        assert!(!output_available(&[bad.path().to_path_buf()], || {
            panic!("probed a machine that already has no sink")
        }));
    }
}
