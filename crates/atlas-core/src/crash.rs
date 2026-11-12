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
    // `capture()` records nothing unless RUST_BACKTRACE is set, and an installed app never has it.
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    let args = std::env::args().collect::<Vec<_>>().join(" ");

    let report = format!(
        "version = \"{version}\"\nos = \"{os}\"\narch = \"{arch}\"\ntimestamp = {ts}\nargs = {args}\nmsg = {msg}\nlocation = {location}\n\nbacktrace = '''\n{backtrace}\n'''\n",
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        msg = toml_str(msg),
        args = toml_str(&args),
        location = toml_str(location),
        backtrace = backtrace.replace("'''", "' ' '"),
    );

    std::fs::write(&path, report).ok()?;
    Some(path)
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
}
