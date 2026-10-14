//! Host address a mobile dev server should advertise (`host_mode` of session
//! actions). Injected into the session as `TAURI_DEV_HOST` and `ATLAS_DEV_HOST`.

use atlas_core::sys::CommandExt as _;
use std::process::Command;

use crate::config::{HostMode, SessionPlatform};

pub const LOOPBACK: &str = "127.0.0.1";

/// Env var names set on the session (never overriding the user's own `env`).
pub const ENV_VARS: [&str; 2] = ["TAURI_DEV_HOST", "ATLAS_DEV_HOST"];

/// Where the target device runs, as far as atlas can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Simulator,
    Physical,
    /// No device selected, or it could not be classified.
    Unknown,
}

/// Android: adb reports emulators as `emulator-<port>`, everything else is hardware.
pub fn android_kind(serial: &str) -> DeviceKind {
    if serial.starts_with("emulator-") {
        DeviceKind::Simulator
    } else {
        DeviceKind::Physical
    }
}

/// iOS: `devices` is the picker list `(id, display)`; physical entries are the
/// ones `devicectl` produced, labelled `… [device]`. Anything else (a
/// simulator UDID or name) is a simulator.
pub fn ios_kind(selected: &str, devices: &[(String, String)]) -> DeviceKind {
    match devices
        .iter()
        .find(|(id, name)| id == selected || name == selected)
    {
        Some((_, name)) if name.ends_with("[device]") => DeviceKind::Physical,
        Some(_) => DeviceKind::Simulator,
        None => DeviceKind::Unknown,
    }
}

/// The decision: `true` = advertise the LAN address, `false` = loopback.
/// `auto` goes LAN only for a device known to be physical (an unknown device
/// stays on loopback, which is always reachable from simulators).
pub fn wants_lan(mode: HostMode, kind: DeviceKind) -> bool {
    match mode {
        HostMode::Lan => true,
        HostMode::Simulator => false,
        HostMode::Auto => kind == DeviceKind::Physical,
    }
}

/// Resolve the host for a session. Blocking (spawns `route`/`ifconfig`/`ip`
/// and, for iOS `auto`, `xcrun`): call from `spawn_blocking`.
/// A failed LAN lookup falls back to loopback.
pub fn resolve_blocking(
    mode: HostMode,
    platform: Option<&SessionPlatform>,
    device_id: Option<&str>,
) -> String {
    let kind = match (mode, device_id, platform) {
        (HostMode::Auto, Some(id), Some(SessionPlatform::Android)) => android_kind(id),
        (HostMode::Auto, Some(id), Some(SessionPlatform::Ios)) => {
            ios_kind(id, &ios_devices_blocking())
        }
        _ => DeviceKind::Unknown,
    };
    if wants_lan(mode, kind) {
        lan_ipv4_blocking().unwrap_or_else(|| LOOPBACK.to_string())
    } else {
        LOOPBACK.to_string()
    }
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).no_window().args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Picker-equivalent list for iOS (simulators + `devicectl` physical devices).
fn ios_devices_blocking() -> Vec<(String, String)> {
    let mut devices = Vec::new();
    if let Some(t) = run("xcrun", &["simctl", "list", "devices", "--json"]) {
        devices.extend(super::manager::parse_simctl_json(&t));
    }
    if let Some(t) = run(
        "xcrun",
        &[
            "devicectl",
            "list",
            "devices",
            "--json-output",
            "/dev/stdout",
        ],
    ) {
        devices.extend(super::manager::parse_devicectl_json(&t));
    }
    devices
}

/// LAN IPv4 of the default-route interface.
/// macOS: `route -n get default` → `interface:` → `ipconfig getifaddr <if>`.
/// Linux: `ip route get 1.1.1.1` → `src <ip>`.
pub fn lan_ipv4_blocking() -> Option<String> {
    if cfg!(target_os = "macos") {
        let iface = parse_route_interface(&run("route", &["-n", "get", "default"])?)?;
        let ip = run("ipconfig", &["getifaddr", &iface])?;
        valid_ipv4(ip.trim())
    } else {
        parse_ip_route_src(&run("ip", &["route", "get", "1.1.1.1"])?)
    }
}

fn valid_ipv4(s: &str) -> Option<String> {
    s.parse::<std::net::Ipv4Addr>().ok().map(|a| a.to_string())
}

/// `interface: en0` line of `route -n get default`.
fn parse_route_interface(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("interface:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `src <ip>` token of `ip route get 1.1.1.1`.
fn parse_ip_route_src(text: &str) -> Option<String> {
    let mut it = text.split_whitespace();
    it.find(|t| *t == "src")?;
    valid_ipv4(it.next()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_and_simulator_modes_ignore_device_kind() {
        for kind in [
            DeviceKind::Simulator,
            DeviceKind::Physical,
            DeviceKind::Unknown,
        ] {
            assert!(wants_lan(HostMode::Lan, kind));
            assert!(!wants_lan(HostMode::Simulator, kind));
        }
    }

    #[test]
    fn auto_is_lan_only_for_physical_devices() {
        assert!(wants_lan(HostMode::Auto, DeviceKind::Physical));
        assert!(!wants_lan(HostMode::Auto, DeviceKind::Simulator));
        assert!(!wants_lan(HostMode::Auto, DeviceKind::Unknown));
    }

    #[test]
    fn android_emulator_vs_hardware() {
        assert_eq!(android_kind("emulator-5554"), DeviceKind::Simulator);
        assert_eq!(android_kind("R58M123ABC"), DeviceKind::Physical);
        assert_eq!(android_kind("192.168.1.9:5555"), DeviceKind::Physical);
    }

    #[test]
    fn ios_physical_is_marked_device() {
        let devices = vec![
            (
                "SIM-UDID".to_string(),
                "iPhone 16 Pro (iPhone-16-Pro)".to_string(),
            ),
            ("PHYS-ID".to_string(), "Jane's iPhone [device]".to_string()),
        ];
        assert_eq!(ios_kind("SIM-UDID", &devices), DeviceKind::Simulator);
        assert_eq!(ios_kind("PHYS-ID", &devices), DeviceKind::Physical);
        assert_eq!(ios_kind("nope", &devices), DeviceKind::Unknown);
    }

    #[test]
    fn parses_macos_route_output() {
        let text = "   route to: default\ndestination: default\n       mask: default\n    gateway: 192.168.1.1\n  interface: en0\n      flags: <UP,GATEWAY,DONE,STATIC,PRCLONING,GLOBAL>\n";
        assert_eq!(parse_route_interface(text).as_deref(), Some("en0"));
        assert_eq!(parse_route_interface("nothing"), None);
    }

    #[test]
    fn parses_linux_ip_route_src() {
        let text = "1.1.1.1 via 192.168.1.1 dev eth0 src 192.168.1.42 uid 1000 \n    cache\n";
        assert_eq!(parse_ip_route_src(text).as_deref(), Some("192.168.1.42"));
        assert_eq!(parse_ip_route_src("1.1.1.1 dev lo"), None);
        assert_eq!(parse_ip_route_src("src not-an-ip"), None);
    }

    #[test]
    fn simulator_mode_resolves_loopback_without_spawning() {
        assert_eq!(
            resolve_blocking(HostMode::Simulator, Some(&SessionPlatform::Ios), Some("x")),
            LOOPBACK
        );
    }
}
