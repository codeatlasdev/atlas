use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::redact::{redact, redact_args};

const ISSUES_URL: &str = "https://github.com/codeatlasdev/atlas/issues/new";

/// Installs a panic hook that writes a TOML crash report and prints a
/// human-friendly message with a link to file the issue.
///
/// Skipped in debug builds and when RUST_BACKTRACE is set.
pub fn install_panic_hook() {
    if cfg!(debug_assertions) || std::env::var("RUST_BACKTRACE").is_ok() {
        return;
    }

    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("unknown panic payload");

        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());

        let report_path = write_crash_report(msg, &location);

        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr);
        let _ = writeln!(stderr, "\x1b[1;31matlas crashed.\x1b[0m");
        let _ = writeln!(stderr);
        let _ = writeln!(stderr, "  \x1b[2m{msg}\x1b[0m");
        let _ = writeln!(stderr, "  \x1b[2mat {location}\x1b[0m");
        let _ = writeln!(stderr);

        match &report_path {
            Some(p) => {
                let _ = writeln!(stderr, "  Crash report: {}", p.display());
                let _ = writeln!(stderr, "  Review it for private data, then attach it:");
            }
            None => {
                let _ = writeln!(stderr, "  (could not write crash report to disk)");
                let _ = writeln!(stderr, "  File a bug:");
            }
        }
        let _ = writeln!(stderr, "    {ISSUES_URL}");

        let _ = writeln!(stderr);
        default(info);
    }));
}

/// Writes a TOML crash report to the atlas data directory.
/// Returns the path if successful.
pub fn write_crash_report(msg: &str, location: &str) -> Option<PathBuf> {
    // `capture()` records nothing unless RUST_BACKTRACE is set, and an installed app never has it.
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    let args = std::env::args().collect::<Vec<_>>();
    write_report_in(&crash_dir(), msg, location, &args, &backtrace)
}

/// [`write_crash_report`] with the directory and the process facts given.
///
/// The file is what a user attaches to a public issue, so every free-text field (and the backtrace, which
/// names the home directory in every frame) goes through [`crate::redact::redact`]. Named
/// `crash-<unix_ms>-<pid>.toml` so two panics in one second are two files; only the newest [`KEEP`] stay.
fn write_report_in(
    dir: &Path,
    msg: &str,
    location: &str,
    args: &[String],
    backtrace: &str,
) -> Option<PathBuf> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ts = now.as_secs();

    std::fs::create_dir_all(dir).ok()?;

    let report = format!(
        "version = \"{version}\"\nos = \"{os}\"\narch = \"{arch}\"\ntimestamp = {ts}\nargs = {args}\nmsg = {msg}\nlocation = {location}\n\nbacktrace = '''\n{backtrace}\n'''\n",
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        msg = toml_str(&redact(msg)),
        args = toml_str(&redact_args(args)),
        location = toml_str(&redact(location)),
        backtrace = redact(backtrace).replace("'''", "' ' '"),
    );

    let path = create_new_report(dir, now.as_millis(), &report)?;
    prune(dir, &path);
    Some(path)
}

/// Writes `report` to a file no one else has: `crash-<ms>-<pid>.toml`, then `-1`, `-2`… for panics of one process in
/// the same millisecond (a panic storm on several threads). `create_new` makes the claim atomic.
fn create_new_report(dir: &Path, ms: u128, report: &str) -> Option<PathBuf> {
    let pid = std::process::id();
    for n in 0.. {
        let name = if n == 0 {
            format!("crash-{ms}-{pid}.toml")
        } else {
            format!("crash-{ms}-{pid}-{n}.toml")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                f.write_all(report.as_bytes()).ok()?;
                return Some(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }
    }
    None
}

/// Crash reports kept in the crash directory.
const KEEP: usize = 20;

/// When a crash report was written, in unix milliseconds, from its file name: `crash-<unix_ms>-<pid>.toml`,
/// or the older `crash-<unix seconds>.toml` (seconds are scaled, so both generations order on one clock and
/// `crash-99` sorts before `crash-100`). `None` for any other name.
pub fn report_time(name: &str) -> Option<u64> {
    let n = name.strip_prefix("crash-")?.strip_suffix(".toml")?;
    let t: u64 = n.split('-').next()?.parse().ok()?;
    // 10^11 ms is 1973, 10^11 s is year 5138: below it a number is seconds.
    Some(if t < 100_000_000_000 { t * 1000 } else { t })
}

/// Delete all but the newest [`KEEP`] reports, never `just_written` (a clock that went backwards makes it look
/// oldest, and it is the one the user is told to attach). Best effort: a report that cannot be removed stays.
fn prune(dir: &Path, just_written: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut reports: Vec<(u64, String, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            Some((report_time(&name)?, name, e.path()))
        })
        .filter(|(_, _, p)| p != just_written)
        .collect();
    if reports.len() < KEEP {
        return;
    }
    reports.sort();
    for (_, _, p) in &reports[..reports.len() - (KEEP - 1)] {
        let _ = std::fs::remove_file(p);
    }
}

/// A TOML basic string. Windows paths (`src\main.rs`, `C:\Users\…`) carry backslashes, which are escapes in TOML:
/// written raw they made the whole report unparsable.
fn toml_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Returns the directory where crash reports are stored.
/// ~/Library/Application Support/atlas/crashes on macOS
pub fn crash_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("atlas")
        .join("crashes")
}

#[cfg(test)]
mod tests {
    /// Incident (1.5.0-alpha.24, Windows): panic locations like `src\main.rs` were written raw into a TOML
    /// basic string (where `\` starts an escape), so the crash report could not be parsed. The escapes used
    /// (`\\ \" \n \r \t \uXXXX`) mean the same in JSON, so a JSON parse proves the round trip.
    #[test]
    fn crash_fields_round_trip_as_escaped_strings() {
        for s in [
            r"crates\atlas-desktop\src\main.rs:12:5",
            "say \"hi\"\nnext\tline\r",
            "C:\\Users\\x\u{1}",
        ] {
            let lit = super::toml_str(s);
            let back: String = serde_json::from_str(&lit).unwrap_or_else(|e| panic!("{lit}: {e}"));
            assert_eq!(back, s);
        }
    }

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn files(dir: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    /// Issue #319: the file is what users attach to a public issue, and it carried the raw command line
    /// (`--token …`) and a backtrace full of `/home/<user>/…`. Everything but the version/OS facts goes
    /// through the log's `redact`, and the real home directory collapses to `~`.
    #[test]
    fn a_report_carries_no_token_and_no_home_path() {
        let dir = tempfile::tempdir().unwrap();
        let home = dirs::home_dir().unwrap();
        let home = home.to_string_lossy();
        let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        let path = super::write_report_in(
            dir.path(),
            &format!("boom at {home}/work"),
            &format!("{home}/.cargo/registry/src/x/lib.rs:1:1"),
            &argv(&["atlas", "login", "--token", token, &format!("GITHUB_TOKEN={token}")]),
            &format!("  0: f\n     at {home}/.cargo/registry/src/x/lib.rs:9\n  1: g\n     at /home/other/proj/y.rs:3"),
        )
        .unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        for leak in [token, &*home, "/home/other"] {
            assert!(!text.contains(leak), "{leak} leaked:\n{text}");
        }
        assert!(text.contains("~/.cargo/registry/src/x/lib.rs:9"), "{text}");
        assert!(
            text.contains("atlas login --token"),
            "the command stays readable:\n{text}"
        );
    }

    /// Issue #319: reports were never pruned. The newest 20 stay, the oldest go.
    #[test]
    fn only_the_newest_twenty_reports_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=25u64 {
            std::fs::write(dir.path().join(format!("crash-{}-1.toml", 1_000 + i)), "").unwrap();
        }
        super::write_report_in(dir.path(), "m", "l", &argv(&["a"]), "b").unwrap();
        let names = files(dir.path());
        assert_eq!(names.len(), 20, "{names:?}");
        for gone in 1..=6u64 {
            let old = format!("crash-{}-1.toml", 1_000 + gone);
            assert!(!names.contains(&old), "{old} should be pruned: {names:?}");
        }
        assert!(
            names.contains(&"crash-1025-1.toml".to_string()),
            "{names:?}"
        );
    }

    /// Old reports (`crash-<unix seconds>.toml`) count and prune with the new ones, ordered by time, not
    /// by name: `crash-99.toml` is older than `crash-1700000000000-5.toml`.
    #[test]
    fn old_second_named_reports_are_pruned_as_the_oldest() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20u64 {
            std::fs::write(
                dir.path().join(format!("crash-{}.toml", 1_700_000_000 + i)),
                "",
            )
            .unwrap();
        }
        let new = super::write_report_in(dir.path(), "m", "l", &argv(&["a"]), "b").unwrap();
        let names = files(dir.path());
        assert_eq!(names.len(), 20, "{names:?}");
        assert!(names.contains(&new.file_name().unwrap().to_string_lossy().into_owned()));
        assert!(
            !names.contains(&"crash-1700000000.toml".to_string()),
            "{names:?}"
        );
    }

    /// Issue #319: `crash-<unix seconds>.toml` made two panics in one second overwrite each other.
    #[test]
    fn two_reports_in_the_same_second_are_two_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = super::write_report_in(dir.path(), "first", "l", &argv(&["a"]), "b").unwrap();
        let b = super::write_report_in(dir.path(), "second", "l", &argv(&["a"]), "b").unwrap();
        assert_ne!(a, b);
        assert_eq!(files(dir.path()).len(), 2);
        assert!(std::fs::read_to_string(a).unwrap().contains("first"));
    }

    /// `crash-99.toml` is older than `crash-100.toml`; a sort by name would delete the newer one.
    #[test]
    fn pruning_orders_numerically_not_by_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("crash-99.toml"), "").unwrap();
        for i in 100..119u64 {
            std::fs::write(dir.path().join(format!("crash-{i}.toml")), "").unwrap();
        }
        super::write_report_in(dir.path(), "m", "l", &argv(&["a"]), "b").unwrap();
        let names = files(dir.path());
        assert_eq!(names.len(), 20, "{names:?}");
        assert!(!names.contains(&"crash-99.toml".to_string()), "{names:?}");
        assert!(names.contains(&"crash-100.toml".to_string()), "{names:?}");
    }

    /// A clock that went backwards (or `crash-0`) makes the new report the oldest; it is still the one the
    /// user is told to attach, so it is never the one pruned.
    #[test]
    fn the_report_just_written_is_never_pruned() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20u64 {
            std::fs::write(
                dir.path()
                    .join(format!("crash-{}-1.toml", 9_000_000_000_000 + i)),
                "",
            )
            .unwrap();
        }
        let new = super::write_report_in(dir.path(), "m", "l", &argv(&["a"]), "b").unwrap();
        assert!(new.exists(), "{:?}", files(dir.path()));
        assert_eq!(files(dir.path()).len(), 20);
    }

    /// Threads panicking in the same millisecond (one process, one pid) each get their own file.
    #[test]
    fn concurrent_reports_never_overwrite_each_other() {
        for _ in 0..20 {
            let dir = tempfile::tempdir().unwrap();
            let barrier = std::sync::Barrier::new(16);
            let paths: std::collections::HashSet<_> = std::thread::scope(|s| {
                let hs: Vec<_> = (0..16)
                    .map(|i| {
                        let (dir, barrier) = (dir.path(), &barrier);
                        s.spawn(move || {
                            barrier.wait();
                            super::write_report_in(dir, &format!("m{i}"), "l", &argv(&["a"]), "b")
                                .unwrap()
                        })
                    })
                    .collect();
                hs.into_iter().map(|h| h.join().unwrap()).collect()
            });
            assert_eq!(paths.len(), 16, "two threads shared a report file");
        }
    }
}
