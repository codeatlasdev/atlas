use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::runtime::clock::{HmsCache, hms, now_ms};
use crate::runtime::logs::LogStream;
use crate::runtime::text::{has_control_bytes, visible_chars};

/// Rotate `<name>.log` → `<name>.prev.log` and return a fresh append-mode handle.
/// Returns None when the file can't be created — disk logs are best-effort.
pub(super) fn rotate_and_open(log_dir: &Path, name: &str) -> Option<File> {
    let _ = std::fs::create_dir_all(log_dir);
    let path = log_dir.join(format!("{name}.log"));
    let _ = std::fs::rename(&path, log_dir.join(format!("{name}.prev.log")));
    open_append(&path)
}

/// Append-mode handle to the current `<name>.log` (shared by stdout and stderr
/// readers; O_APPEND keeps whole-line writes from interleaving mid-line).
pub(super) fn open_append(path: &Path) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// Rotate `<name>.log` and return the stdout/stderr writers of the new run: each
/// pump owns its own append handle, so no lock is shared across tasks or services.
pub(crate) fn open_service_writers(
    log_dir: &Path,
    name: &str,
) -> (Option<LogWriter>, Option<LogWriter>) {
    let out = rotate_and_open(log_dir, name).map(|f| LogWriter::new(log_dir, name, f));
    let err =
        open_append(&log_dir.join(format!("{name}.log"))).map(|f| LogWriter::new(log_dir, name, f));
    (out, err)
}

/// Rotate `<name>.log` once per atlas run (tunnel: reconnects then append).
pub(crate) fn rotate_stream_log(log_dir: &Path, name: &str) {
    let _ = rotate_and_open(log_dir, name);
}

/// Rotating writer appending to `<name>.log`; used by the tunnel's stderr pump.
pub(crate) fn open_stream_log(log_dir: &Path, name: &str) -> Option<LogWriter> {
    let _ = std::fs::create_dir_all(log_dir);
    open_append(&log_dir.join(format!("{name}.log"))).map(|f| LogWriter::new(log_dir, name, f))
}

/// Prefix of the log-buffer service name for a project action's output.
pub(crate) const ACTION_PREFIX: &str = "action:";

/// Log-buffer service name for an action: `action:<label>`.
pub(crate) fn action_service_name(label: &str) -> String {
    format!("{ACTION_PREFIX}{label}")
}

/// `.logs/` file stem for a service name. Plain services map to themselves;
/// `action:<label>` maps to `action-<slug>` (label lowercased, every run of
/// non-alphanumerics collapsed to one `-`), so no label can escape `.logs/`.
pub(crate) fn log_file_stem(service: &str) -> String {
    let Some(label) = service.strip_prefix(ACTION_PREFIX) else {
        return service.to_string();
    };
    let mut slug = String::new();
    for c in label.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    format!("action-{}", if slug.is_empty() { "unnamed" } else { slug })
}

/// Append one `HH:MM:SS [stdout] line` entry to the service's log file, no
/// rotation. Best-effort: used for user marks and one-off notes.
pub(crate) fn append_mark(log_dir: &Path, service: &str, line: &str) {
    let _ = std::fs::create_dir_all(log_dir);
    let path = log_dir.join(format!("{}.log", log_file_stem(service)));
    if let Some(mut f) = open_append(&path) {
        write_line(&mut f, LogStream::Stdout, line);
    }
}

/// Rotate the current log once it exceeds this many bytes.
const MAX_LOG_BYTES: u64 = 50 * 1024 * 1024;
/// Check the file every this many lines (a `metadata` call per line is waste).
const SIZE_CHECK_EVERY: u32 = 256;

/// Per-reader handle to `<name>.log` that rotates to `<name>.prev.log` when the
/// file passes `max_bytes`. stdout and stderr readers each own one; whichever
/// notices first renames, and the other notices its handle's inode no longer
/// matches the path and reopens.
pub(crate) struct LogWriter {
    dir: std::path::PathBuf,
    name: String,
    file: File,
    lines: u32,
    max_bytes: u64,
    /// Formatting local time per line is the hot spot at 100k lines/s.
    clock: HmsCache,
    /// Reused line buffer (one allocation for the writer's lifetime).
    scratch: String,
}

impl LogWriter {
    pub(crate) fn new(dir: &Path, name: &str, file: File) -> Self {
        Self::with_limit(dir, name, file, MAX_LOG_BYTES)
    }

    fn with_limit(dir: &Path, name: &str, file: File, max_bytes: u64) -> Self {
        Self {
            dir: dir.to_path_buf(),
            name: name.to_string(),
            file,
            lines: 0,
            max_bytes,
            clock: HmsCache::default(),
            scratch: String::new(),
        }
    }

    pub(crate) fn write_line(&mut self, stream: LogStream, line: &str) {
        self.lines += 1;
        if self.lines.is_multiple_of(SIZE_CHECK_EVERY) {
            self.rotate_if_needed();
        }
        let clock = self.clock.format(now_ms());
        compose_line(&mut self.scratch, clock, stream, line);
        let _ = self.file.write_all(self.scratch.as_bytes());
    }

    fn rotate_if_needed(&mut self) {
        use atlas_core::sys::file_identity;
        let path = self.dir.join(format!("{}.log", self.name));
        let Ok(mine) = self.file.metadata() else {
            return;
        };
        let on_disk = std::fs::File::open(&path).ok();
        // The sibling reader (or a restart) already replaced the file under our
        // handle: just follow the new one, whatever its size.
        let stale = on_disk
            .as_ref()
            .is_none_or(|f| file_identity(f) != file_identity(&self.file));
        if !stale && mine.len() < self.max_bytes {
            return;
        }
        if !stale {
            let _ = std::fs::rename(&path, self.dir.join(format!("{}.prev.log", self.name)));
        }
        if let Some(f) = open_append(&path) {
            self.file = f;
        }
    }
}

/// Build one ANSI-stripped `HH:MM:SS [stream] line\n` entry in `out` (reused),
/// so it goes to disk with a single syscall.
fn compose_line(out: &mut String, clock: &str, stream: LogStream, line: &str) {
    let tag = match stream {
        LogStream::Stdout => "stdout",
        LogStream::Stderr => "stderr",
    };
    out.clear();
    out.push_str(clock);
    out.push_str(" [");
    out.push_str(tag);
    out.push_str("] ");
    // Plain lines (no ESC/control bytes) skip the stripping pass entirely.
    if has_control_bytes(line) {
        out.extend(visible_chars(line));
    } else {
        out.push_str(line);
    }
    out.push('\n');
}

/// One-shot write used by marks and tests.
fn write_line(file: &mut File, stream: LogStream, line: &str) {
    let mut out = String::new();
    compose_line(&mut out, &hms(now_ms() as i64), stream, line);
    let _ = file.write_all(out.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_previous_log_and_strips_ansi() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("api.log"), "old\n").unwrap();

        let mut f = rotate_and_open(dir.path(), "api").unwrap();
        write_line(&mut f, LogStream::Stderr, "\x1b[31mboom\x1b[0m");

        assert_eq!(
            std::fs::read_to_string(dir.path().join("api.prev.log")).unwrap(),
            "old\n"
        );
        let cur = std::fs::read_to_string(dir.path().join("api.log")).unwrap();
        assert!(cur.ends_with(" [stderr] boom\n"), "{cur:?}");
        assert_eq!(cur.as_bytes()[2], b':');
    }

    #[test]
    fn rotates_when_over_limit_and_sibling_follows() {
        let dir = tempfile::tempdir().unwrap();
        let f = rotate_and_open(dir.path(), "api").unwrap();
        let f2 = open_append(&dir.path().join("api.log")).unwrap();
        // Tiny limit so a handful of lines crosses it.
        let mut out = LogWriter::with_limit(dir.path(), "api", f, 200);
        let mut err = LogWriter::with_limit(dir.path(), "api", f2, 200);

        for i in 0..(SIZE_CHECK_EVERY * 3) {
            out.write_line(LogStream::Stdout, &format!("out line {i}"));
        }
        let prev = dir.path().join("api.prev.log");
        assert!(prev.exists(), "stdout writer must rotate");

        // The sibling's handle still points at the rotated inode; once it
        // checks the size it must move to the fresh api.log, not keep writing
        // into .prev.
        let prev_len = std::fs::metadata(&prev).unwrap().len();
        for i in 0..SIZE_CHECK_EVERY {
            err.write_line(LogStream::Stderr, &format!("err line {i}"));
        }
        let cur = std::fs::read_to_string(dir.path().join("api.log")).unwrap();
        assert!(
            cur.contains("err line"),
            "sibling follows rotation: {cur:?}"
        );
        // .prev grew by at most the lines written before the sibling's first check.
        assert!(std::fs::metadata(&prev).unwrap().len() >= prev_len);
        let cur_len = std::fs::metadata(dir.path().join("api.log")).unwrap().len();
        assert!(cur_len < 200 * 20, "current log stays small: {cur_len}");
    }
}
