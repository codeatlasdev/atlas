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

/// Service names that have a log file in `log_dir` (current runs only), sorted.
pub fn log_services(log_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(log_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let f = e.file_name().into_string().ok()?;
            let stem = f.strip_suffix(".log")?;
            (!stem.ends_with(".prev") && !stem.starts_with('.')).then(|| stem.to_string())
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

pub fn now_secs_of_day() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().num_seconds_from_midnight()
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

    fn is_match(&self, line: &str) -> bool {
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
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let d = tempfile::TempDir::new().unwrap();
        std::fs::write(
            d.path().join("api.log"),
            "10:00:01 [stdout] boot\n10:00:05 [stderr] ERROR db down\n10:00:09 [stdout] ready\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("web.log"),
            "10:00:03 [stdout] vite up\n10:00:07 [stdout] warn: slow\n",
        )
        .unwrap();
        std::fs::write(
            d.path().join("api.prev.log"),
            "09:00:00 [stdout] old ERROR boom\n",
        )
        .unwrap();
        d
    }

    fn opts(service: Option<&str>, lines: usize) -> TailOpts<'_> {
        TailOpts {
            service,
            lines,
            since_minutes: None,
            level: None,
            now_secs: 10 * 3600 + 60,
        }
    }

    #[test]
    fn action_output_is_readable_by_label_or_stem() {
        let d = fixture();
        std::fs::write(
            d.path().join("action-run-checks.log"),
            "10:00:10 [stdout] checking\n10:00:11 [stderr] ERROR lint failed\n",
        )
        .unwrap();
        for name in ["action:Run checks", "action-run-checks"] {
            let v = tail(d.path(), &opts(Some(name), 10)).unwrap();
            assert_eq!(v.len(), 2, "{name}");
            assert_eq!(v[0].service, "action-run-checks");
        }
        let hits = search(
            d.path(),
            &Matcher::parse("lint").unwrap(),
            Some("action:Run checks"),
            10,
            false,
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        // An action label can never reach outside `.logs/`.
        assert!(resolve_service("action:../../etc/passwd").is_some_and(|s| !s.contains('/')));
        assert!(
            log_services(d.path()).contains(&"action-run-checks".to_string()),
            "merged tail includes action logs"
        );
    }

    #[test]
    fn services_exclude_prev_files() {
        assert_eq!(log_services(fixture().path()), vec!["api", "web"]);
    }

    #[test]
    fn tail_single_service_returns_last_n_in_order() {
        let d = fixture();
        let t = tail(d.path(), &opts(Some("api"), 2)).unwrap();
        let text: Vec<_> = t.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            text,
            vec!["10:00:05 [stderr] ERROR db down", "10:00:09 [stdout] ready"]
        );
        assert_eq!(t[0].line_no, 2);
    }

    #[test]
    fn tail_merges_services_by_timestamp() {
        let d = fixture();
        let t = tail(d.path(), &opts(None, 100)).unwrap();
        let order: Vec<_> = t.iter().map(|l| &l.text[..8]).collect();
        assert_eq!(
            order,
            vec!["10:00:01", "10:00:03", "10:00:05", "10:00:07", "10:00:09"]
        );
        assert_eq!(t[1].service, "web");
    }

    #[test]
    fn tail_level_filters() {
        let d = fixture();
        let mut o = opts(None, 100);
        o.level = Some(MinLevel::Error);
        let t = tail(d.path(), &o).unwrap();
        assert_eq!(t.len(), 1);
        assert!(t[0].text.contains("db down"));
        o.level = Some(MinLevel::Warn);
        assert_eq!(tail(d.path(), &o).unwrap().len(), 2);
    }

    #[test]
    fn tail_since_minutes_uses_time_of_day() {
        let d = fixture();
        let mut o = opts(Some("api"), 100);
        o.now_secs = 10 * 3600 + 10; // 10:00:10
        o.since_minutes = Some(0);
        // 0 minutes: only lines at/after now; none qualify.
        assert!(tail(d.path(), &o).unwrap().is_empty());
        o.since_minutes = Some(1);
        assert_eq!(tail(d.path(), &o).unwrap().len(), 3);
    }

    #[test]
    fn since_handles_midnight_wrap() {
        let l = LogLine {
            service: "a".into(),
            line_no: 1,
            previous: false,
            text: "23:59:30 [stdout] x".into(),
        };
        assert!(within(&l, 30, 120)); // now 00:00:30, 2 min window
        assert!(!within(&l, 30, 30));
    }

    #[test]
    fn tail_unknown_service_lists_available() {
        let d = fixture();
        let e = tail(d.path(), &opts(Some("nope"), 10)).unwrap_err();
        assert!(e.contains("api, web"), "{e}");
    }

    #[test]
    fn service_name_cannot_escape_log_dir() {
        let d = fixture();
        assert!(tail(d.path(), &opts(Some("../etc/passwd"), 10)).is_err());
        let m = Matcher::parse("x").unwrap();
        assert!(search(d.path(), &m, Some("../x"), 10, false).is_err());
    }

    #[test]
    fn search_substring_is_case_insensitive_with_line_numbers() {
        let d = fixture();
        let m = Matcher::parse("error").unwrap();
        let hits = search(d.path(), &m, None, 50, false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].service.as_str(), hits[0].line_no), ("api", 2));
    }

    #[test]
    fn search_unknown_service_lists_known_ones() {
        let d = fixture();
        let m = Matcher::parse("x").unwrap();
        let e = search(d.path(), &m, Some("ghost"), 10, false).unwrap_err();
        assert!(
            e.contains("ghost") && e.contains("api") && e.contains("web"),
            "{e}"
        );
    }

    #[test]
    fn search_regex_and_previous_and_limit() {
        let d = fixture();
        let m = Matcher::parse("/ERROR \\w+/").unwrap();
        let hits = search(d.path(), &m, None, 50, true).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits[0].previous && !hits[1].previous);
        assert_eq!(search(d.path(), &m, None, 1, true).unwrap().len(), 1);
    }

    #[test]
    fn matcher_rejects_empty_and_bad_regex() {
        assert!(Matcher::parse("").is_err());
        assert!(Matcher::parse("/(/").is_err());
        // A lone slash-delimited path-ish query of length 2 is a substring.
        let m = Matcher::parse("//").unwrap();
        assert!(m.is_match("a // b") && !m.is_match("a b"));
    }

    #[test]
    fn long_lines_are_clipped() {
        let l = LogLine {
            service: "a".into(),
            line_no: 1,
            previous: false,
            text: "x".repeat(5000),
        };
        assert!(l.render().len() < 2100);
    }

    #[test]
    fn search_ignores_case_whatever_the_query_case() {
        let d = fixture();
        for q in ["error", "Error", "ERROR"] {
            let hits = search(d.path(), &Matcher::parse(q).unwrap(), None, 50, false).unwrap();
            assert_eq!(hits.len(), 1, "{q}");
        }
    }

    #[test]
    fn search_keeps_only_the_newest_limit_hits_in_order() {
        let d = tempfile::TempDir::new().unwrap();
        let body: String = (0..1000)
            .map(|i| format!("10:00:00 [stdout] hit {i}\n10:00:00 [stdout] miss {i}\n"))
            .collect();
        std::fs::write(d.path().join("api.log"), body).unwrap();
        let hits = search(d.path(), &Matcher::parse("hit").unwrap(), None, 3, false).unwrap();
        let nos: Vec<_> = hits.iter().map(|h| h.line_no).collect();
        assert_eq!(
            nos,
            [1995, 1997, 1999],
            "newest three, oldest first, true line numbers"
        );
        assert!(hits[2].text.ends_with("hit 999"));
    }

    #[test]
    fn crlf_and_invalid_utf8_lines_are_searchable() {
        let d = tempfile::TempDir::new().unwrap();
        std::fs::write(
            d.path().join("api.log"),
            b"10:00:00 [stdout] ok \xff needle\r\n10:00:01 [stdout] x\n",
        )
        .unwrap();
        let hits = search(d.path(), &Matcher::parse("needle").unwrap(), None, 5, false).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].text.ends_with("needle"), "{:?}", hits[0].text);
    }

    #[test]
    fn huge_log_tail_reads_only_the_window_but_keeps_line_numbers() {
        let d = tempfile::TempDir::new().unwrap();
        let line = format!("10:00:00 [stdout] {}\n", "x".repeat(80));
        let n = (TAIL_WINDOW_BYTES as usize / line.len()) + 50_000;
        let mut body = line.repeat(n);
        body.push_str("10:00:01 [stdout] LAST\n");
        std::fs::write(d.path().join("api.log"), &body).unwrap();
        let t = tail(d.path(), &opts(Some("api"), 2)).unwrap();
        assert!(t[1].text.ends_with("LAST"));
        assert_eq!(
            t[1].line_no,
            n + 1,
            "true file line number, not window-relative"
        );
        assert_eq!(t[0].line_no, n);
        // The read itself is bounded.
        let all = read_lines(d.path(), "api", false);
        assert!((all.len() as u64) * (line.len() as u64) <= TAIL_WINDOW_BYTES + line.len() as u64);
    }
}
