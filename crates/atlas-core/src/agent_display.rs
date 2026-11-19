//! The private display of a Linux agent instance of Atlas Desktop (`atlas desktop --agent`, `crates/atlas-cli`).
//!
//! An instance runs on its own headless Wayland compositor and its own D-Bus session bus, both under
//! `<instance dir>/run`, so its window can never reach the user's screen and its single-instance D-Bus name can never
//! collide with the user's Atlas. The CLI builds the display half of the instance's environment with [`linux_env`];
//! the app calls [`verify_linux`] before it touches a display and refuses (exit 3) when the environment points at
//! anything else, so a wrong variable fails closed instead of putting a window on the user's session.

use std::path::{Path, PathBuf};

/// `<instance dir>/run`: the instance's `XDG_RUNTIME_DIR` (Wayland socket, sway IPC socket, session bus).
pub fn run_dir(instance: &Path) -> PathBuf {
    instance.join("run")
}

/// The private session bus of the instance.
pub fn bus_address(instance: &Path) -> String {
    format!("unix:path={}", run_dir(instance).join("bus").display())
}

/// The display and bus variables the app of an instance gets, and the only ones: everything else of the caller's
/// session (`DISPLAY`, `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`, `HYPRLAND_*`, …) is cleared.
pub fn linux_env(
    instance: &Path,
    wayland_display: &str,
    swaysock: &Path,
) -> Vec<(&'static str, String)> {
    vec![
        (
            "XDG_RUNTIME_DIR",
            run_dir(instance).to_string_lossy().into_owned(),
        ),
        ("WAYLAND_DISPLAY", wayland_display.to_string()),
        ("DBUS_SESSION_BUS_ADDRESS", bus_address(instance)),
        ("SWAYSOCK", swaysock.to_string_lossy().into_owned()),
        ("XDG_SESSION_TYPE", "wayland".into()),
        // GTK on Wayland only: with no `DISPLAY` it cannot fall back to an X server of the user's.
        ("GDK_BACKEND", "wayland".into()),
        // No accessibility bus: at-spi's launcher would bind sockets under the runtime dir and spawn helpers.
        ("NO_AT_BRIDGE", "1".into()),
        ("GTK_A11Y", "none".into()),
    ]
}

/// Whether the environment (`get` = a variable lookup) points at the private display of the instance named by
/// `ATLAS_DEBUG_SOCK` (`<instance dir>/debug.sock`) and at nothing else. `Err` says which variable is wrong.
pub fn verify_linux(get: impl Fn(&str) -> Option<String>) -> Result<(), String> {
    let var = |k: &str| get(k).filter(|v| !v.is_empty());
    let sock = var("ATLAS_DEBUG_SOCK").ok_or("ATLAS_DEBUG_SOCK is not set")?;
    let instance = Path::new(&sock)
        .parent()
        .ok_or("ATLAS_DEBUG_SOCK has no directory")?;
    let run = run_dir(instance);
    let want = |k: &str, expected: String| match var(k) {
        Some(v) if v == expected => Ok(()),
        other => Err(format!("{k} is {other:?}, not the instance's {expected:?}")),
    };
    want("XDG_RUNTIME_DIR", run.to_string_lossy().into_owned())?;
    want("DBUS_SESSION_BUS_ADDRESS", bus_address(instance))?;
    want("GDK_BACKEND", "wayland".into())?;
    want("NO_AT_BRIDGE", "1".into())?;
    want("GTK_A11Y", "none".into())?;
    if let Some(d) = var("DISPLAY") {
        return Err(format!("DISPLAY={d:?} would let GTK reach an X server"));
    }
    let wl = var("WAYLAND_DISPLAY").ok_or("WAYLAND_DISPLAY is not set")?;
    if wl.contains('/') || wl.contains("..") || !run.join(&wl).exists() {
        return Err(format!(
            "WAYLAND_DISPLAY={wl:?} is not a socket inside {}",
            run.display()
        ));
    }
    if let Some(s) = var("SWAYSOCK")
        && !Path::new(&s).starts_with(&run)
    {
        return Err(format!("SWAYSOCK={s:?} is outside {}", run.display()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// macOS: the virtual displays of agent instances that outlived their process (incident 2026-11-19).
//
// WindowServer removes a virtual display when the process that created it closes it or dies (verified in its log:
// `CGXVirtualDisplayDestroy` follows every instance's end, and a destroy is logged even for the instances that
// failed closed), but it POSTPONES the removal while the machine is in a non-wake state (built-in display asleep,
// screen locked): "Deferred hotplugs cannot be processed in a non-wake state". An instance that started then and
// ended left its display listed, active and 2560 points wide, to the right of the user's monitors, with no process
// to blame. Five of them stacked, because each new start placed itself right of the previous ones.
//
// So: no display is created while no real one is awake, none is created on top of orphans, and `--gc` names them.

/// Vendor id every Atlas agent display is created with: how an agent display is told from the user's monitors.
pub const AGENT_VENDOR: u32 = 0x4147;

/// One display the OS lists, as the start guard and `--gc` see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeenDisplay {
    /// `CGDirectDisplayID`.
    pub id: u32,
    /// `CGDisplayVendorNumber`.
    pub vendor: u32,
    /// `CGDisplaySerialNumber`: for an agent display, the pid of the instance that created it.
    pub serial: u32,
    /// In the active list (an asleep display is online but not active).
    pub active: bool,
    /// Global top-left origin, in points.
    pub origin: (f64, f64),
    /// Size in points.
    pub size: (f64, f64),
}

/// The agent displays whose creator no longer runs (`alive` says whether a pid does). Their owner is gone, so nobody
/// can destroy them from here.
pub fn orphans(seen: &[SeenDisplay], alive: impl Fn(u32) -> bool) -> Vec<SeenDisplay> {
    seen.iter()
        .filter(|d| d.vendor == AGENT_VENDOR && !alive(d.serial))
        .copied()
        .collect()
}

/// Whether one of the user's own displays (anything but an agent display) is awake.
pub fn real_awake(seen: &[SeenDisplay]) -> bool {
    seen.iter().any(|d| d.vendor != AGENT_VENDOR && d.active)
}

/// Why no start may create a display while no real one is awake.
pub const NO_REAL_DISPLAY: &str = "no real display is awake (the Mac is locked or its screen is asleep): macOS postpones removing a virtual display until the system wakes, so one created now would outlive this process. Unlock the Mac and try again";

/// What `--gc` and the start guard say about orphan displays.
pub fn orphan_report(orphans: &[SeenDisplay]) -> String {
    let list: Vec<String> = orphans
        .iter()
        .map(|d| {
            format!(
                "display {} at x={} ({}x{} points, created by pid {}, gone){}",
                d.id,
                d.origin.0,
                d.size.0,
                d.size.1,
                d.serial,
                if d.active { "" } else { ", not active" }
            )
        })
        .collect();
    format!(
        "{} orphan agent display(s) (vendor {AGENT_VENDOR:#x}) of instances that no longer run, extending the desktop \
         to the right: {}. Another process cannot remove a virtual display (`atlas desktop --agent --gc` lists them). \
         macOS drops them when it next processes \
         the removals it deferred (unlock or wake the Mac); if they are still there after that, log out or reboot",
        orphans.len(),
        list.join("; ")
    )
}

/// The start guard, pure: a real display must be awake and no orphan may be listed.
pub fn start_gate(real_awake: bool, orphans: &[SeenDisplay]) -> Result<(), String> {
    if !real_awake {
        return Err(NO_REAL_DISPLAY.into());
    }
    if !orphans.is_empty() {
        return Err(orphan_report(orphans));
    }
    Ok(())
}

/// Every display the OS lists, active or not.
#[cfg(target_os = "macos")]
pub fn seen_displays() -> Vec<SeenDisplay> {
    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGGetOnlineDisplayList(max: u32, out: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayBounds(id: u32) -> CGRect;
        fn CGDisplayVendorNumber(id: u32) -> u32;
        fn CGDisplaySerialNumber(id: u32) -> u32;
        fn CGDisplayIsActive(id: u32) -> u32;
    }
    let mut n = 0u32;
    // SAFETY: CoreGraphics C API with correctly sized buffers; the getters take any id from the list.
    unsafe {
        CGGetOnlineDisplayList(0, std::ptr::null_mut(), &mut n);
        let mut ids = vec![0u32; n as usize];
        CGGetOnlineDisplayList(n, ids.as_mut_ptr(), &mut n);
        ids.truncate(n as usize);
        ids.into_iter()
            .map(|id| {
                let b = CGDisplayBounds(id);
                SeenDisplay {
                    id,
                    vendor: CGDisplayVendorNumber(id),
                    serial: CGDisplaySerialNumber(id),
                    active: CGDisplayIsActive(id) != 0,
                    origin: (b.origin.x, b.origin.y),
                    size: (b.size.width, b.size.height),
                }
            })
            .collect()
    }
}

/// The orphan agent displays right now.
#[cfg(target_os = "macos")]
pub fn orphan_displays() -> Vec<SeenDisplay> {
    orphans(&seen_displays(), crate::sys::pid_alive)
}

/// The start guard on the live machine. A display whose instance has just ended is still listed for a moment (its
/// removal is asynchronous), so an orphan is given `settle` to go before it counts.
#[cfg(target_os = "macos")]
pub fn check_start(settle: std::time::Duration) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    loop {
        let seen = seen_displays();
        let verdict = start_gate(real_awake(&seen), &orphans(&seen, crate::sys::pid_alive));
        if verdict.is_ok() || t0.elapsed() >= settle {
            return verdict;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// An instance directory with the socket a running sway would have made (a plain file is enough: only its
    /// presence inside `run` is checked).
    fn instance() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("atlas-agent-a1");
        std::fs::create_dir_all(run_dir(&dir)).unwrap();
        std::fs::write(run_dir(&dir).join("wayland-0"), "").unwrap();
        (t, dir)
    }

    fn env_of(dir: &Path) -> HashMap<String, String> {
        let sway = run_dir(dir).join("sway-ipc.sock");
        let mut m: HashMap<String, String> = linux_env(dir, "wayland-0", &sway)
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        m.insert(
            "ATLAS_DEBUG_SOCK".into(),
            dir.join("debug.sock").to_string_lossy().into_owned(),
        );
        m
    }

    fn check(m: &HashMap<String, String>) -> Result<(), String> {
        verify_linux(|k| m.get(k).cloned())
    }

    #[test]
    fn the_environment_the_cli_builds_is_the_one_the_app_accepts() {
        let (_t, dir) = instance();
        check(&env_of(&dir)).unwrap();
    }

    // The user's own session, as it looks in a terminal on Hyprland/GNOME/KDE.
    #[test]
    fn the_users_session_is_refused_variable_by_variable() {
        let (_t, dir) = instance();
        let user = [
            ("XDG_RUNTIME_DIR", "/run/user/1000"),
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ("GDK_BACKEND", "x11"),
            ("NO_AT_BRIDGE", "0"),
            ("GTK_A11Y", "atspi"),
            ("DISPLAY", ":0"),
            ("SWAYSOCK", "/run/user/1000/sway-ipc.1000.1.sock"),
        ];
        for (k, v) in user {
            let mut m = env_of(&dir);
            m.insert(k.into(), v.into());
            assert!(check(&m).is_err(), "{k}={v} must be refused");
        }
        // And each missing, except the optional ones.
        for k in [
            "XDG_RUNTIME_DIR",
            "WAYLAND_DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS",
            "GDK_BACKEND",
            "NO_AT_BRIDGE",
            "GTK_A11Y",
            "ATLAS_DEBUG_SOCK",
        ] {
            let mut m = env_of(&dir);
            m.remove(k);
            assert!(check(&m).is_err(), "missing {k} must be refused");
        }
        let mut m = env_of(&dir);
        m.remove("SWAYSOCK");
        check(&m).unwrap();
    }

    #[test]
    fn a_wayland_display_that_escapes_the_run_dir_or_does_not_exist_is_refused() {
        let (_t, dir) = instance();
        for wl in [
            "wayland-9",
            "../../../../run/user/1000/wayland-0",
            "/run/user/1000/wayland-0",
        ] {
            let mut m = env_of(&dir);
            m.insert("WAYLAND_DISPLAY".into(), wl.into());
            assert!(check(&m).is_err(), "{wl}");
        }
    }

    fn display(id: u32, vendor: u32, serial: u32, active: bool, x: f64) -> SeenDisplay {
        SeenDisplay {
            id,
            vendor,
            serial,
            active,
            origin: (x, 0.0),
            size: (2560.0, 1640.0),
        }
    }

    /// Incident 2026-11-19, as `CGGetOnlineDisplayList` listed it: the built-in panel asleep, five agent displays
    /// (serial = the pid of the instance that made it) stacked to the right, none of those pids alive.
    fn incident() -> Vec<SeenDisplay> {
        let mut v = vec![display(1, 0x610, 4_251_086_178, false, 0.0)];
        for (id, pid, x) in [
            (91, 91889, 11592.0),
            (92, 94101, 9032.0),
            (93, 94595, 6472.0),
            (94, 95602, 3912.0),
            (95, 4055, 1352.0),
        ] {
            v.push(display(id, AGENT_VENDOR, pid, true, x));
        }
        v
    }

    #[test]
    fn displays_of_instances_that_are_gone_are_orphans_and_the_users_panel_never_is() {
        let o = orphans(&incident(), |_| false);
        assert_eq!(
            o.iter().map(|d| d.id).collect::<Vec<_>>(),
            [91, 92, 93, 94, 95]
        );
        // a live instance's display, and the panel whose "serial" is not a pid, are not orphans
        let o = orphans(&incident(), |pid| pid == 94101 || pid == 4_251_086_178);
        assert_eq!(o.iter().map(|d| d.id).collect::<Vec<_>>(), [91, 93, 94, 95]);
    }

    #[test]
    fn no_display_is_created_while_the_only_real_one_sleeps() {
        let seen = incident();
        assert!(
            !real_awake(&seen),
            "the panel is asleep and the rest are agents'"
        );
        assert_eq!(
            start_gate(real_awake(&seen), &[]),
            Err(NO_REAL_DISPLAY.into())
        );
        let mut awake = seen;
        awake[0].active = true;
        assert!(real_awake(&awake));
    }

    #[test]
    fn no_display_is_created_on_top_of_orphans_and_the_message_says_how_to_clear_them() {
        let mut seen = incident();
        seen[0].active = true; // awake again
        let o = orphans(&seen, |_| false);
        let why = start_gate(real_awake(&seen), &o).unwrap_err();
        for want in [
            "5 orphan",
            "x=11592",
            "pid 4055",
            "--gc",
            "log out or reboot",
        ] {
            assert!(why.contains(want), "{want} missing from: {why}");
        }
        assert_eq!(start_gate(true, &[]), Ok(()));
    }
}
