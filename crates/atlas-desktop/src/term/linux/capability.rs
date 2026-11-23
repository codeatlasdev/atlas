//! Can this session draw the native terminal, and did the user ask for it?
//!
//! The native terminal on Linux is a `wl_subsurface` of GDK's own Wayland window drawn by wgpu on Vulkan. Everything
//! else keeps xterm.js, the page's terminal: an X11 session, XWayland (`GDK_BACKEND=x11`), a compositor without
//! subsurfaces or viewporter, no Vulkan, NVIDIA's proprietary driver, a software rasterizer, and any failure while
//! making the surface. The decision is pure ([`decide`]) so each of those is a test; [`probe`] gathers the inputs.
//!
//! The setting is `gpuTerminal` in `~/.atlas/desktop/settings.json` (or `ATLAS_GPU_TERMINAL=1|0`): OFF by default in this
//! phase. It lives in Rust's reach, not the page's localStorage, because the main window must be created transparent
//! (the terminal sits BELOW the page and shows through a hole in it) and that is decided before any page exists.

use serde::Serialize;

/// What the machine offers, as seen from this process.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// GDK's default display is a Wayland one (not X11, not XWayland).
    pub gdk_wayland: bool,
    /// Why GDK's Wayland connection could not be wrapped (libwayland-client missing, no registry), when it could not.
    pub connect_error: Option<String>,
    /// Required globals the compositor does not advertise (`wl_subcompositor`, `wp_viewporter`, ...).
    pub missing_globals: Vec<String>,
    /// The GPU probe: the adapter, or why not (`atlas_term::probe`).
    pub gpu: Option<Result<String, String>>,
}

/// `Ok(adapter)`: the native terminal can run here. `Err(reason)`: it cannot, and why (the log and Settings say it).
pub fn decide(env: &Env) -> Result<String, String> {
    if !env.gdk_wayland {
        return Err("not a Wayland session (X11 and XWayland keep xterm.js)".into());
    }
    if let Some(e) = &env.connect_error {
        return Err(format!("no Wayland connection: {e}"));
    }
    if !env.missing_globals.is_empty() {
        return Err(format!(
            "the compositor lacks {}",
            env.missing_globals.join(", ")
        ));
    }
    match &env.gpu {
        Some(Ok(adapter)) => Ok(adapter.clone()),
        Some(Err(e)) => Err(format!("no usable GPU: {e}")),
        None => Err("the GPU was not probed".into()),
    }
}

/// Whether the user turned the native terminal on: `env` is `ATLAS_GPU_TERMINAL`, `setting` the settings.json value.
/// The environment wins (a test or an agent instance sets it without touching a settings file); off when neither says.
pub fn wanted(env: Option<&str>, setting: Option<&serde_json::Value>) -> bool {
    match env.map(str::trim) {
        Some("1" | "true" | "on") => return true,
        Some("0" | "false" | "off") => return false,
        _ => {}
    }
    setting
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// The session says Wayland (cheap, no GPU or display needed): what the main window's transparency decides on, before
/// GTK is up. A Wayland session whose app is forced to X11 (`GDK_BACKEND=x11`) is not one.
pub fn session_is_wayland(var: impl Fn(&str) -> Option<String>) -> bool {
    let set = |k: &str| var(k).is_some_and(|v| !v.is_empty());
    let backend = var("GDK_BACKEND").unwrap_or_default();
    let backend_ok = backend.is_empty()
        || backend
            .split(',')
            .next()
            .is_some_and(|b| b.trim() == "wayland");
    set("WAYLAND_DISPLAY") && backend_ok
}

/// The user's choice, read from the environment and settings.json.
pub fn setting_on() -> bool {
    let env = std::env::var("ATLAS_GPU_TERMINAL").ok();
    let s = crate::shell_env::load_settings();
    wanted(env.as_deref(), s.extra.get("gpuTerminal"))
}

/// Should the main window be created see-through? Only where the native terminal may run and was asked for.
pub fn window_wanted() -> bool {
    setting_on() && session_is_wayland(|k| std::env::var(k).ok())
}

/// The `term_capability` reply.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    /// The machine can draw the native terminal (the Settings toggle is offered).
    pub available: bool,
    /// Why not, when it cannot.
    pub reason: Option<String>,
    /// The adapter, when it can.
    pub adapter: Option<String>,
    /// The setting as saved now.
    pub setting: bool,
    /// The native terminal is in use in this run: available, asked for, and the window was made see-through at launch.
    pub active: bool,
}

/// Build the reply from a decision, the saved setting and whether the window was created see-through.
pub fn reply(decision: Result<String, String>, setting: bool, window_clear: bool) -> Capability {
    let (available, reason, adapter) = match decision {
        Ok(a) => (true, None, Some(a)),
        Err(e) => (false, Some(e), None),
    };
    Capability {
        available,
        reason,
        adapter,
        setting,
        active: available && setting && window_clear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn good() -> Env {
        Env {
            gdk_wayland: true,
            connect_error: None,
            missing_globals: vec![],
            gpu: Some(Ok("AMD Radeon 680M (radv, Vulkan)".into())),
        }
    }

    #[test]
    fn a_wayland_session_with_vulkan_is_available_and_names_the_adapter() {
        assert_eq!(decide(&good()).unwrap(), "AMD Radeon 680M (radv, Vulkan)");
    }

    // Each way the design lists for staying on xterm.js, with the reason Settings shows.
    #[test]
    fn x11_and_xwayland_keep_xterm() {
        let e = Env {
            gdk_wayland: false,
            ..good()
        };
        assert!(decide(&e).unwrap_err().contains("X11"));
    }

    #[test]
    fn a_compositor_without_subsurface_or_viewporter_keeps_xterm() {
        let e = Env {
            missing_globals: vec!["wp_viewporter".into()],
            ..good()
        };
        assert!(decide(&e).unwrap_err().contains("wp_viewporter"));
    }

    #[test]
    fn no_vulkan_or_a_refused_driver_keeps_xterm_with_the_gpu_reason() {
        for why in [
            "no adapter",
            "the proprietary NVIDIA driver is not supported",
        ] {
            let e = Env {
                gpu: Some(Err(why.into())),
                ..good()
            };
            let r = decide(&e).unwrap_err();
            assert!(r.contains(why), "{r}");
        }
        let e = Env {
            gpu: None,
            ..good()
        };
        assert!(decide(&e).is_err(), "an unprobed GPU is not available");
    }

    #[test]
    fn a_wayland_connection_that_cannot_be_made_keeps_xterm_and_says_why() {
        let e = Env {
            connect_error: Some("libwayland-client.so.0: cannot open shared object file".into()),
            ..good()
        };
        assert!(decide(&e).unwrap_err().contains("libwayland-client.so.0"));
    }

    #[test]
    fn the_terminal_is_off_by_default_and_the_environment_wins_over_the_file() {
        assert!(!wanted(None, None), "off by default in this phase");
        assert!(wanted(None, Some(&serde_json::json!(true))));
        assert!(
            !wanted(None, Some(&serde_json::json!("yes"))),
            "only a bool"
        );
        assert!(wanted(Some("1"), Some(&serde_json::json!(false))));
        assert!(!wanted(Some("0"), Some(&serde_json::json!(true))));
        assert!(wanted(Some(" on "), None));
        assert!(!wanted(Some(""), None));
    }

    #[test]
    fn the_session_is_wayland_only_when_the_app_will_use_it() {
        let env = |pairs: &'static [(&str, &str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| (*v).to_string())
            }
        };
        assert!(session_is_wayland(env(&[("WAYLAND_DISPLAY", "wayland-1")])));
        assert!(session_is_wayland(env(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("GDK_BACKEND", "wayland,x11")
        ])));
        // GDK_BACKEND=x11 forces XWayland on a Wayland session: the GTK window is not a Wayland surface.
        assert!(!session_is_wayland(env(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("GDK_BACKEND", "x11")
        ])));
        assert!(!session_is_wayland(env(&[("DISPLAY", ":0")])));
        assert!(!session_is_wayland(env(&[])));
    }

    #[test]
    fn the_terminal_is_active_only_when_available_asked_for_and_the_window_is_see_through() {
        let on = |setting, clear| reply(Ok("gpu".into()), setting, clear).active;
        assert!(on(true, true));
        assert!(!on(false, true), "not asked for");
        assert!(
            !on(true, false),
            "setting turned on this run: restart to make the window see-through"
        );
        assert!(!reply(Err("x".into()), true, true).active);
    }

    #[test]
    fn the_reply_serializes_camel_case_for_the_page() {
        let v = serde_json::to_value(reply(Err("no gpu".into()), true, false)).unwrap();
        assert_eq!(v["available"], false);
        assert_eq!(v["reason"], "no gpu");
        assert_eq!(v["setting"], true);
        assert_eq!(v["active"], false);
        assert!(v["adapter"].is_null());
    }
}
