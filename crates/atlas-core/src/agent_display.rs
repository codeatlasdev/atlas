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
}
