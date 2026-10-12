use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let dir = crash_dir();
    std::fs::create_dir_all(&dir).ok()?;

    let path = dir.join(format!("crash-{ts}.toml"));
    let backtrace = std::backtrace::Backtrace::capture().to_string();
    let args = std::env::args().collect::<Vec<_>>().join(" ");

    let report = format!(
        "version = \"{version}\"\nos = \"{os}\"\narch = \"{arch}\"\ntimestamp = {ts}\nargs = \"{args}\"\nmsg = \"{msg}\"\nlocation = \"{location}\"\n\nbacktrace = '''\n{backtrace}\n'''\n",
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        msg = msg.replace('"', "'"),
        args = args.replace('"', "'"),
    );

    std::fs::write(&path, report).ok()?;
    Some(path)
}

/// Returns the directory where crash reports are stored.
/// ~/Library/Application Support/atlas/crashes on macOS
pub fn crash_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("atlas")
        .join("crashes")
}
