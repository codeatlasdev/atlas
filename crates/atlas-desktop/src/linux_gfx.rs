//! What decides how WebKitGTK paints Atlas on Linux, written to the app log once at startup.
//!
//! Atlas sets none of the WebKitGTK renderer variables itself: Tauri's own guidance
//! (<https://tauri.app/develop/debug/linux-graphics>) is that forcing `WEBKIT_DISABLE_DMABUF_RENDERER` or
//! `WEBKIT_DISABLE_COMPOSITING_MODE` for everyone "disables a faster path for everyone, including users on working
//! setups", and a measured report on Arch + NVIDIA found both set together made scrolling 44-68 ms per frame against
//! 17 ms with only the shared-memory transport (<https://github.com/xnmp/tauri-explorer/issues/1057>). Which one a
//! machine needs depends on the GPU, the driver and X11 vs Wayland, so a bug report has to say what it ran on. The line
//! lands in `app.log`, which "Copy diagnostics" already includes.
//!
//! Pure (the environment is passed in), so it is testable on any OS.

use std::collections::BTreeMap;

/// Variables that change WebKitGTK's or GTK's rendering path; shown verbatim when set.
const PREFIXES: &[&str] = &[
    "WEBKIT_", "GSK_", "GDK_", "__NV_", "__GLX_", "LIBGL_", "MESA_", "LIBVA_",
];

/// The line for the log: session type, display servers, the rendering variables that are set, the NVIDIA driver.
/// `env` is the process environment; `nvidia_driver` is the contents of `/proc/driver/nvidia/version`, if present.
pub fn describe(env: &BTreeMap<String, String>, nvidia_driver: Option<&str>) -> String {
    let get = |k: &str| env.get(k).map(String::as_str).filter(|v| !v.is_empty());
    let session = get("XDG_SESSION_TYPE").unwrap_or("unknown");
    let display = match (get("WAYLAND_DISPLAY"), get("DISPLAY")) {
        (Some(_), Some(_)) => "wayland+xwayland",
        (Some(_), None) => "wayland",
        (None, Some(_)) => "x11",
        (None, None) => "none",
    };
    let set: Vec<String> = env
        .iter()
        .filter(|(k, v)| !v.is_empty() && PREFIXES.iter().any(|p| k.starts_with(p)))
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let nvidia = match nvidia_driver {
        // "NVRM version: NVIDIA UNIX x86_64 Kernel Module  570.133.07  Thu ..." -> the first dotted number.
        Some(v) => v
            .split_whitespace()
            .find(|w| w.contains('.') && w.chars().all(|c| c.is_ascii_digit() || c == '.'))
            .map_or_else(
                || "proprietary driver".to_string(),
                |d| format!("proprietary driver {d}"),
            ),
        None => "none".to_string(),
    };
    format!(
        "linux graphics: session={session} display={display} nvidia={nvidia} env=[{}]",
        if set.is_empty() {
            "none set".to_string()
        } else {
            set.join(" ")
        }
    )
}

/// Write the line to the app log (Linux only: nothing else paints through WebKitGTK).
#[cfg(target_os = "linux")]
pub fn log_startup() {
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let nvidia = std::fs::read_to_string("/proc/driver/nvidia/version").ok();
    crate::diag::log(&describe(&env, nvidia.as_deref()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // Incident (2026-10-26, Arch users: lag, freezes, ghost frames): a report said "Linux" and nothing about the
    // compositor, the GPU or the WebKit variables that decide the rendering path.
    #[test]
    fn an_arch_wayland_nvidia_session_says_so_and_lists_the_webkit_variables() {
        let l = describe(
            &env(&[
                ("XDG_SESSION_TYPE", "wayland"),
                ("WAYLAND_DISPLAY", "wayland-1"),
                ("DISPLAY", ":0"),
                ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
                ("GDK_BACKEND", "x11"),
                ("HOME", "/home/ana"),
                ("PATH", "/usr/bin"),
            ]),
            Some("NVRM version: NVIDIA UNIX x86_64 Kernel Module  570.133.07  Thu Mar 20 2025\n"),
        );
        assert!(l.contains("session=wayland"), "{l}");
        assert!(l.contains("display=wayland+xwayland"), "{l}");
        assert!(l.contains("nvidia=proprietary driver 570.133.07"), "{l}");
        assert!(l.contains("GDK_BACKEND=x11"), "{l}");
        assert!(l.contains("WEBKIT_DISABLE_DMABUF_RENDERER=1"), "{l}");
        assert!(
            !l.contains("HOME") && !l.contains("PATH"),
            "unrelated variables stay out: {l}"
        );
    }

    #[test]
    fn a_plain_x11_session_with_no_overrides_and_no_nvidia() {
        let l = describe(
            &env(&[("XDG_SESSION_TYPE", "x11"), ("DISPLAY", ":0")]),
            None,
        );
        assert_eq!(
            l,
            "linux graphics: session=x11 display=x11 nvidia=none env=[none set]"
        );
    }

    #[test]
    fn an_empty_variable_counts_as_unset_and_a_missing_session_is_unknown() {
        let l = describe(
            &env(&[("WEBKIT_DISABLE_COMPOSITING_MODE", ""), ("DISPLAY", "")]),
            None,
        );
        assert_eq!(
            l,
            "linux graphics: session=unknown display=none nvidia=none env=[none set]"
        );
    }

    #[test]
    fn an_nvidia_driver_file_without_a_parsable_version_still_reports_the_driver() {
        let l = describe(&env(&[]), Some("garbage"));
        assert!(l.contains("nvidia=proprietary driver env="), "{l}");
    }
}
