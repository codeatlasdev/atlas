//! The diagnostics bundle behind Atlas Desktop's "Report" and "Copy details" and `atlas doctor
//! --bundle`: one place that knows what a bug report about the app or its terminal host needs
//! (versions of app, CLI and hostd, distro, session, GPU, how Atlas was installed, crashes, the
//! tail of `app.log`, `stderr.log` and `hostd.log`, the recent hostd incidents) and that redacts
//! secrets before anything leaves this module.
//!
//! The machine facts are `atlas_core::probe`; this module adds what only the host side knows
//! (logs under the base dir, versions, incidents) and renders the text both surfaces print.
//!
//! The desktop frontend never assembles this by hand: it calls `host_diagnostics` and renders the
//! [`Diagnostics`] it gets (camelCase, serialized from these structs; the contract tests below
//! serialize the real value). Its text (`lib/errors/report.ts`) and [`details_text`] print the
//! same lines for the fields both know; `diagnostics.details.txt` (written by the test below from
//! the same value as `diagnostics.json`) is what the frontend tests compare against.
//!
//! Sources, all best effort (a report is built because something is already broken, so a missing
//! file or a dead daemon is an empty field, never an error).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use atlas_core::probe::{self, Crashes, Machine, Packaging, Platform, Runtime};
use atlas_core::redact::redact;
use serde::Serialize;

/// Bytes of a log read from its end before collapsing repeats. The report is bounded by
/// [`TAIL_LINES`], so this only has to reach past a burst of one repeated error to its cause.
pub const TAIL_BYTES: u64 = 1024 * 1024;
/// Lines of a tail that go into a report.
pub const TAIL_LINES: usize = 40;

/// What a report says about the machine and the three binaries that must agree.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Versions {
    pub app: String,
    /// `atlas version` of the installed CLI; `None` when it is missing or did not answer.
    pub cli: Option<String>,
    /// `info.version` of the running terminal host; `None` when it did not answer.
    pub hostd: Option<String>,
    pub hostd_pid: Option<u64>,
    pub hostd_caps: Vec<String>,
}

impl Versions {
    /// `info` is hostd's answer to the `info` call, or `None` when it did not answer.
    pub fn from_info(app: &str, cli: Option<String>, info: Option<&serde_json::Value>) -> Self {
        Self {
            app: app.to_string(),
            cli,
            hostd: info.and_then(|i| i["version"].as_str().map(str::to_string)),
            hostd_pid: info.and_then(|i| i["pid"].as_u64()),
            hostd_caps: info
                .map(|i| {
                    crate::upgrade::caps_of(i)
                        .into_iter()
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// Where Atlas is installed and how macOS treats it: the facts that make one machine differ from
/// another with the same version (`/Applications` vs `~/Applications`, quarantine, translocation,
/// a dev symlink). Paths have the home directory collapsed to `~`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    /// The `.app` this process runs from; `None` when it is not a bundle (`desktop:dev`).
    pub running_bundle: Option<String>,
    /// The `Atlas.app` in `/Applications` or `~/Applications`.
    pub installed_app: Option<String>,
    pub cli: Option<String>,
    /// Where the CLI symlink points (`atlas link`); `None` for a regular file.
    pub cli_linked_to: Option<String>,
    /// The running bundle carries `com.apple.quarantine` (first launch not yet approved, or a
    /// download that was never cleared).
    pub quarantined: bool,
    /// macOS runs the app from a randomized read-only copy (`AppTranslocation`): a quarantined app
    /// started from outside `/Applications`. It cannot see its siblings, update itself or reach
    /// its own CLI by relative path.
    pub translocated: bool,
    /// The followed update channel.
    pub channel: String,
}

fn tilde(p: &Path) -> String {
    redact(&p.to_string_lossy())
}

impl Install {
    /// Pure assembly (unit-tested); `xattrs` is the output of `xattr` for the running bundle.
    pub fn from_parts(
        running: Option<PathBuf>,
        installed: Option<PathBuf>,
        cli: Option<PathBuf>,
        cli_linked_to: Option<String>,
        xattrs: Option<&str>,
        channel: &str,
    ) -> Self {
        Self {
            translocated: running
                .as_deref()
                .is_some_and(|p| p.to_string_lossy().contains("/AppTranslocation/")),
            quarantined: xattrs.is_some_and(|x| x.contains("com.apple.quarantine")),
            running_bundle: running.as_deref().map(tilde),
            installed_app: installed.as_deref().map(tilde),
            cli: cli.as_deref().map(tilde),
            cli_linked_to: cli_linked_to.map(|l| redact(&l)),
            channel: channel.to_string(),
        }
    }

    /// Read the facts of this machine; `cli` is the installed CLI binary the caller found. Best
    /// effort: every missing piece is `None`/`false`.
    pub fn gather(cli: Option<PathBuf>) -> Self {
        let running = std::env::current_exe().ok().and_then(|exe| {
            let app = exe.parent()?.parent()?.parent()?;
            (app.extension()? == "app").then(|| app.to_path_buf())
        });
        let xattrs = running.as_ref().and_then(|b| {
            let out = std::process::Command::new("xattr").arg(b).output().ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        });
        let linked = cli
            .as_ref()
            .and_then(|c| std::fs::read_link(c).ok())
            .map(|t| t.to_string_lossy().into_owned());
        Self::from_parts(
            running,
            atlas_core::paths::installed_app(),
            cli,
            linked,
            xattrs.as_deref(),
            &atlas_core::channel::current().to_string(),
        )
    }
}

/// One thing that went wrong with the terminal host, newest last.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Incident {
    /// Unix seconds.
    pub at: u64,
    /// `unreachable | restarted | upgrade-failed | panic | start-failed`, and for the GPU terminal
    /// `terminal-rebuild | terminal-recovered | terminal-rebuild-failed | terminal-system-event | terminal-dropped`.
    pub kind: String,
    pub detail: String,
}

/// Everything "Report", "Copy details" and `atlas doctor --bundle` carry. Secrets are already redacted.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub versions: Versions,
    /// `Arch Linux [arch], kernel 7.2.15 (x86_64)`, `macOS 15.1 (aarch64)`.
    pub os: String,
    pub install: Install,
    /// Distro, session, GPU and the graphics stack (#275).
    pub platform: Platform,
    /// AppImage, package owner, `~/.local/bin` (#275).
    pub packaging: Packaging,
    /// Core dump rows and Atlas crash files with their time and age (#275).
    pub crashes: Crashes,
    /// Last lines of `hostd.log`, home directory collapsed to `~`.
    pub hostd_log: String,
    /// Last lines of the app log (panics, host incidents).
    pub app_log: String,
    /// Last lines of `stderr.log`: what GTK, WebKit and the web processes printed (#273).
    pub stderr_log: String,
    pub incidents: Vec<Incident>,
    /// The newest `atlas` crash report (CLI, hostd or app panic): version, message and location,
    /// never the backtrace or the arguments. `None` when there is none. Its time and age are
    /// `crashes.atlas[0]`: a crash from last week must not read as new.
    pub last_crash: Option<String>,
}

/// Last [`TAIL_LINES`] lines of `path` (reading at most [`TAIL_BYTES`] from its end), ANSI escapes
/// stripped, runs of identical lines collapsed into one `(repeated N×)` line so a burst of the same
/// error cannot push its cause out of the tail, and redacted. Empty when missing or unreadable.
pub fn tail(path: &Path) -> String {
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(TAIL_BYTES);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let text = crate::strip_ansi(&String::from_utf8_lossy(&buf));
    let mut lines = text.lines();
    // A read that began mid-file starts mid-line: drop that fragment.
    if start > 0 {
        lines.next();
    }
    let mut runs: Vec<(&str, usize)> = Vec::new();
    for l in lines {
        match runs.last_mut() {
            Some((prev, n)) if *prev == l => *n += 1,
            _ => runs.push((l, 1)),
        }
    }
    let skip = runs.len().saturating_sub(TAIL_LINES);
    let out: Vec<String> = runs[skip..]
        .iter()
        .map(|&(l, n)| {
            if n > 1 {
                format!("{l}  (repeated {n}×)")
            } else {
                l.to_string()
            }
        })
        .collect();
    redact(&out.join("\n"))
}

/// Assemble a bundle from what the caller gathered; reads the three logs under `base`.
pub fn assemble(
    base: &Path,
    versions: Versions,
    install: Install,
    incidents: Vec<Incident>,
    machine: Machine,
) -> Diagnostics {
    Diagnostics {
        versions,
        os: probe::os_label(&machine.platform),
        install,
        hostd_log: tail(&crate::paths::log(base)),
        app_log: tail(&crate::paths::app_log(base)),
        stderr_log: tail(&crate::paths::stderr_log(base)),
        incidents,
        last_crash: machine.crashes.atlas.first().map(|c| c.summary.clone()),
        platform: machine.platform,
        packaging: machine.packaging,
        crashes: machine.crashes,
    }
}

/// Probe this machine and assemble the bundle: the one call the app and the CLI make. `cli` is the
/// installed CLI binary, `runtime` what the process knows about its web view (the app only).
pub fn gather(
    base: &Path,
    crash_dir: &Path,
    versions: Versions,
    cli: Option<PathBuf>,
    runtime: &Runtime,
    incidents: Vec<Incident>,
) -> Diagnostics {
    let exe = std::env::current_exe().ok();
    let machine = probe::gather(exe.as_deref(), crash_dir, runtime);
    assemble(base, versions, Install::gather(cli), incidents, machine)
}

/// The sentences that precede a host error: what is skewed, if anything. Same rules and words as
/// `findings` in `lib/errors/report.ts`.
pub fn findings(d: &Diagnostics) -> Vec<String> {
    let Versions {
        app, cli, hostd, ..
    } = &d.versions;
    let mut out = Vec::new();
    if let Some(h) = hostd.as_deref().filter(|h| !app.is_empty() && *h != app) {
        out.push(format!("Terminal host {h} is older than Atlas {app}."));
    }
    if let Some(c) = cli.as_deref().filter(|c| !app.is_empty() && *c != app) {
        out.push(format!(
            "CLI {c} differs from the app {app} (an update did not finish, or the app is linked to a dev build)."
        ));
    }
    if d.install.translocated {
        out.push("The app runs from a macOS translocated copy (App Translocation): move Atlas.app to /Applications and reopen it.".into());
    }
    if let Some(l) = &d.install.cli_linked_to {
        out.push(format!("The CLI is linked to a dev build ({l})."));
    }
    if let (Some(r), Some(i)) = (&d.install.running_bundle, &d.install.installed_app)
        && r != i
        && !d.install.translocated
    {
        out.push(format!(
            "The running app ({r}) is not the installed one ({i})."
        ));
    }
    out
}

/// `2026-11-15 10:00:00 UTC` from unix seconds (civil-from-days, proleptic Gregorian).
fn iso_utc(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// `45s`, `12m`, `5h`, `3d`.
fn age(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86_400 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
}

/// The machine, packaging and crash lines of the report: `machineLines` in
/// `lib/errors/machine.ts` prints the same text for the same payload.
pub fn machine_lines(d: &Diagnostics) -> Vec<String> {
    let (p, k, c) = (&d.platform, &d.packaging, &d.crashes);
    let mut o = Vec::new();
    let opt = |label: &str, v: &Option<String>| v.as_ref().map(|v| format!("{label}: {v}"));
    o.extend(opt("Distro", &p.distro));
    o.extend(opt("Kernel", &p.kernel));
    let session: Vec<String> = [
        p.session.clone(),
        p.desktop.as_ref().map(|v| format!("desktop {v}")),
        p.compositor.as_ref().map(|v| format!("compositor {v}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !session.is_empty() {
        o.push(format!("Session: {}", session.join(", ")));
    }
    o.extend(p.gpus.iter().map(|g| format!("GPU: {g}")));
    o.extend(opt("NVIDIA driver", &p.nvidia_driver));
    o.extend(opt("Mesa", &p.mesa));
    o.extend(opt("GTK", &p.gtk));
    o.extend(opt("Web view", &p.webview));
    if !p.graphics_env.is_empty() {
        o.push(format!("Graphics env: {}", p.graphics_env.join(" ")));
    }
    o.extend(opt("AppImage", &k.appimage));
    if let Some(exe) = &k.exe {
        match &k.exe_owner {
            Some(m) => o.push(format!("Binary: {exe} (package: {m})")),
            None => o.push(format!("Binary: {exe}")),
        }
    }
    if !k.local_bin.is_empty() {
        o.push(format!("~/.local/bin: {}", k.local_bin.join(", ")));
    }
    match (c.coredump_status.as_str(), c.coredumps.len()) {
        ("", _) => {}
        ("ok", 0) => o.push("Core dumps (24h): none".into()),
        ("ok", n) => {
            o.push(format!("Core dumps (24h): {n}"));
            o.extend(
                c.coredumps
                    .iter()
                    .map(|r| format!("  {} {} pid {} {}", iso_utc(r.at), r.signal, r.pid, r.exe)),
            );
        }
        (s, _) => o.push(format!("Core dumps (24h): {s}")),
    }
    match c.atlas.first() {
        Some(f) => o.push(format!(
            "Atlas crash reports: newest {} ({} ago)",
            iso_utc(f.at),
            age(f.age_secs)
        )),
        None => o.push("Atlas crash reports: none".into()),
    }
    o
}

/// The text `atlas doctor --bundle` prints, line for line what Atlas Desktop's "Copy details"
/// (`detailsText` in `lib/errors/report.ts`) puts on the clipboard for the same report.
pub fn details_text(
    version: Option<&str>,
    place: &str,
    kind: Option<&str>,
    message: &str,
    stack: &str,
    d: &Diagnostics,
) -> String {
    let mut lines = vec![
        format!("Atlas {}", version.unwrap_or("(version unknown)")),
        format!(
            "Where: {place}{}",
            kind.map(|k| format!(" ({k})")).unwrap_or_default()
        ),
        String::new(),
        message.to_string(),
        if stack.is_empty() {
            String::new()
        } else {
            format!("\n{stack}")
        },
        String::new(),
        format!(
            "Terminal host: {}",
            d.versions.hostd.as_deref().unwrap_or("not running")
        ),
        format!("CLI: {}", d.versions.cli.as_deref().unwrap_or("unknown")),
        format!("OS: {}", if d.os.is_empty() { "unknown" } else { &d.os }),
        format!(
            "Running bundle: {}",
            d.install
                .running_bundle
                .as_deref()
                .unwrap_or("(not a bundle)")
        ),
        format!(
            "Installed app: {}",
            d.install.installed_app.as_deref().unwrap_or("none found")
        ),
    ];
    lines.extend(machine_lines(d));
    lines.extend(findings(d).into_iter().map(|f| format!("Finding: {f}")));
    if let Some(c) = &d.last_crash {
        lines.push(format!("Last crash: {}", redact(c)));
    }
    for (name, text) in [
        ("app.log", &d.app_log),
        ("stderr.log", &d.stderr_log),
        ("hostd.log", &d.hostd_log),
    ] {
        if !text.is_empty() {
            lines.extend([String::new(), format!("{name} (tail):"), redact(text)]);
        }
    }
    lines.join("\n").trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::probe::{Coredump, CrashFile};
    use atlas_core::redact::REDACTED;

    fn assembled(base: &Path, versions: Versions, machine: Machine) -> Diagnostics {
        assemble(
            base,
            versions,
            Install::from_parts(None, None, None, None, None, "alpha"),
            Vec::new(),
            machine,
        )
    }

    #[test]
    fn tail_keeps_the_last_lines_in_order_and_redacts_them() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hostd.log");
        let mut body = String::new();
        for i in 0..200 {
            body.push_str(&format!("line {i}\n"));
        }
        body.push_str("TOKEN=supersecretvalue123\n");
        std::fs::write(&p, body).unwrap();
        let t = tail(&p);
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines[TAIL_LINES - 2], "line 199");
        assert_eq!(lines[TAIL_LINES - 1], format!("TOKEN={REDACTED}"));
    }

    #[test]
    fn tail_of_a_huge_log_starts_on_a_whole_line() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.log");
        // Distinct lines (identical ones would collapse) and more than TAIL_BYTES of them, so the
        // read starts mid-file.
        let body: String = (0..15_000).map(|i| format!("{i:0>99}\n")).collect();
        assert!(body.len() as u64 > TAIL_BYTES);
        std::fs::write(&p, body).unwrap();
        let t = tail(&p);
        assert!(t.lines().all(|l| l.len() == 99), "a fragment leaked in");
    }

    /// Regression (1.5.0-alpha.8 report): hostd.log ended in 1300+ copies of "another atlas hostd
    /// is running" in red ANSI, so the report's 40-line tail was that one line 40 times with raw
    /// escape codes, and the cause ("Too many open files", the takeovers before it) never reached
    /// the issue. Repeats collapse into one line with a count, escapes are stripped, and the lines
    /// before the burst stay in the tail.
    #[test]
    fn tail_collapses_a_repeated_burst_and_keeps_what_came_before() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hostd.log");
        let mut body =
            String::from("atlas hostd 1.5.0-alpha.5 listening (pid 73595, took over 7 pty(s))\n");
        body.push_str("\x1b[1;31merror:\x1b[0m Too many open files (os error 24)\n");
        for _ in 0..1331 {
            body.push_str("\x1b[1;31merror:\x1b[0m another atlas hostd is running (~/.atlas)\n");
        }
        std::fs::write(&p, body).unwrap();
        let t = tail(&p);
        assert!(!t.contains('\x1b'), "ANSI escapes leaked: {t}");
        assert!(t.contains("Too many open files"), "{t}");
        assert!(t.contains("took over 7 pty(s)"), "{t}");
        assert_eq!(
            t.matches("another atlas hostd is running").count(),
            1,
            "{t}"
        );
        assert!(t.contains("(repeated 1331×)"), "{t}");
    }

    #[test]
    fn a_missing_log_is_an_empty_tail_not_an_error() {
        // 1.5.0-alpha.5 scenario "hostd.log missing": the report is still built.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(tail(&dir.path().join("nope.log")), "");
        let d = assembled(dir.path(), Versions::default(), Machine::default());
        assert!(d.hostd_log.is_empty() && d.app_log.is_empty() && d.stderr_log.is_empty());
    }

    /// A hostd panic: the CLI's panic hook (`atlas_core::crash`) prints the banner into hostd's
    /// stderr, which is `hostd.log`, and writes `crash-<ts>.toml`. The bundle carries both the log
    /// banner and the crash file's message/location (redacted), newest crash only as `last_crash`.
    #[test]
    fn a_hostd_panic_reaches_the_bundle_from_the_log_and_the_crash_file() {
        let base = tempfile::tempdir().unwrap();
        let crashes = tempfile::tempdir().unwrap();
        std::fs::write(
            crate::paths::log(base.path()),
            "atlas crashed.\n  index out of bounds at crates/atlas-host/src/pty/mod.rs:10:5\n",
        )
        .unwrap();
        std::fs::write(
            crashes.path().join("crash-100.toml"),
            "version = \"1.5.0-alpha.4\"\nmsg = \"old one\"\nlocation = \"a.rs:1:1\"\n",
        )
        .unwrap();
        std::fs::write(
            crashes.path().join("crash-200.toml"),
            "version = \"1.5.0-alpha.5\"\nargs = \"atlas hostd\"\nmsg = \"boom TOKEN=abc12345secret\"\nlocation = \"crates/atlas-host/src/server.rs:9:1\"\n\nbacktrace = \'\'\'\nframe\n\'\'\'\n",
        )
        .unwrap();
        let machine = Machine {
            crashes: Crashes {
                atlas: probe::crash_files(crashes.path(), 1000),
                ..Crashes::default()
            },
            ..Machine::default()
        };
        let d = assembled(base.path(), Versions::default(), machine);
        assert!(d.hostd_log.contains("index out of bounds"));
        let c = d.last_crash.expect("the newest crash report is summarized");
        assert!(c.contains("boom") && c.contains("server.rs:9:1"), "{c}");
        assert!(
            !c.contains("abc12345secret") && !c.contains("old one"),
            "{c}"
        );
        assert!(
            !c.contains("frame"),
            "the backtrace stays out of the report: {c}"
        );
        // #275: it carries when it happened, so an old crash does not read as new.
        assert_eq!(
            (d.crashes.atlas[0].at, d.crashes.atlas[0].age_secs),
            (200, 800)
        );
    }

    #[test]
    fn no_crash_directory_means_no_crash_line() {
        let base = tempfile::tempdir().unwrap();
        let machine = Machine {
            crashes: Crashes {
                atlas: probe::crash_files(&base.path().join("nope"), 0),
                ..Crashes::default()
            },
            ..Machine::default()
        };
        let d = assembled(base.path(), Versions::default(), machine);
        assert_eq!(d.last_crash, None);
        assert_eq!(
            machine_lines(&d).last().map(String::as_str),
            Some("Atlas crash reports: none")
        );
    }

    /// The machine facts a maintainer needs to diagnose a user's install remotely, with the home
    /// directory collapsed: which bundle runs, where the app and CLI are installed, whether macOS
    /// quarantined or translocated the app, and whether the CLI is a dev symlink.
    #[cfg(unix)]
    #[test]
    fn install_facts_name_the_paths_and_flag_translocation_and_quarantine() {
        let home = dirs::home_dir().unwrap();
        let bundle = home.join("Applications/Atlas.app");
        let i = Install::from_parts(
            Some(bundle.clone()),
            Some(bundle.clone()),
            Some(home.join(".atlas/bin/atlas")),
            None,
            Some("com.apple.quarantine"),
            "alpha",
        );
        assert_eq!(
            i.running_bundle.as_deref(),
            Some("~/Applications/Atlas.app")
        );
        assert_eq!(i.installed_app.as_deref(), Some("~/Applications/Atlas.app"));
        assert_eq!(i.cli.as_deref(), Some("~/.atlas/bin/atlas"));
        assert!(i.quarantined);
        assert!(!i.translocated);
        assert_eq!(i.channel, "alpha");
        // Gatekeeper's randomized read-only copy: the running bundle is not the installed one.
        let t = Install::from_parts(
            Some("/private/var/folders/xx/AppTranslocation/ABC/d/Atlas.app".into()),
            Some("/Applications/Atlas.app".into()),
            None,
            None,
            None,
            "stable",
        );
        assert!(t.translocated);
        assert_eq!(t.cli, None);
    }

    /// hostd's `info` answer in its real shape (`atlas hostd --status`), and no answer at all.
    #[test]
    fn versions_come_from_hostds_info_and_a_dead_host_is_none() {
        let info = serde_json::json!({"version": "1.5.0-alpha.4", "pid": 8664, "caps": ["pty_attach_raw", "upgrade"]});
        let v = Versions::from_info("1.5.0-alpha.5", Some("1.5.0-alpha.5".into()), Some(&info));
        assert_eq!(v.hostd.as_deref(), Some("1.5.0-alpha.4"));
        assert_eq!(v.hostd_pid, Some(8664));
        assert_eq!(v.hostd_caps, ["pty_attach_raw", "upgrade"]);
        let none = Versions::from_info("1.5.0-alpha.5", None, None);
        assert_eq!((none.hostd, none.hostd_pid, none.cli), (None, None, None));
        assert!(none.hostd_caps.is_empty());
    }

    #[test]
    fn a_linked_cli_reports_where_it_points() {
        let i = Install::from_parts(
            None,
            None,
            Some("/h/.atlas/bin/atlas".into()),
            Some("/repo/target/debug/atlas".into()),
            None,
            "alpha",
        );
        assert_eq!(i.cli_linked_to.as_deref(), Some("/repo/target/debug/atlas"));
        assert!(i.running_bundle.is_none());
    }

    /// A Linux session, pinned: real structs, hand-picked values.
    fn linux_machine() -> Machine {
        Machine {
            platform: Platform {
                distro: Some("Arch Linux [arch]".into()),
                kernel: Some("7.2.15-1-omarchy".into()),
                session: Some("wayland".into()),
                desktop: Some("Hyprland".into()),
                compositor: Some("Hyprland".into()),
                gpus: vec!["AMD (amdgpu)".into()],
                nvidia_driver: None,
                mesa: Some("26.2.5-1".into()),
                gtk: Some("3.24.52".into()),
                webview: Some("WebKitGTK 2.54.1".into()),
                graphics_env: vec!["WEBKIT_DISABLE_DMABUF_RENDERER=1".into()],
            },
            packaging: Packaging {
                appimage: Some("~/Apps/Atlas_1.5.0-alpha.25_amd64.AppImage".into()),
                exe: Some("/tmp/.mount_AtlasAbc/usr/bin/atlas-desktop".into()),
                exe_owner: None,
                local_bin: vec!["atlas".into()],
            },
            crashes: Crashes {
                coredump_status: "ok".into(),
                coredumps: vec![Coredump {
                    at: 1_700_000_100,
                    pid: 4242,
                    signal: "SIGSEGV".into(),
                    exe: "/tmp/.mount_AtlasAbc/usr/lib/webkit2gtk-4.1/WebKitWebProcess".into(),
                }],
                atlas: vec![CrashFile {
                    at: 1_700_000_000,
                    age_secs: 7200,
                    summary: "atlas 1.5.0-alpha.24: index out of bounds (crates/atlas-host/src/server.rs:9:1)".into(),
                }],
            },
        }
    }

    fn linux_bundle(base: &Path) -> Diagnostics {
        assemble(
            base,
            Versions {
                app: "1.5.0-alpha.25".into(),
                cli: Some("1.5.0-alpha.25".into()),
                hostd: Some("1.5.0-alpha.24".into()),
                hostd_pid: Some(8664),
                hostd_caps: vec!["pty_attach_raw".into()],
            },
            Install::from_parts(
                None,
                None,
                Some("/usr/bin/atlas".into()),
                None,
                None,
                "alpha",
            ),
            Vec::new(),
            linux_machine(),
        )
    }

    #[test]
    fn a_linux_report_says_distro_session_gpu_packaging_and_crashes() {
        let base = tempfile::tempdir().unwrap();
        let d = linux_bundle(base.path());
        assert_eq!(
            machine_lines(&d),
            [
                "Distro: Arch Linux [arch]",
                "Kernel: 7.2.15-1-omarchy",
                "Session: wayland, desktop Hyprland, compositor Hyprland",
                "GPU: AMD (amdgpu)",
                "Mesa: 26.2.5-1",
                "GTK: 3.24.52",
                "Web view: WebKitGTK 2.54.1",
                "Graphics env: WEBKIT_DISABLE_DMABUF_RENDERER=1",
                "AppImage: ~/Apps/Atlas_1.5.0-alpha.25_amd64.AppImage",
                "Binary: /tmp/.mount_AtlasAbc/usr/bin/atlas-desktop",
                "~/.local/bin: atlas",
                "Core dumps (24h): 1",
                "  2023-11-14 22:15:00 UTC SIGSEGV pid 4242 /tmp/.mount_AtlasAbc/usr/lib/webkit2gtk-4.1/WebKitWebProcess",
                "Atlas crash reports: newest 2023-11-14 22:13:20 UTC (2h ago)",
            ]
        );
    }

    #[test]
    fn core_dump_status_says_unavailable_none_or_nothing_where_it_does_not_apply() {
        let base = tempfile::tempdir().unwrap();
        let mut d = linux_bundle(base.path());
        let core = |d: &Diagnostics| {
            machine_lines(d)
                .into_iter()
                .filter(|l| l.starts_with("Core dumps"))
                .collect::<Vec<_>>()
        };
        d.crashes.coredumps.clear();
        assert_eq!(core(&d), ["Core dumps (24h): none"]);
        d.crashes.coredump_status = "unavailable".into();
        assert_eq!(core(&d), ["Core dumps (24h): unavailable"]);
        d.crashes.coredump_status = String::new();
        assert!(core(&d).is_empty());
    }

    #[test]
    fn time_and_age_formats() {
        assert_eq!(iso_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(iso_utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(iso_utc(1_700_000_000), "2023-11-14 22:13:20 UTC");
        assert_eq!(iso_utc(4_102_444_799), "2099-12-31 23:59:59 UTC");
        assert_eq!(
            [0, 59, 60, 3599, 3600, 86_399, 86_400, 700_000].map(age),
            ["0s", "59s", "1m", "59m", "1h", "23h", "1d", "8d"]
        );
    }

    #[test]
    fn findings_name_a_skewed_host_and_a_dev_linked_cli() {
        let base = tempfile::tempdir().unwrap();
        let mut d = linux_bundle(base.path());
        assert_eq!(
            findings(&d),
            ["Terminal host 1.5.0-alpha.24 is older than Atlas 1.5.0-alpha.25."]
        );
        d.install.cli_linked_to = Some("/repo/target/debug/atlas".into());
        d.versions.cli = Some("1.5.0-alpha.1".into());
        assert_eq!(findings(&d).len(), 3);
        // The CLI has no app version of its own to compare with: no skew claims.
        d.versions.app = String::new();
        assert_eq!(
            findings(&d),
            ["The CLI is linked to a dev build (/repo/target/debug/atlas)."]
        );
    }

    /// Everything the new fields carry passes the same redaction as the logs: the home directory
    /// becomes `~` and tokens disappear, whichever field they were in.
    #[test]
    fn the_new_fields_never_carry_a_token_or_the_home_directory() {
        let home = dirs::home_dir().unwrap();
        let home = home.to_string_lossy();
        let base = tempfile::tempdir().unwrap();
        let crashes = tempfile::tempdir().unwrap();
        std::fs::write(
            crashes.path().join("crash-1700000000.toml"),
            // A literal TOML string ('…'): a Windows home's `\U` is not an escape there.
            format!("version = \"1.5.0\"\nmsg = \"boom TOKEN=abc12345secret\"\nlocation = '{home}/src/a.rs:1'\n"),
        )
        .unwrap();
        std::fs::create_dir_all(base.path().join("desktop")).unwrap();
        std::fs::write(
            crate::paths::stderr_log(base.path()),
            format!("WebKitWebProcess: OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz012345 in {home}/x\n"),
        )
        .unwrap();
        // Built with serde so a Windows home (`C:\Users\…`) is escaped into valid JSON.
        let json = serde_json::json!([{
            "time": 1000000, "pid": 1, "sig": 11, "exe": format!("{home}/.atlas/bin/atlas"),
        }])
        .to_string();
        let mut machine = linux_machine();
        machine.crashes.coredumps = probe::parse_coredumps(&json).unwrap();
        machine.crashes.atlas = probe::crash_files(crashes.path(), 1_700_000_100);
        let d = assembled(base.path(), Versions::default(), machine);
        let all = format!(
            "{}\n{}",
            serde_json::to_string(&d).unwrap(),
            details_text(None, "doctor", None, "", "", &d)
        );
        // The JSON half escapes a Windows home's `\`: look for that spelling too.
        let escaped = serde_json::to_string(home.as_ref()).unwrap();
        let escaped = escaped.trim_matches('"');
        for leak in [
            home.as_ref(),
            escaped,
            "abc12345secret",
            "sk-proj-abcdefghijklmnopqrstuvwxyz012345",
        ] {
            assert!(!all.contains(leak), "{leak} leaked: {all}");
        }
        assert!(all.contains("~/.atlas/bin/atlas"), "{all}");
    }

    /// Cross-boundary contract: the frontend (`lib/errors/diagnostics.ts`, `machine.ts`) reads these
    /// exact keys. The value is the REAL probe of this machine, serialized, never a hand-written mock.
    #[test]
    fn the_real_diagnostics_serialize_with_the_keys_the_frontend_reads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(crate::paths::log(dir.path()), "atlas hostd up\n").unwrap();
        let d = gather(
            dir.path(),
            &dir.path().join("crashes"),
            Versions {
                app: "1.5.0-alpha.5".into(),
                cli: Some("1.5.0-alpha.5".into()),
                hostd: Some("1.5.0-alpha.4".into()),
                hostd_pid: Some(7),
                hostd_caps: vec!["pty_attach_raw".into()],
            },
            None,
            &Runtime::default(),
            Vec::new(),
        );
        let v = serde_json::to_value(&d).unwrap();
        for k in [
            "versions",
            "os",
            "install",
            "platform",
            "packaging",
            "crashes",
            "hostdLog",
            "appLog",
            "stderrLog",
            "incidents",
            "lastCrash",
        ] {
            assert!(v.get(k).is_some(), "missing {k} in {v}");
        }
        for k in ["app", "cli", "hostd", "hostdPid", "hostdCaps"] {
            assert!(v["versions"].get(k).is_some(), "missing versions.{k}");
        }
        for k in [
            "distro",
            "kernel",
            "session",
            "desktop",
            "compositor",
            "gpus",
            "nvidiaDriver",
            "mesa",
            "gtk",
            "webview",
            "graphicsEnv",
        ] {
            assert!(v["platform"].get(k).is_some(), "missing platform.{k}");
        }
        for k in ["appimage", "exe", "exeOwner", "localBin"] {
            assert!(v["packaging"].get(k).is_some(), "missing packaging.{k}");
        }
        for k in ["coredumpStatus", "coredumps", "atlas"] {
            assert!(v["crashes"].get(k).is_some(), "missing crashes.{k}");
        }
        assert_eq!(v["hostdLog"], serde_json::json!("atlas hostd up"));
        assert_eq!(v["versions"]["hostd"], "1.5.0-alpha.4");
        // On this OS the probe really found something (an `os` that is not just the family, a
        // kernel on Linux), so a probe that silently returns nothing fails here.
        assert!(
            v["os"]
                .as_str()
                .is_some_and(|o| o.contains(std::env::consts::ARCH))
        );
        #[cfg(target_os = "linux")]
        assert!(v["platform"]["kernel"].is_string(), "{v}");
    }

    /// A deterministic bundle of the macOS shape: real structs, real redaction, real serializer.
    fn mac_bundle() -> Diagnostics {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            crate::paths::log(dir.path()),
            "atlas hostd 1.5.0-alpha.4 listening on /Users/ana/.atlas/hostd.sock (pid 8664, took over 2 pty(s))\n\
             spawn failed: GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("desktop")).unwrap();
        std::fs::write(
            crate::paths::app_log(dir.path()),
            "1791943000 unreachable: socket in use\n",
        )
        .unwrap();
        std::fs::write(
            crate::paths::stderr_log(dir.path()),
            "Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...\n",
        )
        .unwrap();
        let machine = Machine {
            platform: Platform {
                kernel: Some("24.1.0".into()),
                webview: Some("WebKit 619.1.26".into()),
                ..Platform::default()
            },
            packaging: Packaging {
                exe: Some("/Applications/Atlas.app/Contents/MacOS/atlas-desktop".into()),
                ..Packaging::default()
            },
            crashes: Crashes {
                atlas: vec![CrashFile {
                    at: 1_791_940_000,
                    age_secs: 3000,
                    summary: "atlas 1.5.0-alpha.4: index out of bounds (crates/atlas-host/src/server.rs:9:1)".into(),
                }],
                ..Crashes::default()
            },
        };
        let mut d = assemble(
            dir.path(),
            Versions {
                app: "1.5.0-alpha.5".into(),
                cli: Some("1.5.0-alpha.5".into()),
                hostd: Some("1.5.0-alpha.4".into()),
                hostd_pid: Some(8664),
                hostd_caps: vec!["pty_attach_raw".into(), "upgrade".into(), "lsp".into()],
            },
            Install::from_parts(
                Some("/Applications/Atlas.app".into()),
                Some("/Applications/Atlas.app".into()),
                Some("/Users/ana/.atlas/bin/atlas".into()),
                None,
                Some("com.apple.quarantine"),
                "alpha",
            ),
            vec![Incident {
                at: 1_791_943_000,
                kind: "unreachable".into(),
                detail: "socket in use".into(),
            }],
            machine,
        );
        // The OS label asks the machine running the test; the fixture is a contract, not a log.
        d.os = "macOS 15.1 (aarch64)".into();
        d
    }

    fn write_or_check(name: &str, fresh: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__")
            .join(name);
        if std::env::var_os("UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, fresh).unwrap();
        }
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("fixture {name} missing: run with UPDATE_FIXTURES=1"));
        let stale = format!("stale fixture {name}: re-run with UPDATE_FIXTURES=1");
        if name.ends_with(".json") {
            // The formatter may lay the committed file out differently: compare the values.
            let a: serde_json::Value = serde_json::from_str(&committed).unwrap();
            let b: serde_json::Value = serde_json::from_str(fresh).unwrap();
            assert_eq!(a, b, "{stale}");
        } else {
            assert_eq!(committed, fresh, "{stale}");
        }
    }

    fn pretty(v: &serde_json::Value) -> String {
        let mut buf = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(
            &mut buf,
            serde_json::ser::PrettyFormatter::with_indent(b"\t"),
        );
        serde::Serialize::serialize(v, &mut ser).unwrap();
        String::from_utf8(buf).unwrap() + "\n"
    }

    /// Contract with the frontend: `diagnostics.json` (macOS shape) and `diagnostics-linux.json` are
    /// the REAL serialization of [`Diagnostics`] (secrets already redacted), and the `*.details.txt`
    /// next to them is what [`details_text`] (`atlas doctor --bundle`) prints for the same value. The
    /// TS tests parse the JSON and check that "Copy details" prints the same lines, so a rename or a
    /// wording change on either side fails a test. `UPDATE_FIXTURES=1` regenerates them.
    #[test]
    fn fixtures_are_the_real_serialization_and_the_cli_text() {
        let linux_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(linux_dir.path().join("desktop")).unwrap();
        std::fs::write(
            crate::paths::stderr_log(linux_dir.path()),
            "Gtk-WARNING: cannot open display\nCould not create default EGL display: EGL_BAD_PARAMETER\n",
        )
        .unwrap();
        let mut linux = linux_bundle(linux_dir.path());
        linux.os = "Arch Linux [arch], kernel 7.2.15-1-omarchy (x86_64)".into();
        linux.last_crash = linux.crashes.atlas.first().map(|c| c.summary.clone());
        let mac = mac_bundle();
        for (name, d) in [("diagnostics", &mac), ("diagnostics-linux", &linux)] {
            write_or_check(
                &format!("{name}.json"),
                &pretty(&serde_json::to_value(d).unwrap()),
            );
            let text = details_text(
                Some(&d.versions.app),
                "terminal host",
                Some("hostd"),
                "Terminal host not reachable",
                "",
                d,
            );
            write_or_check(&format!("{name}.details.txt"), &(text + "\n"));
        }
        let log = mac.hostd_log.as_str();
        assert!(
            !log.contains("ghp_") && !log.contains("/Users/ana"),
            "{log}"
        );
    }
}
