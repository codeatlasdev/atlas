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

use atlas_core::probe::GRAPHICS_ENV_PREFIXES as PREFIXES;

/// What Atlas did about `GTK_IM_MODULE` at startup (#247). The native terminal's IME preedit is drawn only when GTK's
/// text-input-v3 module (`wayland`) is the one in use: with the variable unset or `fcitx`, the committed text still
/// arrives but nothing shows while composing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImModule {
    /// Unset, a Wayland session and an input method running: Atlas set it to `wayland`.
    Set,
    /// The user's own value (including `fcitx`, `ibus`, `none`): never touched.
    Yours(String),
    /// Unset, but the app does not run on Wayland (X11, XWayland, `GDK_BACKEND=x11`): left alone.
    NotWayland,
    /// Unset on Wayland, but no input method answered: left alone.
    NoInputMethod,
}

impl ImModule {
    /// The `im=` field of the log line.
    fn describe(&self) -> String {
        match self {
            Self::Set => "wayland (set by Atlas: Wayland session with an input method)".into(),
            Self::Yours(v) => format!("{v} (yours)"),
            Self::NotWayland => "unset (not a Wayland session)".into(),
            Self::NoInputMethod => "unset (no input method running)".into(),
        }
    }
}

/// Decide `GTK_IM_MODULE`: `wayland` only when the variable is unset (or empty), the app will use GDK's Wayland backend
/// and an input method is running; a value the user set is always respected. `input_method_running` is the probe, asked
/// only when everything else says yes (it opens a Wayland connection), never on X11 or when the user chose.
pub fn im_module(
    var: impl Fn(&str) -> Option<String>,
    input_method_running: impl FnOnce() -> bool,
) -> ImModule {
    let set = |k: &str| var(k).filter(|v| !v.is_empty());
    if let Some(v) = set("GTK_IM_MODULE") {
        return ImModule::Yours(v);
    }
    if !session_is_wayland(&var) {
        return ImModule::NotWayland;
    }
    if input_method_running() {
        ImModule::Set
    } else {
        ImModule::NoInputMethod
    }
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

/// Input method daemons whose presence means someone is composing text on this session: fcitx5 and IBus both speak
/// `zwp_text_input_v3` to a Wayland compositor (or to GTK's own module) on the user's behalf.
const IM_DAEMONS: &[&str] = &["fcitx5", "ibus-daemon"];

/// Is one of [`IM_DAEMONS`] among these process names (`/proc/<pid>/comm`)?
pub fn has_im_daemon<'a>(comms: impl IntoIterator<Item = &'a str>) -> bool {
    comms.into_iter().any(|c| IM_DAEMONS.contains(&c.trim()))
}

/// The line for the log: session type, display servers, the rendering variables that are set, the NVIDIA driver and
/// what became of `GTK_IM_MODULE`.
/// `env` is the process environment; `nvidia_driver` is the contents of `/proc/driver/nvidia/version`, if present.
pub fn describe(
    env: &BTreeMap<String, String>,
    nvidia_driver: Option<&str>,
    im: &ImModule,
) -> String {
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
        "linux graphics: session={session} display={display} nvidia={nvidia} im={} env=[{}]",
        im.describe(),
        if set.is_empty() {
            "none set".to_string()
        } else {
            set.join(" ")
        }
    )
}

/// A process of this user is one of [`IM_DAEMONS`] (`/proc/<pid>/comm`).
#[cfg(target_os = "linux")]
fn im_daemon_running() -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return false;
    };
    // SAFETY: geteuid has no preconditions and cannot fail.
    let me = unsafe { libc::geteuid() };
    let comms: Vec<String> = procs
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter(|e| e.metadata().is_ok_and(|m| m.uid() == me))
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .collect();
    has_im_daemon(comms.iter().map(String::as_str))
}

/// The registry state of the probe's own short-lived connection.
#[cfg(target_os = "linux")]
struct Registry;

#[cfg(target_os = "linux")]
impl
    wayland_client::Dispatch<
        wayland_client::protocol::wl_registry::WlRegistry,
        wayland_client::globals::GlobalListContents,
    > for Registry
{
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_registry::WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &wayland_client::globals::GlobalListContents,
        _: &wayland_client::Connection,
        _: &wayland_client::QueueHandle<Self>,
    ) {
    }
}

/// Does the compositor advertise `zwp_text_input_manager_v3`? Asked on a connection of its own, before GTK exists
/// (GTK picks its input method module while it starts). Any failure, including a missing libwayland-client, is "no".
#[cfg(target_os = "linux")]
fn compositor_has_text_input_v3() -> bool {
    std::panic::catch_unwind(|| {
        let conn = wayland_client::Connection::connect_to_env().ok()?;
        let (globals, _queue) =
            wayland_client::globals::registry_queue_init::<Registry>(&conn).ok()?;
        Some(
            globals
                .contents()
                .with_list(|g| g.iter().any(|x| x.interface == "zwp_text_input_manager_v3")),
        )
    })
    .ok()
    .flatten()
    .unwrap_or(false)
}

/// Decide `GTK_IM_MODULE` from the real environment and set it, then say what was done. Must run on the main thread
/// before GTK initialises.
#[cfg(target_os = "linux")]
fn default_im_module() -> ImModule {
    let im = im_module(
        |k| std::env::var(k).ok(),
        || im_daemon_running() || compositor_has_text_input_v3(),
    );
    if im == ImModule::Set {
        // SAFETY: `main` is still on its one thread of consequence: GTK, WebKit and every worker that reads the
        // environment start after this, and the one thread running (the log trimmer) only sleeps and touches files.
        unsafe { std::env::set_var("GTK_IM_MODULE", "wayland") };
    }
    im
}

/// Write the line to the app log (Linux only: nothing else paints through WebKitGTK).
#[cfg(target_os = "linux")]
pub fn log_startup() {
    let im = default_im_module();
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let nvidia = std::fs::read_to_string("/proc/driver/nvidia/version").ok();
    crate::diag::log(&describe(&env, nvidia.as_deref(), &im));
}

/// Whether the desktop is a tiling window manager, from `XDG_CURRENT_DESKTOP` (a `:`-separated list,
/// case-insensitive) or the socket variable the WM exports. `var` looks up an environment variable. There the WM owns the
/// window (focus, size, close), so Atlas draws no caption buttons and no band for them (#269).
pub fn is_tiling(var: impl Fn(&str) -> Option<String>) -> bool {
    const TILING: &[&str] = &[
        "hyprland", "sway", "i3", "niri", "river", "qtile", "bspwm", "awesome", "dwm", "xmonad",
    ];
    // sway, i3, Hyprland and niri export their own socket to every client, also when started from a TTY (no display
    // manager, so no XDG_CURRENT_DESKTOP).
    const SOCKETS: &[&str] = &[
        "SWAYSOCK",
        "I3SOCK",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "NIRI_SOCKET",
    ];
    SOCKETS
        .iter()
        .any(|k| var(k).is_some_and(|v| !v.is_empty()))
        || var("XDG_CURRENT_DESKTOP").is_some_and(|d| {
            d.split(':')
                .any(|part| TILING.contains(&part.trim().to_ascii_lowercase().as_str()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fcitx5_and_ibus_are_input_method_daemons_a_shell_is_not() {
        assert!(has_im_daemon(["bash\n", "fcitx5\n"]));
        assert!(has_im_daemon(["ibus-daemon"]));
        assert!(!has_im_daemon([
            "bash",
            "fcitx5-remote",
            "ibus-ui-gtk3",
            "sway"
        ]));
        assert!(!has_im_daemon([]));
    }

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    /// #269: Hyprland (Omarchy) showed caption buttons the WM already owns; GNOME and KDE need them.
    #[test]
    fn tiling_desktops_are_recognized_and_stacking_ones_are_not() {
        for d in ["Hyprland", "sway", "i3", "niri", "XFCE:i3"] {
            assert!(is_tiling(lookup(&[("XDG_CURRENT_DESKTOP", d)])), "{d}");
        }
        for d in ["GNOME", "ubuntu:GNOME", "KDE", "X-Cinnamon", "XFCE", ""] {
            assert!(!is_tiling(lookup(&[("XDG_CURRENT_DESKTOP", d)])), "{d}");
        }
        assert!(!is_tiling(lookup(&[])));
    }

    /// sway/i3 started from a TTY (`exec sway`) or a bare `startx` have no display manager to set
    /// XDG_CURRENT_DESKTOP, but they export their own socket; so do Hyprland and niri.
    #[test]
    fn a_wm_started_without_a_display_manager_is_recognized_by_its_own_variables() {
        for v in [
            "SWAYSOCK",
            "I3SOCK",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "NIRI_SOCKET",
        ] {
            assert!(is_tiling(lookup(&[(v, "/run/user/1000/wm.sock")])), "{v}");
        }
        // An empty variable is "unset" for this purpose; a GNOME session that merely inherited nothing stays stacking.
        assert!(!is_tiling(lookup(&[("SWAYSOCK", "")])));
        assert!(!is_tiling(lookup(&[("XDG_CURRENT_DESKTOP", "GNOME")])));
    }

    #[test]
    fn the_session_is_wayland_only_when_the_app_will_use_it() {
        assert!(session_is_wayland(lookup(&[(
            "WAYLAND_DISPLAY",
            "wayland-1"
        )])));
        assert!(session_is_wayland(lookup(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("GDK_BACKEND", "wayland,x11")
        ])));
        // GDK_BACKEND=x11 forces XWayland on a Wayland session: the GTK window is not a Wayland surface.
        assert!(!session_is_wayland(lookup(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("GDK_BACKEND", "x11")
        ])));
        assert!(!session_is_wayland(lookup(&[("DISPLAY", ":0")])));
        assert!(!session_is_wayland(lookup(&[])));
    }

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
            &ImModule::Set,
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
            &ImModule::NotWayland,
        );
        assert_eq!(
            l,
            "linux graphics: session=x11 display=x11 nvidia=none im=unset (not a Wayland session) env=[none set]"
        );
    }

    #[test]
    fn an_empty_variable_counts_as_unset_and_a_missing_session_is_unknown() {
        let l = describe(
            &env(&[("WEBKIT_DISABLE_COMPOSITING_MODE", ""), ("DISPLAY", "")]),
            None,
            &ImModule::NoInputMethod,
        );
        assert_eq!(
            l,
            "linux graphics: session=unknown display=none nvidia=none im=unset (no input method running) env=[none set]"
        );
    }

    #[test]
    fn an_nvidia_driver_file_without_a_parsable_version_still_reports_the_driver() {
        let l = describe(&env(&[]), Some("garbage"), &ImModule::NotWayland);
        assert!(l.contains("nvidia=proprietary driver im="), "{l}");
    }

    /// The log line says what Atlas did about the input method, so a report of "no preedit" is answerable from it.
    #[test]
    fn the_log_line_says_what_became_of_gtk_im_module() {
        let say = |im| describe(&env(&[("WAYLAND_DISPLAY", "wayland-1")]), None, &im);
        assert!(
            say(ImModule::Set).contains("im=wayland (set by Atlas"),
            "{}",
            say(ImModule::Set)
        );
        let yours = say(ImModule::Yours("fcitx".into()));
        assert!(yours.contains("im=fcitx (yours)"), "{yours}");
        let none = say(ImModule::NoInputMethod);
        assert!(
            none.contains("im=unset (no input method running)"),
            "{none}"
        );
    }

    use std::cell::Cell;

    /// Run the decision; the second value is how many times the (expensive) probe was asked.
    fn im(pairs: &[(&str, &str)], running: bool) -> (ImModule, u32) {
        let asked = Cell::new(0);
        let d = im_module(lookup(pairs), || {
            asked.set(asked.get() + 1);
            running
        });
        (d, asked.get())
    }

    /// #247: the native terminal's preedit needs GTK's text-input-v3 module; Atlas picks it where it can work.
    #[test]
    fn an_unset_module_on_wayland_with_an_input_method_becomes_wayland() {
        assert_eq!(
            im(&[("WAYLAND_DISPLAY", "wayland-1")], true),
            (ImModule::Set, 1)
        );
        // The app is Wayland also when GDK_BACKEND lists it first, and an empty variable counts as unset.
        assert_eq!(
            im(
                &[
                    ("WAYLAND_DISPLAY", "wayland-1"),
                    ("GDK_BACKEND", "wayland,x11"),
                    ("GTK_IM_MODULE", ""),
                ],
                true
            )
            .0,
            ImModule::Set
        );
    }

    /// A user who exports `fcitx`, `ibus`, `wayland`, `none` or anything else keeps it, and nothing is probed.
    #[test]
    fn a_value_the_user_set_is_never_touched() {
        for v in ["fcitx", "ibus", "wayland", "none", "xim", "simple"] {
            assert_eq!(
                im(
                    &[("WAYLAND_DISPLAY", "wayland-1"), ("GTK_IM_MODULE", v)],
                    true
                ),
                (ImModule::Yours(v.into()), 0),
                "{v}"
            );
        }
    }

    /// X11, XWayland and `GDK_BACKEND=x11` on a Wayland session: GDK is not on Wayland, text-input-v3 is no use.
    #[test]
    fn an_x11_app_is_left_alone() {
        for pairs in [
            &[("DISPLAY", ":0")][..],
            &[("WAYLAND_DISPLAY", "wayland-1"), ("GDK_BACKEND", "x11")],
            &[],
        ] {
            assert_eq!(im(pairs, true), (ImModule::NotWayland, 0), "{pairs:?}");
        }
    }

    #[test]
    fn without_an_input_method_nothing_is_set() {
        assert_eq!(
            im(&[("WAYLAND_DISPLAY", "wayland-1")], false),
            (ImModule::NoInputMethod, 1)
        );
    }
}
