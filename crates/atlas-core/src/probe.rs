//! The machine facts a bug report needs and nothing else can recover afterwards (#275): which distro,
//! session and GPU ran Atlas, how it was installed, and what crashed. One probe for Atlas Desktop's
//! "Copy details" and `atlas doctor --bundle`, so both say the same thing.
//!
//! Everything is best effort and bounded: a report is being built because something is already
//! broken, so a missing file, an absent tool or a hung `coredumpctl` is an absent field, never an
//! error. Parsers are pure (text in, value out) and unit-tested against real file shapes.
//! Core dumps are never read: only `coredumpctl list` rows (time, pid, signal, executable).

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

/// Distro, kernel, session, GPU and the graphics stack versions. Linux fills all of it; macOS and
/// Windows fill what is cheap (`webview`, `kernel`); a field that cannot be read is `None`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    /// `Arch Linux [arch]`, `Ubuntu 24.04.1 LTS [ubuntu 24.04]`: `/etc/os-release`.
    pub distro: Option<String>,
    pub kernel: Option<String>,
    /// `XDG_SESSION_TYPE`: `wayland`, `x11`, `tty`.
    pub session: Option<String>,
    /// `XDG_CURRENT_DESKTOP`.
    pub desktop: Option<String>,
    /// The compositor or window manager when its own socket variable says which (`Hyprland`, `sway`).
    pub compositor: Option<String>,
    /// One `AMD (amdgpu)` per GPU under `/sys/class/drm`.
    pub gpus: Vec<String>,
    /// Version of the proprietary NVIDIA kernel module, when loaded.
    pub nvidia_driver: Option<String>,
    pub mesa: Option<String>,
    pub gtk: Option<String>,
    /// `WebKitGTK 2.46.5`, `WebKit 619.1.26`, `WebView2 130.0.2849.56`.
    pub webview: Option<String>,
    /// `NAME=value` of the variables that change WebKitGTK's and GTK's rendering path.
    pub graphics_env: Vec<String>,
}

/// How Atlas got onto this machine.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Packaging {
    /// `$APPIMAGE`: the file the AppImage was started from.
    pub appimage: Option<String>,
    /// The running executable.
    pub exe: Option<String>,
    /// `pacman`, `apt`, `dnf`, `zypper`: the package manager whose database owns `exe`.
    pub exe_owner: Option<String>,
    /// `atlas` / `atlas-desktop` found in `~/.local/bin`.
    pub local_bin: Vec<String>,
}

/// One row of `coredumpctl list`: no core contents, no command line.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Coredump {
    /// Unix seconds.
    pub at: u64,
    pub pid: u64,
    /// `SIGSEGV`; `signal 31` for one without a name here.
    pub signal: String,
    pub exe: String,
}

/// A crash report Atlas wrote itself (`atlas_core::crash`).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CrashFile {
    /// Unix seconds of the panic (from the file name).
    pub at: u64,
    /// Seconds between the panic and this report, so a stale crash cannot pass for a new one.
    pub age_secs: u64,
    /// Version, message and location, redacted; never the backtrace or the arguments.
    pub summary: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Crashes {
    /// `ok`; `unavailable` when `coredumpctl` is missing, timed out or failed (not the same as
    /// none); empty where there is no systemd-coredump to ask (macOS, Windows).
    pub coredump_status: String,
    /// Last 24 h, `atlas*` and `WebKit*` executables only.
    pub coredumps: Vec<Coredump>,
    /// Newest first, at most [`MAX_CRASH_FILES`].
    pub atlas: Vec<CrashFile>,
}

/// Everything the probe finds.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Machine {
    pub platform: Platform,
    pub packaging: Packaging,
    pub crashes: Crashes,
}

pub const MAX_CRASH_FILES: usize = 3;
/// Variables that change WebKitGTK's or GTK's rendering path; reported verbatim when set.
pub const GRAPHICS_ENV_PREFIXES: &[&str] = &[
    "WEBKIT_", "GSK_", "GDK_", "__NV_", "__GLX_", "LIBGL_", "MESA_", "LIBVA_",
];
/// One package-manager or `pkg-config` query.
const QUERY_TIMEOUT: Duration = Duration::from_millis(1500);
/// `coredumpctl` reads the journal, which can be slow on a big one.
const COREDUMP_TIMEOUT: Duration = Duration::from_secs(3);

/// Versions the running process knows better than any package database: an AppImage bundles its
/// own WebKitGTK and GTK, so asking the host's package manager would name the wrong ones.
#[derive(Debug, Clone, Default)]
pub struct Runtime {
    pub webkitgtk: Option<String>,
    pub gtk: Option<String>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `Arch Linux [arch]` from the text of `/etc/os-release`.
pub fn distro_line(os_release: &str) -> Option<String> {
    let get = |key: &str| {
        os_release.lines().find_map(|l| {
            let v = l.strip_prefix(key)?.strip_prefix('=')?;
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
            (!v.is_empty()).then(|| v.to_string())
        })
    };
    let id = get("ID");
    let name = get("PRETTY_NAME")
        .or_else(|| get("NAME"))
        .or_else(|| id.clone())?;
    let tag = match (id, get("VERSION_ID")) {
        (Some(id), Some(v)) => format!(" [{id} {v}]"),
        (Some(id), None) => format!(" [{id}]"),
        _ => String::new(),
    };
    Some(format!("{name}{tag}"))
}

fn vendor_name(id: &str) -> String {
    match id {
        "0x10de" => "NVIDIA",
        "0x1002" => "AMD",
        "0x8086" => "Intel",
        "0x1af4" => "virtio",
        "0x15ad" => "VMware",
        "0x1414" => "Microsoft",
        "0x5143" => "Qualcomm",
        "0x106b" => "Apple",
        _ => id,
    }
    .to_string()
}

/// `AMD (amdgpu)` per `<drm>/card*/device` that has a vendor id; the driver is the name of the
/// `driver` symlink, absent for a card with no bound driver.
pub fn drm_gpus(drm: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(drm) else {
        return Vec::new();
    };
    let mut cards: Vec<_> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        // `card0`, never a connector (`card0-DP-1`) or a render node.
        .filter(|n| {
            n.strip_prefix("card")
                .is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
        })
        .collect();
    cards.sort();
    cards
        .iter()
        .filter_map(|c| {
            let dev = drm.join(c).join("device");
            let vendor = std::fs::read_to_string(dev.join("vendor")).ok()?;
            let driver = std::fs::read_link(dev.join("driver"))
                .ok()
                .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "no driver bound".to_string());
            Some(format!("{} ({driver})", vendor_name(vendor.trim())))
        })
        .collect()
}

/// The module version out of `/proc/driver/nvidia/version`.
pub fn nvidia_driver(proc_version: &str) -> Option<String> {
    // "NVRM version: NVIDIA UNIX x86_64 Kernel Module  570.133.07  Thu ..." -> the first dotted number.
    proc_version
        .lines()
        .next()?
        .split_whitespace()
        .find(|w| w.contains('.') && w.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        .map(str::to_string)
}

fn signal_name(sig: u64) -> String {
    match sig {
        4 => "SIGILL".into(),
        5 => "SIGTRAP".into(),
        6 => "SIGABRT".into(),
        7 => "SIGBUS".into(),
        8 => "SIGFPE".into(),
        9 => "SIGKILL".into(),
        11 => "SIGSEGV".into(),
        15 => "SIGTERM".into(),
        n => format!("signal {n}"),
    }
}

/// Rows of `coredumpctl list --json=short` for `atlas*` and `WebKit*` executables (also under an
/// AppImage mount). `None` when the text is not that JSON array.
pub fn parse_coredumps(json: &str) -> Option<Vec<Coredump>> {
    let rows: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    Some(
        rows.iter()
            .filter_map(|r| {
                let exe = r["exe"].as_str()?;
                let base = exe.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
                (base.starts_with("atlas") || base.starts_with("webkit")).then(|| Coredump {
                    // Journal time is microseconds.
                    at: r["time"].as_u64().unwrap_or(0) / 1_000_000,
                    pid: r["pid"].as_u64().unwrap_or(0),
                    signal: r["sig"]
                        .as_u64()
                        .map_or_else(|| "unknown signal".into(), signal_name),
                    exe: crate::redact::redact(exe),
                })
            })
            .collect(),
    )
}

/// What `coredumpctl list` means: an array, or "No coredumps found." with exit 1 (a real "none");
/// anything else (no `--json` on old systemd, no journal access) is not an answer.
pub fn interpret_coredumpctl(ok: bool, stdout: &str, stderr: &str) -> Option<Vec<Coredump>> {
    if ok {
        parse_coredumps(stdout)
    } else {
        stderr.contains("No coredumps found").then(Vec::new)
    }
}

/// Version, message and location of a `crash-*.toml`, redacted; the backtrace and the command
/// line stay in the file on disk. Values are TOML basic strings written by `crash::toml_str`;
/// its escapes (`\\ \" \n \r \t \uXXXX`) are JSON's too.
fn crash_summary(text: &str) -> Option<String> {
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k)?.trim_start().strip_prefix('='))
            .map(|v| {
                let v = v.trim();
                serde_json::from_str::<String>(v)
                    .unwrap_or_else(|_| v.trim_matches('"').to_string())
            })
    };
    let msg = field("msg")?;
    let line = match (field("version"), field("location")) {
        (Some(v), Some(l)) => format!("atlas {v}: {msg} ({l})"),
        (_, Some(l)) => format!("{msg} ({l})"),
        _ => msg,
    };
    Some(crate::redact::redact(&line))
}

/// One crash report on disk: when it was written (unix ms, from its name) and its redacted summary.
#[derive(Debug, Clone, PartialEq)]
pub struct CrashEntry {
    pub at_ms: u64,
    pub summary: String,
}

/// Every crash report in `dir` (`crate::crash::crash_dir`), newest first, read lazily. The time is the one in the
/// file name (`crash-<unix_ms>-<pid>.toml`, or the older `crash-<unix seconds>.toml`), so two generations order on
/// one clock. A report that cannot be read or has no message is skipped.
pub fn crash_entries(dir: &Path) -> impl Iterator<Item = CrashEntry> {
    let mut found: Vec<(u64, std::path::PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            Some((
                crate::crash::report_time(e.file_name().to_str()?)?,
                e.path(),
            ))
        })
        .collect();
    found.sort_by_key(|a| std::cmp::Reverse(a.0));
    found.into_iter().filter_map(|(at_ms, p)| {
        Some(CrashEntry {
            at_ms,
            summary: crash_summary(&std::fs::read_to_string(p).ok()?)?,
        })
    })
}

/// The newest crash reports, at most [`MAX_CRASH_FILES`], with their age at `now_secs`.
pub fn crash_files(dir: &Path, now_secs: u64) -> Vec<CrashFile> {
    crash_entries(dir)
        .take(MAX_CRASH_FILES)
        .map(|c| {
            let at = c.at_ms / 1000;
            CrashFile {
                at,
                age_secs: now_secs.saturating_sub(at),
                summary: c.summary,
            }
        })
        .collect()
}

/// How a bounded child ended: its exit status and what it printed.
pub struct Ran {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

fn drain<R: std::io::Read + Send + 'static>(r: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = r {
            let _ = r.read_to_end(&mut buf);
        }
        buf
    })
}

/// Run `program args…` to its end within `timeout`; the child is killed past it. `None` for a
/// program that is missing or does not finish in time.
pub fn run_full(program: &str, args: &[&str], timeout: Duration) -> Option<Ran> {
    use crate::sys::CommandExt;
    use std::process::Stdio;
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .spawn()
        .ok()?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let (out, err) = (out.join().ok()?, err.join().ok()?);
    Some(Ran {
        ok: status?.success(),
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
    })
}

/// [`run_full`], keeping stdout of a run that exited 0.
pub fn run_bounded(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    run_full(program, args, timeout)
        .filter(|r| r.ok)
        .map(|r| r.stdout)
}

/// The first of [`GRAPHICS_ENV_PREFIXES`]' variables that are set, as `NAME=value`, sorted and redacted.
fn graphics_env(vars: impl Iterator<Item = (String, String)>) -> Vec<String> {
    let mut out: Vec<String> = vars
        .filter(|(k, v)| !v.is_empty() && GRAPHICS_ENV_PREFIXES.iter().any(|p| k.starts_with(p)))
        .map(|(k, v)| crate::redact::redact(&format!("{k}={v}")))
        .collect();
    out.sort();
    out
}

/// The compositor or window manager, from the socket variable it exports to every client (also
/// when started from a TTY, where there is no `XDG_CURRENT_DESKTOP`).
fn compositor(var: &impl Fn(&str) -> Option<String>) -> Option<String> {
    [
        ("HYPRLAND_INSTANCE_SIGNATURE", "Hyprland"),
        ("SWAYSOCK", "sway"),
        ("NIRI_SOCKET", "niri"),
        ("I3SOCK", "i3"),
    ]
    .iter()
    .find(|(k, _)| var(k).is_some())
    .map(|(_, n)| (*n).to_string())
}

/// A library whose version is asked for by `pkg-config`, else by each package manager's name for it.
#[cfg(target_os = "linux")]
struct Lib {
    pc: &'static str,
    pacman: &'static str,
    dpkg: &'static [&'static str],
    rpm: &'static str,
}

#[cfg(target_os = "linux")]
const WEBKITGTK: Lib = Lib {
    pc: "webkit2gtk-4.1",
    pacman: "webkit2gtk-4.1",
    dpkg: &["libwebkit2gtk-4.1-0"],
    rpm: "webkit2gtk4.1",
};
#[cfg(target_os = "linux")]
const GTK: Lib = Lib {
    pc: "gtk+-3.0",
    pacman: "gtk3",
    dpkg: &["libgtk-3-0t64", "libgtk-3-0"],
    rpm: "gtk3",
};
#[cfg(target_os = "linux")]
const MESA: Lib = Lib {
    pc: "dri",
    pacman: "mesa",
    dpkg: &["libgl1-mesa-dri"],
    rpm: "mesa-dri-drivers",
};

/// The installed version of `lib`. `run` returns a program's stdout on exit 0 (stubbed in tests
/// with the real tools' output shapes). A machine without the `-dev` packages has no `.pc` file,
/// which is the common case for a user, so the package databases are the main route.
#[cfg(target_os = "linux")]
fn lib_version(lib: &Lib, run: &dyn Fn(&str, &[&str]) -> Option<String>) -> Option<String> {
    let first = |s: String| {
        s.lines()
            .next()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
    };
    if let Some(v) = run("pkg-config", &["--modversion", lib.pc]).and_then(first) {
        return Some(v);
    }
    // `webkit2gtk-4.1 2.46.5-1`
    if let Some(v) = run("pacman", &["-Q", lib.pacman])
        .and_then(|o| o.split_whitespace().nth(1).map(str::to_string))
    {
        return Some(v);
    }
    for pkg in lib.dpkg {
        if let Some(v) = run("dpkg-query", &["-W", "-f=${Version}\\n", pkg]).and_then(first) {
            return Some(v);
        }
    }
    run("rpm", &["-q", "--qf", "%{VERSION}-%{RELEASE}\\n", lib.rpm]).and_then(first)
}

/// `10.0.26100.1` out of `Microsoft Windows [Version 10.0.26100.1]` (`cmd /c ver`).
#[cfg(any(windows, test))]
fn parse_windows_ver(out: &str) -> Option<String> {
    let v = out.split("Version").nth(1)?.trim().trim_end_matches(']');
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// `130.0.2849.56` out of `reg query … /v pv` (`    pv    REG_SZ    130.0.2849.56`).
#[cfg(any(windows, test))]
fn parse_reg_pv(out: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        (w.next()? == "pv" && w.next()? == "REG_SZ")
            .then(|| w.next().map(str::to_string))
            .flatten()
    })
}

#[cfg(windows)]
fn webview2_version() -> Option<String> {
    // The Evergreen runtime registers itself here, machine-wide (32-bit view) or per user.
    const KEYS: [&str; 2] = [
        r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        r"HKCU\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
    ];
    KEYS.iter()
        .find_map(|key| {
            run_bounded("reg", &["query", key, "/v", "pv"], QUERY_TIMEOUT)
                .and_then(|o| parse_reg_pv(&o))
        })
        .map(|v| format!("WebView2 {v}"))
}

/// `Arch Linux [arch], kernel 7.2.15 (x86_64)`, `macOS 15.1 (aarch64)`, `Windows 10.0.26100.1 (x86_64)`.
pub fn os_label(platform: &Platform) -> String {
    let arch = std::env::consts::ARCH;
    match std::env::consts::OS {
        "linux" => match (&platform.distro, &platform.kernel) {
            (Some(d), Some(k)) => format!("{d}, kernel {k} ({arch})"),
            (Some(d), None) => format!("{d} ({arch})"),
            (None, Some(k)) => format!("Linux, kernel {k} ({arch})"),
            (None, None) => format!("Linux ({arch})"),
        },
        "macos" => {
            let v = run_bounded("sw_vers", &["-productVersion"], QUERY_TIMEOUT)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
            v.map_or_else(
                || format!("macOS ({arch})"),
                |v| format!("macOS {v} ({arch})"),
            )
        }
        #[cfg(windows)]
        "windows" => {
            let v = run_bounded("cmd", &["/d", "/c", "ver"], QUERY_TIMEOUT)
                .and_then(|o| parse_windows_ver(&o));
            v.map_or_else(
                || format!("Windows ({arch})"),
                |v| format!("Windows {v} ({arch})"),
            )
        }
        o => format!("{o} ({arch})"),
    }
}

fn platform(rt: &Runtime) -> Platform {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let mut p = Platform {
        session: var("XDG_SESSION_TYPE"),
        desktop: var("XDG_CURRENT_DESKTOP"),
        compositor: compositor(&var),
        graphics_env: graphics_env(std::env::vars()),
        ..Platform::default()
    };
    #[cfg(target_os = "linux")]
    {
        let read = |p: &str| std::fs::read_to_string(p).ok();
        p.distro = read("/etc/os-release")
            .or_else(|| read("/usr/lib/os-release"))
            .and_then(|t| distro_line(&t));
        p.kernel = read("/proc/sys/kernel/osrelease")
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());
        p.gpus = drm_gpus(Path::new("/sys/class/drm"));
        p.nvidia_driver = read("/proc/driver/nvidia/version").and_then(|t| nvidia_driver(&t));
        let run = |prog: &str, args: &[&str]| run_bounded(prog, args, QUERY_TIMEOUT);
        let (mesa, gtk, webkit) = std::thread::scope(|s| {
            let mesa = s.spawn(|| lib_version(&MESA, &run));
            let gtk = s.spawn(|| rt.gtk.clone().or_else(|| lib_version(&GTK, &run)));
            let webkit = s.spawn(|| {
                rt.webkitgtk
                    .clone()
                    .or_else(|| lib_version(&WEBKITGTK, &run))
            });
            (
                mesa.join().ok().flatten(),
                gtk.join().ok().flatten(),
                webkit.join().ok().flatten(),
            )
        });
        p.mesa = mesa;
        p.gtk = gtk;
        p.webview = webkit.map(|v| format!("WebKitGTK {v}"));
    }
    #[cfg(target_os = "macos")]
    {
        let _ = rt;
        p.kernel = run_bounded("uname", &["-r"], QUERY_TIMEOUT)
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());
        p.webview = run_bounded(
            "plutil",
            &[
                "-extract",
                "CFBundleShortVersionString",
                "raw",
                "-o",
                "-",
                "/System/Library/Frameworks/WebKit.framework/Resources/Info.plist",
            ],
            QUERY_TIMEOUT,
        )
        .map(|v| format!("WebKit {}", v.trim()))
        .filter(|v| v.len() > "WebKit ".len());
    }
    #[cfg(windows)]
    {
        let _ = rt;
        p.webview = webview2_version();
    }
    p
}

/// `atlas` and `atlas-desktop` in `<home>/.local/bin` (a link counts, also a dangling one).
fn local_bin(home: &Path) -> Vec<String> {
    ["atlas", "atlas-desktop"]
        .iter()
        .filter(|n| home.join(".local/bin").join(n).symlink_metadata().is_ok())
        .map(|n| (*n).to_string())
        .collect()
}

/// Pure assembly of [`Packaging`]: every path and URL-ish value is redacted (home as `~`, tokens gone).
fn packaging_of(
    appimage: Option<String>,
    exe: Option<&Path>,
    owner: Option<crate::pkgowner::PackageManager>,
    home: Option<&Path>,
) -> Packaging {
    Packaging {
        appimage: appimage
            .filter(|p| !p.is_empty())
            .map(|p| crate::redact::redact(&p)),
        exe: exe.map(|e| crate::redact::redact(&e.to_string_lossy())),
        exe_owner: owner.map(|m| m.name().to_string()),
        local_bin: home.map(local_bin).unwrap_or_default(),
    }
}

fn packaging(exe: Option<&Path>) -> Packaging {
    let owner = exe
        .filter(|_| cfg!(target_os = "linux"))
        .and_then(|e| crate::pkgowner::owner_with(e, |p, a| run_bounded(p, a, QUERY_TIMEOUT)));
    packaging_of(
        std::env::var("APPIMAGE").ok(),
        exe,
        owner,
        dirs::home_dir().as_deref(),
    )
}

fn coredumps() -> (String, Vec<Coredump>) {
    // Only systemd-coredump has this question; elsewhere it is not "unavailable", it does not apply.
    if !cfg!(target_os = "linux") {
        return (String::new(), Vec::new());
    }
    let rows = run_full(
        "coredumpctl",
        &["list", "--since", "-1d", "--json=short", "--no-pager"],
        COREDUMP_TIMEOUT,
    )
    .and_then(|r| interpret_coredumpctl(r.ok, &r.stdout, &r.stderr));
    match rows {
        Some(rows) => ("ok".into(), rows),
        None => ("unavailable".into(), Vec::new()),
    }
}

/// Probe this machine. `exe` is the running executable, `crash_dir` where Atlas writes crash
/// reports, `runtime` what the process itself knows about its web view. Slow queries run in
/// parallel and are each bounded, so a hung package database delays a report by seconds, not forever.
pub fn gather(exe: Option<&Path>, crash_dir: &Path, runtime: &Runtime) -> Machine {
    std::thread::scope(|s| {
        let dumps = s.spawn(coredumps);
        let platform = s.spawn(|| platform(runtime));
        let packaging = packaging(exe);
        let atlas = crash_files(crash_dir, now_secs());
        let (coredump_status, coredumps) = dumps
            .join()
            .unwrap_or_else(|_| ("unavailable".into(), Vec::new()));
        Machine {
            platform: platform.join().unwrap_or_default(),
            packaging,
            crashes: Crashes {
                coredump_status,
                coredumps,
                atlas,
            },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCH: &str = "NAME=\"Arch Linux\"\nPRETTY_NAME=\"Arch Linux\"\nID=arch\nBUILD_ID=rolling\nANSI_COLOR=\"38;2;23;147;209\"\nHOME_URL=\"https://archlinux.org/\"\n";
    const UBUNTU: &str = "PRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nNAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nVERSION=\"24.04.1 LTS (Noble Numbat)\"\nID=ubuntu\nID_LIKE=debian\n";
    const FEDORA: &str = "NAME=\"Fedora Linux\"\nVERSION=\"40 (Workstation Edition)\"\nID=fedora\nVERSION_ID=40\nPRETTY_NAME=\"Fedora Linux 40 (Workstation Edition)\"\nVARIANT_ID=workstation\n";

    #[test]
    fn os_release_of_arch_ubuntu_and_fedora_give_one_distro_line() {
        // Rolling Arch has no VERSION_ID: the line must not end in a dangling space.
        assert_eq!(distro_line(ARCH).as_deref(), Some("Arch Linux [arch]"));
        assert_eq!(
            distro_line(UBUNTU).as_deref(),
            Some("Ubuntu 24.04.1 LTS [ubuntu 24.04]")
        );
        assert_eq!(
            distro_line(FEDORA).as_deref(),
            Some("Fedora Linux 40 (Workstation Edition) [fedora 40]")
        );
    }

    #[test]
    fn os_release_without_a_name_or_id_is_not_a_distro() {
        assert_eq!(distro_line(""), None);
        assert_eq!(distro_line("# nothing useful\nHOME_URL=x\n"), None);
        // Only an ID: still names the distro.
        assert_eq!(distro_line("ID=nixos\n").as_deref(), Some("nixos [nixos]"));
    }

    #[cfg(unix)]
    fn card(root: &Path, name: &str, vendor: Option<&str>, driver: Option<&str>) {
        let dev = root.join(name).join("device");
        std::fs::create_dir_all(&dev).unwrap();
        if let Some(v) = vendor {
            std::fs::write(dev.join("vendor"), format!("{v}\n")).unwrap();
        }
        if let Some(d) = driver {
            // Real sysfs: `driver` is a symlink into `/sys/bus/pci/drivers/<name>`.
            let target = root.join("bus").join("drivers").join(d);
            std::fs::create_dir_all(&target).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, dev.join("driver")).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn drm_sysfs_gives_one_gpu_line_per_card_with_vendor_and_driver() {
        let d = tempfile::tempdir().unwrap();
        let drm = d.path().join("drm");
        card(&drm, "card0", Some("0x10de"), Some("nvidia"));
        card(&drm, "card1", Some("0x1002"), Some("amdgpu"));
        card(&drm, "card2", Some("0x8086"), None);
        // Connectors (`card1-DP-1`) and render nodes are not cards; a card without a vendor is skipped.
        card(&drm, "card1-DP-1", None, None);
        card(&drm, "renderD128", Some("0x1002"), Some("amdgpu"));
        card(&drm, "card3", None, None);
        assert_eq!(
            drm_gpus(&drm),
            ["NVIDIA (nvidia)", "AMD (amdgpu)", "Intel (no driver bound)"]
        );
        assert!(drm_gpus(&d.path().join("missing")).is_empty());
    }

    #[test]
    fn nvidia_proc_version_gives_the_module_version() {
        let v = "NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  570.133.07  Release Build  (archlinux-builder@archlinux)  Thu Mar 20 2025\nGCC version:  gcc version 14.2.1\n";
        assert_eq!(nvidia_driver(v).as_deref(), Some("570.133.07"));
        assert_eq!(nvidia_driver("garbage"), None);
    }

    /// Shape of `coredumpctl list --since -1d --json=short` (systemd 257), trimmed to two rows
    /// of ours and one that is not: the filter keeps `atlas*` and `WebKit*` (also under an
    /// AppImage mount) and drops everything else; times are microseconds.
    const COREDUMPS: &str = r#"[
        {"time":1794707950780314,"pid":476694,"uid":1000,"gid":1000,"sig":11,"corefile":"present","exe":"/usr/lib/xdg-desktop-portal-hyprland","size":858280},
        {"time":1794776957835540,"pid":681071,"uid":1000,"gid":1000,"sig":6,"corefile":"present","exe":"/usr/bin/atlas-desktop","size":1108180},
        {"time":1794777355899393,"pid":31180,"uid":1000,"gid":1000,"sig":11,"corefile":"missing","exe":"/tmp/.mount_AtlasXyz/usr/lib/webkit2gtk-4.1/WebKitWebProcess","size":null},
        {"time":1794777400000000,"pid":31181,"uid":1000,"gid":1000,"sig":31,"corefile":"present","exe":"/opt/atlas/bin/atlas","size":1}
    ]"#;

    #[test]
    fn coredumpctl_json_keeps_atlas_and_webkit_rows_only() {
        let rows = parse_coredumps(COREDUMPS).unwrap();
        assert_eq!(
            rows,
            [
                Coredump {
                    at: 1794776957,
                    pid: 681071,
                    signal: "SIGABRT".into(),
                    exe: "/usr/bin/atlas-desktop".into()
                },
                Coredump {
                    at: 1794777355,
                    pid: 31180,
                    signal: "SIGSEGV".into(),
                    exe: "/tmp/.mount_AtlasXyz/usr/lib/webkit2gtk-4.1/WebKitWebProcess".into()
                },
                Coredump {
                    at: 1794777400,
                    pid: 31181,
                    signal: "signal 31".into(),
                    exe: "/opt/atlas/bin/atlas".into()
                },
            ]
        );
    }

    #[test]
    fn coredumpctl_with_no_rows_or_unparseable_output() {
        // No dumps in the window: `coredumpctl` prints an empty array: a real "none".
        assert_eq!(parse_coredumps("[]"), Some(Vec::new()));
        // Anything else is not an answer (old systemd without --json, a pager message).
        assert_eq!(parse_coredumps("No coredumps found."), None);
        assert_eq!(parse_coredumps(""), None);
    }

    /// Observed on systemd 262: no match is exit 1 with "No coredumps found." on stderr and nothing
    /// on stdout, which is an answer ("none"); a failure without that text is not.
    #[test]
    fn coredumpctl_no_match_is_none_but_a_failure_is_unavailable() {
        assert_eq!(
            interpret_coredumpctl(false, "", "No coredumps found.\n"),
            Some(Vec::new())
        );
        assert_eq!(
            interpret_coredumpctl(false, "", "Failed to open journal: Permission denied\n"),
            None
        );
        assert_eq!(interpret_coredumpctl(true, "[]", ""), Some(Vec::new()));
        assert_eq!(interpret_coredumpctl(true, "not json", ""), None);
    }

    #[test]
    fn the_executable_of_a_core_dump_is_redacted_like_every_other_path() {
        let home = dirs::home_dir().unwrap();
        // Built with serde so a Windows home (`C:\Users\…`) is escaped into valid JSON.
        let json = serde_json::json!([{
            "time": 1000000, "pid": 1, "sig": 11,
            "exe": format!("{}/.atlas/bin/atlas", home.display()),
        }])
        .to_string();
        let rows = parse_coredumps(&json).unwrap();
        assert_eq!(rows[0].exe, "~/.atlas/bin/atlas");
    }

    /// Real output shapes of each package manager's query; the first route that answers wins.
    #[cfg(target_os = "linux")]
    #[test]
    fn library_versions_come_from_pkg_config_or_the_package_database() {
        let answers = |table: &'static [(&'static str, &'static str)]| {
            move |prog: &str, _: &[&str]| {
                table
                    .iter()
                    .find(|(p, _)| *p == prog)
                    .map(|(_, o)| (*o).to_string())
            }
        };
        // Arch without -dev .pc files for it: pacman answers `name version`.
        assert_eq!(
            lib_version(
                &WEBKITGTK,
                &answers(&[("pacman", "webkit2gtk-4.1 2.46.5-1\n")])
            )
            .as_deref(),
            Some("2.46.5-1")
        );
        // Ubuntu: dpkg-query prints the bare version.
        assert_eq!(
            lib_version(&GTK, &answers(&[("dpkg-query", "3.24.41-4ubuntu1.3\n")])).as_deref(),
            Some("3.24.41-4ubuntu1.3")
        );
        // Fedora.
        assert_eq!(
            lib_version(&MESA, &answers(&[("rpm", "24.1.5-1.fc40\n")])).as_deref(),
            Some("24.1.5-1.fc40")
        );
        // pkg-config beats the databases (it is what the linker would use).
        assert_eq!(
            lib_version(
                &GTK,
                &answers(&[("pkg-config", "3.24.52\n"), ("pacman", "gtk3 3.24.51-1\n")])
            )
            .as_deref(),
            Some("3.24.52")
        );
        assert_eq!(lib_version(&GTK, &answers(&[])), None);
        // A blank answer is not a version.
        assert_eq!(lib_version(&GTK, &answers(&[("pkg-config", "\n")])), None);
    }

    #[test]
    fn windows_build_and_webview2_version_parse_from_real_tool_output() {
        assert_eq!(
            parse_windows_ver("\r\nMicrosoft Windows [Version 10.0.26100.1742]\r\n").as_deref(),
            Some("10.0.26100.1742")
        );
        assert_eq!(parse_windows_ver("nothing"), None);
        let reg = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}\r\n    pv    REG_SZ    130.0.2849.56\r\n";
        assert_eq!(parse_reg_pv(reg).as_deref(), Some("130.0.2849.56"));
        assert_eq!(
            parse_reg_pv("ERROR: The system was unable to find the specified registry key"),
            None
        );
    }

    #[test]
    fn compositor_and_graphics_env_come_from_the_environment() {
        let env = |set: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                set.iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| (*v).to_string())
            }
        };
        assert_eq!(
            compositor(&env(&[("HYPRLAND_INSTANCE_SIGNATURE", "abc_1_2")])).as_deref(),
            Some("Hyprland")
        );
        assert_eq!(
            compositor(&env(&[("SWAYSOCK", "/run/x")])).as_deref(),
            Some("sway")
        );
        assert_eq!(compositor(&env(&[])), None);
        let vars = [
            ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
            ("GDK_BACKEND", "x11"),
            ("LIBGL_ALWAYS_SOFTWARE", ""),
            ("HOME", "/h"),
            ("MESA_TOKEN", "TOKEN=abc12345secret"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let out = graphics_env(vars.into_iter());
        // Sorted, empty values and unrelated variables dropped, secrets redacted.
        assert_eq!(out.len(), 3, "{out:?}");
        assert_eq!(out[0], "GDK_BACKEND=x11");
        assert!(out[1].starts_with("MESA_TOKEN=") && !out[1].contains("abc12345secret"));
        assert_eq!(out[2], "WEBKIT_DISABLE_DMABUF_RENDERER=1");
    }

    #[test]
    fn local_bin_lists_the_atlas_links_that_exist() {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        assert!(local_bin(home.path()).is_empty());
        std::fs::write(bin.join("atlas"), "x").unwrap();
        std::fs::write(bin.join("other"), "x").unwrap();
        assert_eq!(local_bin(home.path()), ["atlas"]);
    }

    /// The AppImage path and the executable are user paths: home becomes `~`, a token in them is gone.
    #[test]
    fn packaging_paths_are_redacted_and_the_owner_is_named() {
        let home = dirs::home_dir().unwrap();
        let appimage = format!(
            "{}/Apps/Atlas.AppImage?TOKEN=abc12345secret",
            home.display()
        );
        // A string, like the AppImage above: `Path::join` would put a `\` after a Windows home.
        let exe = std::path::PathBuf::from(format!("{}/.local/bin/atlas-desktop", home.display()));
        let p = packaging_of(
            Some(appimage),
            Some(&exe),
            Some(crate::pkgowner::PackageManager::Pacman),
            None,
        );
        let appimage = p.appimage.unwrap();
        assert!(appimage.starts_with("~/Apps/Atlas.AppImage"), "{appimage}");
        assert!(!appimage.contains("abc12345secret"), "{appimage}");
        assert_eq!(p.exe.as_deref(), Some("~/.local/bin/atlas-desktop"));
        assert_eq!(p.exe_owner.as_deref(), Some("pacman"));
        // An empty $APPIMAGE (set but blank) is not an AppImage.
        assert_eq!(
            packaging_of(Some(String::new()), None, None, None).appimage,
            None
        );
    }

    /// Regression (#275): `last_crash` carried no time, so a crash from last week was attached to
    /// today's report as if it were new. Every crash file now carries its time and age.
    #[test]
    fn crash_files_carry_time_and_age_newest_first() {
        let d = tempfile::tempdir().unwrap();
        for (name, msg) in [
            ("crash-1700000000.toml", "week old"),
            ("crash-1700600000123-42.toml", "fresh TOKEN=abc12345secret"),
            ("crash-1700500000-7.toml", "middle"),
            ("not-a-crash.toml", "ignored"),
        ] {
            std::fs::write(d.path().join(name), format!("version = \"1.5.0\"\nmsg = \"{msg}\"\nlocation = \"a.rs:1\"\n\nbacktrace = '''\nframe\n'''\n")).unwrap();
        }
        let files = crash_files(d.path(), 1700600100);
        assert_eq!(files.len(), 3);
        assert_eq!(
            (files[0].at, files[0].age_secs),
            (1700600000, 100),
            "{files:?}"
        );
        assert_eq!(files[2].at, 1700000000);
        assert_eq!(files[2].age_secs, 600100);
        assert!(files[0].summary.starts_with("atlas 1.5.0: fresh"));
        assert!(!files[0].summary.contains("abc12345secret"));
        assert!(!files.iter().any(|f| f.summary.contains("frame")));
        // A clock that went backwards: age saturates instead of wrapping.
        assert_eq!(crash_files(d.path(), 10)[0].age_secs, 0);
    }

    #[test]
    fn crash_files_are_capped_and_a_missing_dir_is_empty() {
        let d = tempfile::tempdir().unwrap();
        for i in 0..(MAX_CRASH_FILES + 4) {
            std::fs::write(
                d.path().join(format!("crash-{}.toml", 1700000000 + i)),
                format!("msg = \"m{i}\"\n"),
            )
            .unwrap();
        }
        let files = crash_files(d.path(), 1800000000);
        assert_eq!(files.len(), MAX_CRASH_FILES);
        assert_eq!(files[0].summary, format!("m{}", MAX_CRASH_FILES + 3));
        assert!(crash_files(&d.path().join("nope"), 0).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_program_is_killed_at_the_timeout_and_a_missing_one_is_none() {
        let t = std::time::Instant::now();
        assert_eq!(
            run_bounded("sleep", &["5"], Duration::from_millis(200)),
            None
        );
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
        assert_eq!(
            run_bounded("echo", &["hi"], Duration::from_secs(2)).as_deref(),
            Some("hi\n")
        );
        assert_eq!(run_bounded("false", &[], Duration::from_secs(2)), None);
        assert_eq!(
            run_bounded("atlas-no-such-tool", &[], Duration::from_secs(1)),
            None
        );
    }
}
