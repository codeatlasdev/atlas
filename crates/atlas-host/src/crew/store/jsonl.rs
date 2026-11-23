//! How every crew log file (`tasks.jsonl`, `usage.jsonl`, `audit.jsonl`, `<trigger>_seen.jsonl`) is read
//! and appended so a crash or a bad byte costs one record, never the rest of the file (#293).
//!
//! - Readers go line by line with `read_until(b'\n')` and decode lossily: a line that is not UTF-8 is at
//!   worst one record with a replacement character, and the lines after it are still read.
//! - A writer first checks that the file ends in a newline. A torn final line (a crash mid-append) is
//!   terminated, so it stays one unreadable line and the next record is not glued onto it.

use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::paths;

/// One line of a log file, as read.
pub(crate) struct Line {
    /// The line without its `\n` (or `\r\n`), decoded lossily.
    pub text: String,
    /// Bytes the line took in the file, newline included.
    pub bytes: usize,
    /// Whether the line ended with `\n`: only the last line of a file can be torn.
    pub terminated: bool,
}

/// The lines of `r`, newest last. An I/O error ends the iteration (it is not bad data).
pub(crate) fn lines<R: BufRead>(mut r: R) -> impl Iterator<Item = Line> {
    let mut buf = Vec::new();
    std::iter::from_fn(move || {
        buf.clear();
        match r.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => None,
            Ok(n) => {
                let terminated = buf.last() == Some(&b'\n');
                if terminated {
                    buf.pop();
                    if buf.last() == Some(&b'\r') {
                        buf.pop();
                    }
                }
                Some(Line {
                    text: String::from_utf8_lossy(&buf).into_owned(),
                    bytes: n,
                    terminated,
                })
            }
        }
    })
}

/// The text of the lines of the file at `path`; a missing or unreadable file has none.
pub(crate) fn read_lines(path: &Path) -> Vec<String> {
    let Ok(f) = std::fs::File::open(path) else {
        return vec![];
    };
    lines(io::BufReader::new(f)).map(|l| l.text).collect()
}

/// Append `line` (a newline is added) to `path`, creating it, after terminating a torn final line.
/// `durable`: flush the data to disk before returning (a state change must survive a power cut).
/// Returns the file's length afterwards.
pub(crate) fn append_line(path: &Path, line: &str, durable: bool) -> io::Result<u64> {
    append_lines(path, &[line], durable)
}

/// [`append_line`] for several lines: one open, one write, one flush (the file is created even for none). A
/// poll that records a hundred ids must not open the file a hundred times.
pub(crate) fn append_lines(path: &Path, lines: &[&str], durable: bool) -> io::Result<u64> {
    let mut f = atlas_core::sys::owner_only(
        std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .append(true),
    )
    .open(path)?;
    let len = f.metadata()?.len();
    if lines.is_empty() {
        return Ok(len);
    }
    let mut buf = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum::<usize>() + 1);
    if len > 0 {
        let mut last = [0u8; 1];
        f.seek(SeekFrom::End(-1))?;
        f.read_exact(&mut last)?;
        if last[0] != b'\n' {
            buf.push('\n');
        }
    }
    for line in lines {
        buf.push_str(line);
        buf.push('\n');
    }
    // One write: with O_APPEND it lands at the end whatever the seek above did.
    f.write_all(buf.as_bytes())?;
    if durable {
        f.sync_data()?;
    }
    Ok(len + buf.len() as u64)
}

/// The last `max_bytes` of the file at `path`, decoded lossily (all of it when it is shorter).
pub(crate) fn read_tail(path: &Path, max_bytes: u64) -> io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(max_bytes)))?;
    let mut buf = Vec::new();
    f.take(max_bytes).read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Cut the file at `path` back to its newest whole lines (about `keep_bytes`) once it is longer than
/// `max_bytes`. Atomic (`paths::write_private`): a reader sees the old file or the new one. `Ok(true)`
/// when it was cut.
pub(crate) fn trim_to_tail(path: &Path, max_bytes: u64, keep_bytes: u64) -> io::Result<bool> {
    if std::fs::metadata(path)?.len() <= max_bytes {
        return Ok(false);
    }
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let start = len.saturating_sub(keep_bytes);
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    f.take(keep_bytes).read_to_end(&mut buf)?;
    // Started mid-line: the first piece is a fragment, drop it.
    let from = if start > 0 {
        buf.iter()
            .position(|&b| b == b'\n')
            .map_or(buf.len(), |i| i + 1)
    } else {
        0
    };
    paths::write_private(path, &buf[from..])?;
    Ok(true)
}
