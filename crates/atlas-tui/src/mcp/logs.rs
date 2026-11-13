//! Disk-log readers for the MCP read tools. Files are `.logs/<svc>.log`
//! (previous run: `<svc>.prev.log`), lines `HH:MM:SS [stream] text`, ANSI already stripped.

use std::path::Path;

use crate::runtime::logs::LogLevel;

pub const DEFAULT_TAIL: usize = 100;
pub const MAX_TAIL: usize = 2000;
pub const DEFAULT_SEARCH_LIMIT: usize = 50;
pub const MAX_SEARCH_LIMIT: usize = 500;
/// A single line is clipped to this many chars so one minified blob can't flood the reply.
const MAX_LINE_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinLevel {
    Warn,
    Error,
}

impl MinLevel {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "warn" | "warning" => Ok(Self::Warn),
            "error" | "err" => Ok(Self::Error),
            other => Err(format!("invalid level '{other}' (use error or warn)")),
        }
    }

    fn allows(self, line: &str) -> bool {
        // Skip the clock/stream prefix so `[stderr]` can never read as a level.
        let body = line.splitn(3, ' ').nth(2).unwrap_or(line);
        matches!(
            (self, LogLevel::detect(body)),
            (Self::Warn, LogLevel::Warn | LogLevel::Error) | (Self::Error, LogLevel::Error)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub service: String,
    /// 1-based line number within the file.
    pub line_no: usize,
    pub previous: bool,
    pub text: String,
}

impl LogLine {
    pub fn render(&self) -> String {
        let prev = if self.previous { " (prev run)" } else { "" };
        format!(
            "[{}{prev}:{}] {}",
            self.service,
            self.line_no,
            clip(&self.text)
        )
    }

    /// Seconds-of-day parsed from the `HH:MM:SS` prefix, if present.
    fn secs_of_day(&self) -> Option<u32> {
        let t = self.text.get(..8)?;
        let b = t.as_bytes();
        if b[2] != b':' || b[5] != b':' {
            return None;
        }
        let n = |r: std::ops::Range<usize>| t.get(r)?.parse::<u32>().ok();
        let (h, m, s) = (n(0..2)?, n(3..5)?, n(6..8)?);
        (h < 24 && m < 60 && s < 60).then_some(h * 3600 + m * 60 + s)
    }
}

fn clip(s: &str) -> String {
    match s.char_indices().nth(MAX_LINE_CHARS) {
        Some((i, _)) => format!("{}… [clipped]", &s[..i]),
        None => s.to_string(),
    }
}

/// Service names that have a log file in `log_dir` (current runs only), sorted. The audit
/// trail (`atlas-audit.log`) is not a service: it is read as `service: "audit"`.
pub fn log_services(log_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(log_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let f = e.file_name().into_string().ok()?;
            let stem = f.strip_suffix(".log")?;
            (!stem.ends_with(".prev")
                && !stem.starts_with('.')
                && stem != crate::control::audit::AUDIT_LOG)
                .then(|| stem.to_string())
        })
        .collect();
    names.sort();
    names
}

/// Reject names that could escape `.logs/` (the service name becomes a file name).
pub fn valid_service_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != ".."
}

/// Resolve a user-facing log name to its `.logs/` file stem: `action:<label>`
/// (as shown in the All tab) becomes `action-<slug>`; anything else is a stem
/// already. Returns None for names that could escape `.logs/`.
pub fn resolve_service(name: &str) -> Option<String> {
    let stem = crate::runtime::manager::log_file_stem(name);
    valid_service_name(&stem).then_some(stem)
}

/// Largest slice of one log file a tail read looks at (from the end).
const TAIL_WINDOW_BYTES: u64 = 8 * 1024 * 1024;

fn log_path(log_dir: &Path, service: &str, previous: bool) -> std::path::PathBuf {
    log_dir.join(if previous {
        format!("{service}.prev.log")
    } else {
        format!("{service}.log")
    })
}

/// Lines of one log file, 1-based numbers preserved. Files larger than
/// `TAIL_WINDOW_BYTES` are read from the end only (the first, possibly cut,
/// line of the window is dropped), so a 50 MiB log costs 8 MiB, not 50.
fn read_lines(log_dir: &Path, service: &str, previous: bool) -> Vec<LogLine> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(log_path(log_dir, service, previous)) else {
        return Vec::new();
    };
    let len = f.metadata().map_or(0, |m| m.len());
    let mut skipped_lines = 0usize;
    let mut bytes = Vec::new();
    if len > TAIL_WINDOW_BYTES {
        // Count the newlines we skip so line numbers stay true to the file.
        let mut buf = vec![0u8; 1 << 20];
        let mut left = len - TAIL_WINDOW_BYTES;
        while left > 0 {
            let n = buf.len().min(left as usize);
            if f.read_exact(&mut buf[..n]).is_err() {
                break;
            }
            skipped_lines += buf[..n].iter().filter(|&&b| b == b'\n').count();
            left -= n as u64;
        }
        let _ = f.seek(SeekFrom::Start(len - TAIL_WINDOW_BYTES));
        if f.read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        // Drop the cut first line (it counted as a skipped newline only when complete).
        if let Some(nl) = bytes.iter().position(|&b| b == b'\n') {
            bytes.drain(..=nl);
            skipped_lines += 1;
        }
    } else if f.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    String::from_utf8_lossy(&bytes)
        .lines()
        .enumerate()
        .map(|(i, l)| LogLine {
            service: service.to_string(),
            line_no: skipped_lines + i + 1,
            previous,
            text: l.to_string(),
        })
        .collect()
}

/// Stream `file`'s lines through `keep`, holding only the newest `limit`
/// matches (a ring), so searching a huge log never materialises it.
fn stream_matches(
    log_dir: &Path,
    service: &str,
    previous: bool,
    limit: usize,
    keep: &dyn Fn(&str) -> bool,
    out: &mut std::collections::VecDeque<LogLine>,
) {
    use std::io::BufRead;
    let Ok(f) = std::fs::File::open(log_path(log_dir, service, previous)) else {
        return;
    };
    let mut reader = std::io::BufReader::with_capacity(256 * 1024, f);
    let mut buf = Vec::new();
    let mut line_no = 0usize;
    loop {
        buf.clear();
        // A line is bounded: anything past 1 MiB is read and discarded in place.
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        line_no += 1;
        if buf.last() == Some(&b'\n') {
            buf.pop();
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
        }
        let text = String::from_utf8_lossy(&buf);
        if keep(&text) {
            if out.len() == limit {
                out.pop_front();
            }
            out.push_back(LogLine {
                service: service.to_string(),
                line_no,
                previous,
                text: text.into_owned(),
            });
        }
    }
}

/// Merge per-service line lists by `HH:MM:SS` prefix. The sort is stable, so
/// ties and un-prefixed lines keep their original per-file order.
fn merge_by_time(mut lines: Vec<LogLine>) -> Vec<LogLine> {
    // Lines without a timestamp inherit the previous line's so they stay attached.
    let mut last = 0u32;
    let keys: Vec<u32> = lines
        .iter()
        .map(|l| {
            if let Some(s) = l.secs_of_day() {
                last = s;
            }
            last
        })
        .collect();
    let mut idx: Vec<usize> = (0..lines.len()).collect();
    // Per-file order is chronological except across midnight; only reorder
    // across files, never within one (compare keys only for different services).
    idx.sort_by(|&a, &b| {
        if lines[a].service == lines[b].service {
            a.cmp(&b)
        } else {
            keys[a].cmp(&keys[b])
        }
    });
    let mut out: Vec<Option<LogLine>> = lines.drain(..).map(Some).collect();
    idx.into_iter().filter_map(|i| out[i].take()).collect()
}

/// Wall-clock cutoff test for `since_minutes` (lines carry only time-of-day).
fn within(line: &LogLine, now_secs: u32, since_secs: u32) -> bool {
    match line.secs_of_day() {
        // Un-timestamped lines (stack-trace continuations) are kept; the tail window bounds them.
        None => true,
        Some(t) => {
            // t > now means the line is from before midnight.
            let age = if t <= now_secs {
                now_secs - t
            } else {
                now_secs + 86_400 - t
            };
            age <= since_secs
        }
    }
}

pub struct TailOpts<'a> {
    pub service: Option<&'a str>,
    pub lines: usize,
    pub since_minutes: Option<u64>,
    pub level: Option<MinLevel>,
    pub now_secs: u32,
}

/// Services to read: the named one (must have a log file; `.prev.log` counts when
/// `allow_prev`) or every service with a log.
fn services_for(
    log_dir: &Path,
    service: Option<&str>,
    allow_prev: bool,
) -> Result<Vec<String>, String> {
    let Some(s) = service else {
        return Ok(log_services(log_dir));
    };
    let Some(stem) = resolve_service(s) else {
        return Err(format!("invalid service name '{s}'"));
    };
    let exists = |ext: &str| log_dir.join(format!("{stem}.{ext}")).exists();
    if !exists("log") && !(allow_prev && exists("prev.log")) {
        return Err(format!(
            "no log file for service '{s}'. Services with logs: {}",
            log_services(log_dir).join(", ")
        ));
    }
    Ok(vec![stem])
}

pub fn tail(log_dir: &Path, o: &TailOpts) -> Result<Vec<LogLine>, String> {
    let services = services_for(log_dir, o.service, false)?;
    let mut all: Vec<LogLine> = Vec::new();
    for s in &services {
        let mut v = read_lines(log_dir, s, false);
        // Bound memory/work per service: nothing older than the last `lines` can survive the final tail.
        if o.level.is_none() && o.since_minutes.is_none() && v.len() > o.lines {
            v.drain(..v.len() - o.lines);
        }
        all.extend(v);
    }
    let mut merged = if services.len() > 1 {
        merge_by_time(all)
    } else {
        all
    };
    if let Some(m) = o.since_minutes {
        let secs = (m.min(24 * 60) * 60) as u32;
        merged.retain(|l| within(l, o.now_secs, secs));
    }
    if let Some(level) = o.level {
        merged.retain(|l| level.allows(&l.text));
    }
    let start = merged.len().saturating_sub(o.lines);
    Ok(merged.split_off(start))
}

/// A search query: `/pattern/` is a regex, anything else a literal; both are
/// case-insensitive (the tool's documented contract for agents).
pub struct Matcher(crate::runtime::logindex::Matcher);

impl Matcher {
    pub fn parse(query: &str) -> Result<Self, String> {
        if query.is_empty() {
            return Err("query must not be empty".into());
        }
        let (q, regex) = if query.len() > 2 && query.starts_with('/') && query.ends_with('/') {
            (&query[1..query.len() - 1], true)
        } else {
            (query, false)
        };
        crate::runtime::logindex::Matcher::compile_insensitive(q, regex).map(Self)
    }

    pub(super) fn is_match(&self, line: &str) -> bool {
        self.0.is_match(line)
    }
}

pub fn search(
    log_dir: &Path,
    matcher: &Matcher,
    service: Option<&str>,
    limit: usize,
    include_previous: bool,
) -> Result<Vec<LogLine>, String> {
    let services = services_for(log_dir, service, true)?;
    // Most recent matches matter most: a ring of the last `limit` hits.
    let mut hits = std::collections::VecDeque::with_capacity(limit.min(1024));
    for s in &services {
        if include_previous {
            stream_matches(log_dir, s, true, limit, &|l| matcher.is_match(l), &mut hits);
        }
        stream_matches(
            log_dir,
            s,
            false,
            limit,
            &|l| matcher.is_match(l),
            &mut hits,
        );
    }
    Ok(hits.into())
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
