//! The private display of a Linux agent instance (`atlas desktop --agent`): a headless sway and a session bus of its
//! own under `<instance dir>/run`, so the window never reaches the user's screen (#280) and the app's single-instance
//! D-Bus name never meets the user's Atlas (#281). The environment the app gets from it is
//! `atlas_core::agent_display::linux_env`; the app refuses to start on anything else.
//!
//! The sway setup must agree with `scripts/linux-shot.sh` (same backend, renderer and output name): the two are the
//! same private-display recipe, one as a shell script for CI, one started from here.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use super::desktop_agent::{Aux, Session};

/// Logical size of the agent output; `ATLAS_AGENT_SCALE=2` doubles the pixels, not the layout.
const OUTPUT: (u32, u32) = (1440, 900);
const STARTUP: Duration = Duration::from_secs(15);

/// What a started display gives the app.
pub struct Display {
    /// Compositor and bus, in start order (`--stop` ends them in reverse).
    pub aux: Vec<Aux>,
    /// `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `SWAYSOCK`, …: the app's display variables.
    pub env: Vec<(&'static str, String)>,
}

/// `ATLAS_AGENT_SCALE`: `2` is a retina output; anything else is a standard 1x monitor (the CI default).
pub fn scale_from(value: Option<&str>) -> u32 {
    if value == Some("2") { 2 } else { 1 }
}

/// The sway config of an instance. Floating windows keep the size the app asks for (`debug resize` and
/// `shot --full` rely on it); no borders or title bars (the page draws its own), no Xwayland (no `DISPLAY`).
pub fn sway_config(scale: u32) -> String {
    format!(
        "output HEADLESS-1 resolution {}x{} scale {scale}\ndefault_border none\ndefault_floating_border none\nxwayland disable\nfor_window [app_id=\".*\"] floating enable\nexec_always true\n",
        OUTPUT.0 * scale,
        OUTPUT.1 * scale
    )
}

/// A minimal session bus: its own socket, no service activation (a portal or a notification daemon of the user's
/// would otherwise be started on, and outlive, the private bus).
pub fn bus_config(instance: &Path) -> String {
    format!(
        "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n<busconfig><type>session</type><listen>{}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\" eavesdrop=\"true\"/><allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>\n",
        atlas_core::agent_display::bus_address(instance)
    )
}

/// The Wayland socket a compositor made in `run`: the only `wayland-N` entry (not its `.lock`).
pub fn wayland_socket(run: &Path) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(run)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            n.strip_prefix("wayland-")
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    names.sort();
    names.into_iter().next()
}

fn sway_ipc_socket(run: &Path) -> Option<PathBuf> {
    std::fs::read_dir(run)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("sway-ipc.") && n.ends_with(".sock"))
        })
}

/// A program on the caller's `PATH`.
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .any(|d| d.join(name).is_file())
}

/// Kills what a failed start already spawned.
struct Spawned(Vec<Child>);

impl Drop for Spawned {
    fn drop(&mut self) {
        for c in &mut self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// A daemon of the display: no environment but `env`, no terminal, its own process group, output to `log`, and no
/// new privileges (sway's file capabilities would hide its `ATLAS_SESSION` tag from the hygiene sweep).
fn spawn(prog: &str, args: &[&str], env: &[(&str, String)], log: &Path) -> Result<Child> {
    let out = std::fs::File::create(log)?;
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (*k, v)))
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out)
        .process_group(0);
    atlas_core::sys::no_new_privs(&mut cmd);
    cmd.spawn().with_context(|| format!("starting {prog}"))
}

fn wait_for<T>(
    what: &str,
    child: &mut Child,
    log: &Path,
    mut ready: impl FnMut() -> Option<T>,
) -> Result<T> {
    let t0 = Instant::now();
    loop {
        if let Some(v) = ready() {
            return Ok(v);
        }
        if atlas_core::sys::interrupted() {
            bail!("interrupted while starting {what}");
        }
        if let Some(status) = child.try_wait()? {
            let tail = std::fs::read_to_string(log).unwrap_or_default();
            bail!(
                "{what} exited ({status}) before it was ready: {}",
                tail.trim()
            );
        }
        if t0.elapsed() > STARTUP {
            bail!(
                "{what} was not ready after {}s; see {}",
                STARTUP.as_secs(),
                log.display()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Start the private session bus and the headless compositor of `s`. On error nothing is left running.
pub fn start(s: &Session, scale: u32, path: &str, session: &str) -> Result<Display> {
    for tool in ["sway", "dbus-daemon"] {
        if !on_path(tool) {
            bail!(
                "{tool} not found: an agent instance needs a headless Wayland compositor (Arch: `pacman -S sway grim`; Debian/Ubuntu: `apt install sway grim dbus`)"
            );
        }
    }
    let run = atlas_core::agent_display::run_dir(&s.dir);
    // The caller's own runtime dir is never ours: refuse before anything is created in it.
    if std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|d| Path::new(&d) == run) {
        bail!(
            "{} is this shell's XDG_RUNTIME_DIR; refusing to run an agent display in it",
            run.display()
        );
    }
    std::fs::create_dir_all(&run)?;
    std::fs::set_permissions(&run, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    std::fs::write(s.dir.join("bus.conf"), bus_config(&s.dir))?;
    std::fs::write(s.dir.join("sway.conf"), sway_config(scale))?;
    let run_s = run.to_string_lossy().into_owned();
    let bus = atlas_core::agent_display::bus_address(&s.dir);

    let mut spawned = Spawned(Vec::new());
    let bus_log = s.dir.join("dbus.log");
    let conf = s.dir.join("bus.conf").to_string_lossy().into_owned();
    let child = spawn(
        "dbus-daemon",
        &["--nofork", "--config-file", &conf],
        &[
            ("PATH", path.to_string()),
            (atlas_core::hygiene::SESSION_ENV, session.to_string()),
        ],
        &bus_log,
    )?;
    spawned.0.push(child);
    let sock = run.join("bus");
    wait_for("dbus-daemon", &mut spawned.0[0], &bus_log, || {
        sock.exists().then_some(())
    })?;

    let sway_log = s.dir.join("sway.log");
    let conf = s.dir.join("sway.conf").to_string_lossy().into_owned();
    let child = spawn(
        "sway",
        &["-c", &conf],
        &[
            ("PATH", path.to_string()),
            (atlas_core::hygiene::SESSION_ENV, session.to_string()),
            ("HOME", s.home().to_string_lossy().into_owned()),
            ("XDG_RUNTIME_DIR", run_s.clone()),
            ("DBUS_SESSION_BUS_ADDRESS", bus),
            ("XDG_SESSION_TYPE", "wayland".into()),
            ("WLR_BACKENDS", "headless".into()),
            ("WLR_LIBINPUT_NO_DEVICES", "1".into()),
            ("WLR_RENDERER", "pixman".into()),
        ],
        &sway_log,
    )?;
    spawned.0.push(child);
    let (wayland, ipc) = wait_for("sway", &mut spawned.0[1], &sway_log, || {
        Some((wayland_socket(&run)?, sway_ipc_socket(&run)?))
    })?;

    let aux = spawned
        .0
        .iter()
        .map(|c| {
            Ok(Aux {
                pid: c.id(),
                start: atlas_core::procs::start_time(c.id())
                    .context("the display process vanished")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    // Disarm: from here `--stop` owns them (the caller records `aux` before anything else can fail).
    spawned.0.clear();
    Ok(Display {
        aux,
        env: atlas_core::agent_display::linux_env(&s.dir, &wayland, &ipc),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_is_a_floating_headless_one_with_no_x_server() {
        let c = sway_config(1);
        assert!(c.contains("output HEADLESS-1 resolution 1440x900 scale 1"));
        assert!(c.contains("floating enable") && c.contains("xwayland disable"));
        assert!(sway_config(2).contains("resolution 2880x1800 scale 2"));
    }

    #[test]
    fn only_a_two_asks_for_retina_and_a_typo_stays_standard() {
        assert_eq!(scale_from(Some("2")), 2);
        for v in [None, Some("1"), Some("3"), Some(""), Some("retina")] {
            assert_eq!(scale_from(v), 1, "{v:?}");
        }
    }

    #[test]
    fn the_bus_listens_on_the_address_the_app_is_given() {
        let c = bus_config(Path::new("/tmp/atlas-agent-t"));
        assert!(c.contains("<listen>unix:path=/tmp/atlas-agent-t/run/bus</listen>"));
        assert!(
            !c.contains("servicedir"),
            "no service activation on the private bus"
        );
    }

    #[test]
    fn the_wayland_socket_is_the_numbered_one_never_its_lock() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(wayland_socket(t.path()), None);
        for n in ["wayland-0.lock", "wayland-x", "sway-ipc.1.2.sock", "bus"] {
            std::fs::write(t.path().join(n), "").unwrap();
        }
        assert_eq!(wayland_socket(t.path()), None);
        std::fs::write(t.path().join("wayland-0"), "").unwrap();
        assert_eq!(wayland_socket(t.path()).as_deref(), Some("wayland-0"));
    }
}
