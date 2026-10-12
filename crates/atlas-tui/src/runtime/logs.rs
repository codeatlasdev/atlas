use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

/// How long a partial line may sit before it is shown anyway (prompts and
/// progress output that never send a newline).
const PARTIAL_FLUSH_AFTER: std::time::Duration = std::time::Duration::from_millis(150);

/// Read `reader` on its own task, handing each assembled line to `on_line`
/// (see `linebuf::LineAssembler`: partial lines, CRLF / `\r` progress bars,
/// lossy UTF-8 and the per-line cap are handled there).
/// The handle lets the owner abort the pump when the process it reads is gone.
pub(crate) fn spawn_line_pump<R>(
    mut reader: R,
    mut on_line: impl FnMut(String) + Send + 'static,
) -> tokio::task::JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::AsyncReadExt;
    tokio::spawn(async move {
        let mut asm = super::linebuf::LineAssembler::new();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            // Waiting on a partial line: give it a moment, then show it as is.
            let read = if asm.has_partial() {
                match tokio::time::timeout(PARTIAL_FLUSH_AFTER, reader.read(&mut buf)).await {
                    Ok(r) => r,
                    Err(_) => {
                        asm.flush(|l| on_line(l.text));
                        continue;
                    }
                }
            } else {
                reader.read(&mut buf).await
            };
            match read {
                Ok(0) | Err(_) => break,
                Ok(n) => asm.push(&buf[..n], |l| on_line(l.text)),
            }
        }
        // EOF: a final line without a newline still counts.
        asm.flush(|l| on_line(l.text));
    })
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Global arrival order across all services (merge key for the All tab).
    pub seq: u64,
    pub timestamp: u64,
    pub service: String,
    pub stream: LogStream,
    pub content: String,
    pub level: LogLevel,
    /// Pre-formatted "HH:MM:SS" — computed once at push, free on every render.
    pub time_str: String,
    /// `-` separator line inserted by the user (never counted as output/errors).
    pub mark: bool,
    /// `── restarted ──` separator inserted when a service (re)starts. Like a
    /// mark it is never output; unlike a mark it is not user-made.
    pub restart: bool,
    /// The line was cut at `linebuf::MAX_LINE_BYTES`.
    pub truncated: bool,
}

impl LogEntry {
    /// Marks and restart separators: drawn as rules, never searched/counted.
    pub fn is_separator(&self) -> bool {
        self.mark || self.restart
    }

    /// Heap bytes this entry holds (what the ring's byte budget counts).
    fn cost(&self) -> usize {
        std::mem::size_of::<Self>() + self.content.len() + self.service.len() + self.time_str.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

static LEVEL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(error|err|fatal|panic|warn|warning|trace|debug)\b").expect("level regex")
});

impl LogLevel {
    /// Lowercase wire name (`debug|info|warn|error`).
    pub fn wire(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    /// Severity order: Debug < Info < Warn < Error.
    pub fn rank(self) -> u8 {
        match self {
            Self::Debug => 0,
            Self::Info => 1,
            Self::Warn => 2,
            Self::Error => 3,
        }
    }

    /// Classify a line by whole-word level tokens in its visible text
    /// (`"0 errors found"` and URLs like `/debug` stay `Info`).
    pub fn detect(content: &str) -> Self {
        // Fast path: the vast majority of lines carry no level word at all.
        if !may_contain_level(content.as_bytes()) {
            return Self::Info;
        }
        let plain: std::borrow::Cow<'_, str> =
            if content.bytes().any(|b| b < 0x20 || b == 0x7f || b >= 0x80) {
                strip_ansi(content).into()
            } else {
                content.into()
            };
        let mut level = Self::Info;
        for m in LEVEL_RE.find_iter(&plain) {
            // `/debug` or `.error` inside a path/URL/identifier is not a level token.
            if matches!(
                plain[..m.start()].chars().next_back(),
                Some('/' | '.' | '-' | '_')
            ) {
                continue;
            }
            let found = match m.as_str().to_ascii_lowercase().as_str() {
                "error" | "err" | "fatal" | "panic" => Self::Error,
                "warn" | "warning" => Self::Warn,
                _ => Self::Debug,
            };
            // Severity wins over Debug: Error > Warn > Debug > Info.
            let rank = |l: Self| match l {
                Self::Error => 3,
                Self::Warn => 2,
                Self::Debug => 1,
                Self::Info => 0,
            };
            if rank(found) > rank(level) {
                level = found;
            }
        }
        level
    }
}

/// Could `bytes` contain one of the level words (`err`, `fatal`, `panic`,
/// `warn`, `trace`, `debug`) in any case? A cheap 3-byte window test that lets
/// `detect` skip the regex for ordinary lines. False positives are fine.
fn may_contain_level(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|w| {
        let l = [
            w[0].to_ascii_lowercase(),
            w[1].to_ascii_lowercase(),
            w[2].to_ascii_lowercase(),
        ];
        matches!(&l, b"err" | b"fat" | b"pan" | b"war" | b"tra" | b"deb")
    })
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warn => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

/// Evictions remembered for indexes that sync between frames; a reader that
/// fell further behind rebuilds.
const EVICTION_LOG: usize = 65_536;

#[cfg(test)]
pub(crate) const EVICTION_LOG_FOR_TESTS: usize = EVICTION_LOG;

/// Default per-service byte budget (entry payload + bookkeeping).
pub const DEFAULT_RING_BYTES: usize = 8 * 1024 * 1024;

/// One service's ring plus its running byte cost.
#[derive(Default)]
struct Ring {
    entries: VecDeque<LogEntry>,
    bytes: usize,
}

impl Ring {
    /// Append `e`, evicting from the front past the line/byte budget; every
    /// evicted entry's `seq` goes to `evicted`.
    fn push(
        &mut self,
        e: LogEntry,
        max_lines: usize,
        max_bytes: usize,
        mut evicted: impl FnMut(u64),
    ) {
        self.bytes += e.cost();
        self.entries.push_back(e);
        // Always keep the newest entry, however big.
        while self.entries.len() > 1 && (self.entries.len() > max_lines || self.bytes > max_bytes) {
            if let Some(old) = self.entries.pop_front() {
                self.bytes -= old.cost();
                evicted(old.seq);
            }
        }
    }
}

/// Caches the last formatted `HH:MM:SS`: chrono's local-time formatting is far
/// too slow to run per line at 100k lines/s, and it only changes once a second.
#[derive(Default)]
struct TimeCache {
    sec: Option<u64>,
    text: String,
}

impl TimeCache {
    fn parts(&mut self, ts_ms: u64) -> (u64, String) {
        let sec = ts_ms / 1000;
        if self.sec != Some(sec) {
            self.text = hms(ts_ms as i64);
            self.sec = Some(sec);
        }
        (ts_ms, self.text.clone())
    }
}

/// Per-service ring buffers: a chatty service can only evict its own history.
/// `capacity` is lines per service, `max_bytes` bytes per service (whichever
/// is hit first evicts the oldest); `all()` merges by arrival sequence number.
/// Every line gets a monotonic `seq`, its level and its timestamp exactly once,
/// at ingest.
pub struct LogBuffer {
    rings: HashMap<String, Ring>,
    /// `seq` of the newest restart separator per service. Survives the marker
    /// being evicted from its ring, so "since restart" stays correct.
    last_restart: HashMap<String, u64>,
    /// `seq` of recently evicted entries, oldest first; `evicted_total` counts
    /// every eviction ever, so a reader remembers "how many I have seen".
    evictions: VecDeque<u64>,
    evicted_total: u64,
    capacity: usize,
    max_bytes: usize,
    next_seq: u64,
    /// Bumped by anything that invalidates indexes built over the buffer
    /// without being an append (`clear`).
    generation: u64,
    time: TimeCache,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self::with_budget(capacity, DEFAULT_RING_BYTES)
    }

    pub fn with_budget(capacity: usize, max_bytes: usize) -> Self {
        Self {
            rings: HashMap::new(),
            last_restart: HashMap::new(),
            evictions: VecDeque::new(),
            evicted_total: 0,
            capacity,
            max_bytes,
            next_seq: 0,
            generation: 0,
            time: TimeCache::default(),
        }
    }

    pub fn push(&mut self, service: String, content: String, stream: LogStream) -> &LogEntry {
        let kind = if content.ends_with(super::linebuf::TRUNCATION_MARK) {
            Kind::Truncated
        } else {
            Kind::Line
        };
        self.push_entry(service, content, stream, kind, None)
    }

    /// Like `push`, for a line that happened earlier (an attached client's backlog):
    /// keeps the original epoch-millis time instead of stamping "now".
    pub fn push_at(&mut self, service: String, content: String, stream: LogStream, ts_ms: u64) {
        self.push_entry(service, content, stream, Kind::Line, Some(ts_ms));
    }

    /// Ingest with the wire `kind` of a replayed/streamed line: `"restart"`
    /// and `"mark"` become separators, anything else a normal line.
    pub fn push_kind(
        &mut self,
        service: String,
        content: String,
        stream: LogStream,
        kind: &str,
        ts_ms: Option<u64>,
    ) {
        let kind = match kind {
            "restart" => Kind::Restart,
            "mark" => Kind::Mark,
            _ if content.ends_with(super::linebuf::TRUNCATION_MARK) => Kind::Truncated,
            _ => Kind::Line,
        };
        self.push_entry(service, content, stream, kind, ts_ms);
    }

    /// Insert a `──── HH:MM:SS mark ────` separator into `service`'s stream.
    /// Returns the line text (for the disk log).
    pub fn push_mark(&mut self, service: String) -> String {
        let (_, time_str) = self.time.parts(now_ms());
        let line = format!("──── {time_str} mark ────");
        self.push_entry(service, line.clone(), LogStream::Stdout, Kind::Mark, None);
        line
    }

    /// Insert a `──── HH:MM:SS restarted ────` separator: the lines below it
    /// belong to a new run of `service`. Returns the line text (disk log).
    pub fn push_restart(&mut self, service: String) -> String {
        let (_, time_str) = self.time.parts(now_ms());
        let line = format!("──── {time_str} restarted ────");
        self.push_entry(
            service,
            line.clone(),
            LogStream::Stdout,
            Kind::Restart,
            None,
        );
        line
    }

    fn push_entry(
        &mut self,
        service: String,
        content: String,
        stream: LogStream,
        kind: Kind,
        at_ms: Option<u64>,
    ) -> &LogEntry {
        let (timestamp, time_str) = self.time.parts(at_ms.unwrap_or_else(now_ms));
        let level = if matches!(kind, Kind::Line | Kind::Truncated) {
            LogLevel::detect(&content)
        } else {
            LogLevel::Info
        };
        let entry = LogEntry {
            seq: self.next_seq,
            timestamp,
            service: service.clone(),
            stream,
            level,
            content,
            time_str,
            mark: kind == Kind::Mark,
            restart: kind == Kind::Restart,
            truncated: kind == Kind::Truncated,
        };
        self.next_seq += 1;
        if kind == Kind::Restart {
            self.last_restart.insert(service.clone(), entry.seq);
        }
        let (cap, bytes) = (self.capacity, self.max_bytes);
        let ring = self.rings.entry(service).or_default();
        let (log, total) = (&mut self.evictions, &mut self.evicted_total);
        ring.push(entry, cap, bytes, |seq| {
            if log.len() == EVICTION_LOG {
                log.pop_front();
            }
            log.push_back(seq);
            *total += 1;
        });
        ring.entries.back().expect("just pushed")
    }

    /// Sequence number the next pushed entry will get (entries with `seq >= n`
    /// arrived after `n` was sampled).
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Evictions so far (see `evicted_since`).
    pub fn evicted_total(&self) -> u64 {
        self.evicted_total
    }

    /// `seq` of the entries evicted after the first `seen` evictions, in
    /// eviction order. `None` when the log no longer reaches back that far (the
    /// reader must rebuild).
    pub fn evicted_since(&self, seen: u64) -> Option<impl Iterator<Item = u64> + '_> {
        let missed = self.evicted_total.checked_sub(seen)? as usize;
        (missed <= self.evictions.len()).then(|| {
            self.evictions
                .range(self.evictions.len() - missed..)
                .copied()
        })
    }

    /// Changes when the buffer was cleared (indexes must rebuild).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// `seq` of the newest restart separator of `service` — or of any service
    /// for `None`. `None` when nothing restarted since the buffer was cleared.
    pub fn last_restart_seq(&self, service: Option<&str>) -> Option<u64> {
        match service {
            Some(s) => self.last_restart.get(s).copied(),
            None => self.last_restart.values().copied().max(),
        }
    }

    /// Oldest `seq` still held — for one service, or across all of them.
    /// `next_seq` when empty.
    pub fn oldest_seq(&self, service: Option<&str>) -> u64 {
        let front = |r: &Ring| r.entries.front().map(|e| e.seq);
        match service {
            Some(s) => self.rings.get(s).and_then(front),
            None => self.rings.values().filter_map(front).min(),
        }
        .unwrap_or(self.next_seq)
    }

    /// Total heap bytes held by all rings (what the byte budget bounds).
    pub fn bytes(&self) -> usize {
        self.rings.values().map(|r| r.bytes).sum()
    }

    /// Visit entries with `seq >= from` in `seq` order, for one service or all.
    /// Cost is O(visited + services · log ring): each ring is binary-searched.
    pub fn for_each_from(&self, from: u64, service: Option<&str>, mut f: impl FnMut(&LogEntry)) {
        let mut cursors: Vec<std::collections::vec_deque::Iter<'_, LogEntry>> = self
            .rings
            .iter()
            .filter(|(name, _)| service.is_none_or(|s| s == name.as_str()))
            .map(|(_, r)| {
                let start = r.entries.partition_point(|e| e.seq < from);
                r.entries.range(start..)
            })
            .collect();
        match cursors.len() {
            0 => {}
            1 => cursors.swap_remove(0).for_each(&mut f),
            _ => {
                // k-way merge by seq (k = number of services: tiny).
                let mut heads: Vec<Option<&LogEntry>> =
                    cursors.iter_mut().map(Iterator::next).collect();
                while let Some((i, _)) = heads
                    .iter()
                    .enumerate()
                    .filter_map(|(i, h)| h.map(|e| (i, e.seq)))
                    .min_by_key(|&(_, s)| s)
                {
                    if let Some(e) = heads[i] {
                        f(e);
                    }
                    heads[i] = cursors[i].next();
                }
            }
        }
    }

    /// The entry with this `seq` (binary search in its service's ring).
    pub fn get(&self, service: &str, seq: u64) -> Option<&LogEntry> {
        let ring = &self.rings.get(service)?.entries;
        let i = ring.partition_point(|e| e.seq < seq);
        ring.get(i).filter(|e| e.seq == seq)
    }

    /// The entry with this `seq`, whichever service owns it (log(ring) per service).
    pub fn get_any(&self, seq: u64) -> Option<&LogEntry> {
        self.rings.values().find_map(|r| {
            let i = r.entries.partition_point(|e| e.seq < seq);
            r.entries.get(i).filter(|e| e.seq == seq)
        })
    }

    pub fn len(&self) -> usize {
        self.rings.values().map(|r| r.entries.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.rings.values().all(|r| r.entries.is_empty())
    }

    /// Every entry, oldest first (merged by sequence number).
    pub fn all(&self) -> Vec<&LogEntry> {
        self.merged(|_| true)
    }

    /// Merge the rings whose service passes `keep`, oldest first.
    pub fn merged(&self, keep: impl Fn(&str) -> bool) -> Vec<&LogEntry> {
        let mut out: Vec<&LogEntry> = self
            .rings
            .iter()
            .filter(|(name, _)| keep(name))
            .flat_map(|(_, ring)| ring.entries.iter())
            .collect();
        out.sort_unstable_by_key(|e| e.seq);
        out
    }

    /// Newest entry across all services.
    pub fn last(&self) -> Option<&LogEntry> {
        self.rings
            .values()
            .filter_map(|r| r.entries.back())
            .max_by_key(|e| e.seq)
    }

    pub fn filter_service(&self, service: &str) -> Vec<&LogEntry> {
        self.rings
            .get(service)
            .map(|r| r.entries.iter().collect())
            .unwrap_or_default()
    }

    pub fn filter_level(&self, min_level: LogLevel) -> Vec<&LogEntry> {
        self.merged(|_| true)
            .into_iter()
            .filter(|e| e.level.rank() >= min_level.rank())
            .collect()
    }

    pub fn errors(&self) -> Vec<&LogEntry> {
        self.merged(|_| true)
            .into_iter()
            .filter(|e| e.level == LogLevel::Error && !e.is_separator())
            .collect()
    }

    pub fn since(&self, seconds: u64) -> Vec<&LogEntry> {
        let cutoff = now_ms().saturating_sub(seconds.saturating_mul(1000));
        self.merged(|_| true)
            .into_iter()
            .filter(|e| e.timestamp >= cutoff)
            .collect()
    }

    pub fn tail(&self, n: usize) -> Vec<&LogEntry> {
        let all = self.all();
        let start = all.len().saturating_sub(n);
        all[start..].to_vec()
    }

    pub fn tail_service(&self, service: &str, n: usize) -> Vec<&LogEntry> {
        let filtered = self.filter_service(service);
        let start = filtered.len().saturating_sub(n);
        filtered[start..].to_vec()
    }

    /// Entries whose visible text matches `pattern` (smartcase literal).
    pub fn search(&self, pattern: &str) -> Vec<&LogEntry> {
        let Some(m) = super::logindex::Matcher::new(pattern, false) else {
            return Vec::new();
        };
        self.merged(|_| true)
            .into_iter()
            .filter(|e| !e.is_separator() && m.is_match(&strip_ansi(&e.content)))
            .collect()
    }

    pub fn clear(&mut self) {
        self.rings.clear();
        self.last_restart.clear();
        self.evictions.clear();
        self.generation += 1;
    }
}

/// What kind of row an entry is (private: callers use the `push_*` methods).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Line,
    Truncated,
    Mark,
    Restart,
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Local-time `HH:MM:SS` of an epoch-millis instant.
pub fn hms(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

pub fn format_for_clipboard(entries: &[&LogEntry], context: &ClipboardContext) -> String {
    let mut output = String::new();

    output.push_str(&format!("# {} — Dev Logs\n", context.project_name));
    output.push_str(&format!(
        "# Generated: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    if !context.services.is_empty() {
        output.push_str(&format!("# Services: {}\n", context.services.join(", ")));
    }
    if let Some(ref filter) = context.filter {
        output.push_str(&format!("# Filter: {}\n", filter));
    }
    output.push('\n');

    let mut current_service = "";
    for entry in entries {
        if entry.service != current_service {
            if !current_service.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!("── {} ──\n", entry.service));
            current_service = &entry.service;
        }

        let ts = format!(
            "{}.{:03}",
            hms(entry.timestamp as i64),
            entry.timestamp % 1000
        );
        let level_marker = match entry.level {
            LogLevel::Error => "[ERR]",
            LogLevel::Warn => "[WRN]",
            LogLevel::Debug => "[DBG]",
            LogLevel::Info => "",
        };

        if level_marker.is_empty() {
            output.push_str(&format!("{} {}\n", ts, strip_ansi(&entry.content)));
        } else {
            output.push_str(&format!(
                "{} {} {}\n",
                ts,
                level_marker,
                strip_ansi(&entry.content)
            ));
        }
    }

    output
}

pub fn format_for_ai_prompt(entries: &[&LogEntry], context: &ClipboardContext) -> String {
    let mut output = String::new();

    output.push_str("I'm working on a development environment and encountering issues. ");
    output.push_str("Here are the recent logs from my services:\n\n");

    output.push_str(&format!("Project: {}\n", context.project_name));
    output.push_str(&format!("Services: {}\n", context.services.join(", ")));
    output.push_str("\n```\n");

    let error_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.level == LogLevel::Error || e.level == LogLevel::Warn)
        .map(|(i, _)| i)
        .collect();

    if error_indices.is_empty() {
        let start = entries.len().saturating_sub(30);
        for entry in &entries[start..] {
            output.push_str(&format!(
                "[{}] {}\n",
                entry.service,
                strip_ansi(&entry.content)
            ));
        }
    } else {
        let mut shown: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for &idx in &error_indices {
            let start = idx.saturating_sub(3);
            let end = (idx + 3).min(entries.len());
            #[allow(clippy::needless_range_loop)]
            for i in start..end {
                if shown.insert(i) {
                    let marker = if i == idx { ">>>" } else { "   " };
                    output.push_str(&format!(
                        "{} [{}] {}\n",
                        marker,
                        entries[i].service,
                        strip_ansi(&entries[i].content)
                    ));
                }
            }
            if end < entries.len() {
                output.push_str("   ...\n");
            }
        }
    }

    output.push_str("```\n\n");
    output.push_str("Can you help me understand what's going wrong and how to fix it?\n");

    output
}

#[derive(Debug, Clone)]
pub struct ClipboardContext {
    pub project_name: String,
    pub services: Vec<String>,
    pub filter: Option<String>,
}

pub fn strip_ansi(s: &str) -> String {
    visible_chars(s).collect()
}

/// Characters of `s` that remain after removing CSI/OSC/string-control escape
/// sequences and C0/C1 control bytes (tab, LF and CR are kept).
pub fn visible_chars(s: &str) -> impl Iterator<Item = char> + '_ {
    #[derive(Clone, Copy)]
    enum State {
        Ground,
        Esc,
        Csi,
        Osc,
        OscEsc,
        StringControl,
        StringEsc,
    }

    let mut state = State::Ground;
    s.chars().filter_map(move |ch| {
        let code = ch as u32;
        let mut out = None;
        state = match state {
            State::Ground => match ch {
                '\x1b' => State::Esc,
                '\u{009b}' => State::Csi,
                '\u{009d}' => State::Osc,
                '\u{0090}' | '\u{009e}' | '\u{009f}' => State::StringControl,
                '\t' | '\n' | '\r' => {
                    out = Some(ch);
                    State::Ground
                }
                _ if code < 0x20 || (0x7f..=0x9f).contains(&code) => State::Ground,
                _ => {
                    out = Some(ch);
                    State::Ground
                }
            },
            State::Esc => match ch {
                '[' => State::Csi,
                ']' => State::Osc,
                'P' | 'X' | '^' | '_' => State::StringControl,
                '\x1b' => State::Esc,
                _ => State::Ground,
            },
            State::Csi => {
                if ch == '\x1b' {
                    State::Esc
                } else if (0x40..=0x7e).contains(&code) {
                    State::Ground
                } else {
                    State::Csi
                }
            }
            State::Osc => match ch {
                '\x07' => State::Ground,
                '\x1b' => State::OscEsc,
                _ => State::Osc,
            },
            State::OscEsc => match ch {
                '\\' => State::Ground,
                '\x1b' => State::OscEsc,
                _ => State::Osc,
            },
            State::StringControl => match ch {
                '\x1b' => State::StringEsc,
                _ => State::StringControl,
            },
            State::StringEsc => match ch {
                '\\' => State::Ground,
                '\x1b' => State::StringEsc,
                _ => State::StringControl,
            },
        };
        out
    })
}

pub async fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let mut child = if cfg!(target_os = "macos") {
        Command::new("pbcopy").stdin(Stdio::piped()).spawn()?
    } else {
        Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()?
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }

    let status = child.wait().await?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("clipboard command failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_detect_error() {
        assert_eq!(
            LogLevel::detect("Error: connection refused"),
            LogLevel::Error
        );
        assert_eq!(LogLevel::detect("FATAL: out of memory"), LogLevel::Error);
        assert_eq!(
            LogLevel::detect("panic: index out of bounds"),
            LogLevel::Error
        );
    }

    #[test]
    fn test_log_level_detect_word_tokens() {
        assert_eq!(LogLevel::detect("0 errors found"), LogLevel::Info);
        assert_eq!(LogLevel::detect("ERROR: boom"), LogLevel::Error);
        assert_eq!(LogLevel::detect("https://x/debug"), LogLevel::Info);
        assert_eq!(
            LogLevel::detect("\x1b[31mERROR\x1b[0m boom"),
            LogLevel::Error
        );
        assert_eq!(
            LogLevel::detect("\x1b[31mred but plain\x1b[0m"),
            LogLevel::Info
        );
        assert_eq!(LogLevel::detect("debug then warn"), LogLevel::Warn);
    }

    #[test]
    fn test_visible_chars_matches_strip_ansi() {
        let s = "\x1b[1;34mblue\x1b[0m \x1b]0;title\x07text\x1b[?25l";
        assert_eq!(visible_chars(s).collect::<String>(), "blue text");
    }

    #[test]
    fn test_log_level_detect_warn() {
        assert_eq!(LogLevel::detect("Warning: deprecated API"), LogLevel::Warn);
        assert_eq!(LogLevel::detect("WARN slow query detected"), LogLevel::Warn);
    }

    #[test]
    fn test_log_level_detect_info() {
        assert_eq!(
            LogLevel::detect("Server listening on port 3000"),
            LogLevel::Info
        );
        assert_eq!(LogLevel::detect("GET /api/users 200"), LogLevel::Info);
    }

    /// One table for both languages: `apps/desktop/src/lib/logs.test.ts` runs the same file
    /// through the desktop's `levelOf` (which shows debug as info).
    #[test]
    fn test_log_level_detect_shared_fixture() {
        let raw = include_str!("../../../../testdata/log-levels.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("fixture json");
        let cases = v["cases"].as_array().expect("cases");
        assert!(cases.len() > 20);
        for c in cases {
            let text = c["text"].as_str().expect("text");
            let want = c["level"].as_str().expect("level");
            assert_eq!(LogLevel::detect(text).wire(), want, "{text:?}");
        }
    }

    #[test]
    fn test_log_level_detect_debug() {
        assert_eq!(
            LogLevel::detect("DEBUG: cache hit for key xyz"),
            LogLevel::Debug
        );
        assert_eq!(LogLevel::detect("trace entering function"), LogLevel::Debug);
    }

    #[test]
    fn test_log_buffer_push_and_tail() {
        let mut buf = LogBuffer::new(100);
        buf.push("api".to_string(), "line 1".to_string(), LogStream::Stdout);
        buf.push("api".to_string(), "line 2".to_string(), LogStream::Stdout);
        buf.push("web".to_string(), "line 3".to_string(), LogStream::Stdout);

        assert_eq!(buf.len(), 3);
        assert_eq!(buf.tail(2).len(), 2);
        assert_eq!(buf.tail(2)[0].content, "line 2");
    }

    #[test]
    fn test_log_buffer_capacity() {
        let mut buf = LogBuffer::new(3);
        for i in 0..5 {
            buf.push("svc".to_string(), format!("line {i}"), LogStream::Stdout);
        }
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.all().first().unwrap().content, "line 2");
        assert_eq!(buf.all().last().unwrap().content, "line 4");
    }

    #[test]
    fn chatty_service_cannot_evict_others() {
        let mut buf = LogBuffer::new(3);
        buf.push("quiet".into(), "q0".into(), LogStream::Stdout);
        for i in 0..50 {
            buf.push("chatty".into(), format!("c{i}"), LogStream::Stdout);
        }
        buf.push("quiet".into(), "q1".into(), LogStream::Stdout);
        // Quiet keeps both lines; chatty keeps only its newest 3.
        assert_eq!(buf.filter_service("quiet").len(), 2);
        let chatty: Vec<_> = buf
            .filter_service("chatty")
            .iter()
            .map(|e| e.content.clone())
            .collect();
        assert_eq!(chatty, ["c47", "c48", "c49"]);
        // All merges by arrival order.
        let merged: Vec<_> = buf.all().iter().map(|e| e.content.clone()).collect();
        assert_eq!(merged, ["q0", "c47", "c48", "c49", "q1"]);
        assert_eq!(buf.last().unwrap().content, "q1");
    }

    #[test]
    fn mark_is_inserted_and_not_an_error() {
        let mut buf = LogBuffer::new(10);
        let line = buf.push_mark("api".into());
        assert!(line.starts_with("──── ") && line.ends_with(" mark ────"));
        let e = buf.all()[0];
        assert!(e.mark);
        assert_eq!(e.content, line);
        assert!(buf.errors().is_empty());
    }

    #[test]
    fn test_log_buffer_filter_service() {
        let mut buf = LogBuffer::new(100);
        buf.push("api".to_string(), "api line".to_string(), LogStream::Stdout);
        buf.push("web".to_string(), "web line".to_string(), LogStream::Stdout);
        buf.push(
            "api".to_string(),
            "api line 2".to_string(),
            LogStream::Stdout,
        );

        let filtered = buf.filter_service("api");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_log_buffer_errors() {
        let mut buf = LogBuffer::new(100);
        buf.push(
            "api".to_string(),
            "normal log".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "Error: something broke".to_string(),
            LogStream::Stderr,
        );
        buf.push(
            "api".to_string(),
            "another normal".to_string(),
            LogStream::Stdout,
        );

        let errors = buf.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].content, "Error: something broke");
    }

    #[test]
    fn test_log_buffer_search() {
        let mut buf = LogBuffer::new(100);
        buf.push(
            "api".to_string(),
            "Connected to database".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "Error: database timeout".to_string(),
            LogStream::Stderr,
        );
        buf.push(
            "web".to_string(),
            "Compiled successfully".to_string(),
            LogStream::Stdout,
        );

        let results = buf.search("database");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[31mError\x1b[0m"), "Error");
        assert_eq!(strip_ansi("no escapes here"), "no escapes here");
        assert_eq!(strip_ansi("\x1b[1;34mblue\x1b[0m text"), "blue text");
    }

    #[test]
    fn test_strip_ansi_preserves_newlines() {
        assert_eq!(
            strip_ansi("\x1b[31mfirst\x1b[0m\nsecond\r\n"),
            "first\nsecond\r\n"
        );
        assert_eq!(strip_ansi("\x1b[?25lready\x1b[?25h"), "ready");
    }

    #[test]
    fn test_format_for_clipboard() {
        let mut buf = LogBuffer::new(100);
        buf.push(
            "api".to_string(),
            "Started on :3000".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "Error: ECONNREFUSED".to_string(),
            LogStream::Stderr,
        );
        buf.push("web".to_string(), "Compiled".to_string(), LogStream::Stdout);

        let entries: Vec<&LogEntry> = buf.all();
        let ctx = ClipboardContext {
            project_name: "myapp".to_string(),
            services: vec!["api".to_string(), "web".to_string()],
            filter: None,
        };

        let output = format_for_clipboard(&entries, &ctx);
        assert!(output.contains("# myapp"));
        assert!(output.contains("── api ──"));
        assert!(output.contains("[ERR]"));
        assert!(output.contains("ECONNREFUSED"));
    }

    #[test]
    fn test_format_for_ai_prompt() {
        let mut buf = LogBuffer::new(100);
        buf.push(
            "api".to_string(),
            "Listening on :3000".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "GET /health 200".to_string(),
            LogStream::Stdout,
        );
        buf.push(
            "api".to_string(),
            "Error: Cannot read property 'id' of undefined".to_string(),
            LogStream::Stderr,
        );
        buf.push(
            "api".to_string(),
            "    at UserController.get (/app/src/user.ts:42)".to_string(),
            LogStream::Stderr,
        );

        let entries: Vec<&LogEntry> = buf.all();
        let ctx = ClipboardContext {
            project_name: "myapp".to_string(),
            services: vec!["api".to_string()],
            filter: None,
        };

        let output = format_for_ai_prompt(&entries, &ctx);
        assert!(output.contains("I'm working on a development environment"));
        assert!(output.contains(">>>"));
        assert!(output.contains("Cannot read property"));
        assert!(output.contains("Can you help me"));
    }

    #[test]
    fn test_format_for_ai_prompt_no_errors() {
        let mut buf = LogBuffer::new(100);
        for i in 0..50 {
            buf.push(
                "api".to_string(),
                format!("Normal log line {i}"),
                LogStream::Stdout,
            );
        }

        let entries: Vec<&LogEntry> = buf.all();
        let ctx = ClipboardContext {
            project_name: "test".to_string(),
            services: vec!["api".to_string()],
            filter: None,
        };

        let output = format_for_ai_prompt(&entries, &ctx);
        assert!(output.contains("Normal log line 49"));
        assert!(output.contains("Normal log line 20"));
    }

    #[test]
    fn test_log_buffer_tail_service() {
        let mut buf = LogBuffer::new(100);
        for i in 0..10 {
            buf.push("api".to_string(), format!("api {i}"), LogStream::Stdout);
            buf.push("web".to_string(), format!("web {i}"), LogStream::Stdout);
        }

        let tail = buf.tail_service("api", 3);
        assert_eq!(tail.len(), 3);
        assert_eq!(tail[0].content, "api 7");
        assert_eq!(tail[2].content, "api 9");
    }
}

#[cfg(test)]
mod ingest_tests {
    use super::*;

    #[test]
    fn every_line_gets_a_monotonic_seq_level_and_time_once() {
        let mut b = LogBuffer::new(100);
        b.push("a".into(), "ok".into(), LogStream::Stdout);
        b.push("b".into(), "ERROR boom".into(), LogStream::Stderr);
        b.push("a".into(), "warn: slow".into(), LogStream::Stdout);
        let all = b.all();
        assert_eq!(all.iter().map(|e| e.seq).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(
            all.iter().map(|e| e.level).collect::<Vec<_>>(),
            [LogLevel::Info, LogLevel::Error, LogLevel::Warn]
        );
        assert!(all.iter().all(|e| e.time_str.len() == 8 && e.timestamp > 0));
        assert_eq!(b.next_seq(), 3);
    }

    #[test]
    fn restart_marker_is_a_separator_not_output_or_an_error() {
        let mut b = LogBuffer::new(100);
        b.push("a".into(), "boot".into(), LogStream::Stdout);
        let line = b.push_restart("a".into());
        assert!(line.starts_with("──── ") && line.ends_with(" restarted ────"));
        let e = b.last().unwrap();
        assert!(e.restart && e.is_separator() && !e.mark);
        assert!(b.errors().is_empty());
        assert!(
            b.search("restarted").is_empty(),
            "separators are never matched"
        );
    }

    #[test]
    fn truncated_lines_are_flagged_from_the_assembler_marker() {
        let mut b = LogBuffer::new(10);
        let cut = format!(
            "{}{}",
            "x".repeat(10),
            super::super::linebuf::TRUNCATION_MARK
        );
        b.push("a".into(), cut, LogStream::Stdout);
        b.push("a".into(), "normal".into(), LogStream::Stdout);
        let v = b.filter_service("a");
        assert!(v[0].truncated && !v[1].truncated);
    }

    #[test]
    fn byte_budget_evicts_oldest_but_always_keeps_the_newest_line() {
        let mut b = LogBuffer::with_budget(1_000_000, 4096);
        for i in 0..100 {
            b.push(
                "a".into(),
                format!("{i} {}", "y".repeat(200)),
                LogStream::Stdout,
            );
        }
        assert!(b.bytes() <= 4096 + 600, "bytes {}", b.bytes());
        assert!(b.len() < 100 && !b.is_empty());
        assert!(b.last().unwrap().content.starts_with("99 "));
        // A single line over the whole budget is still kept.
        let mut b = LogBuffer::with_budget(10, 100);
        b.push("a".into(), "z".repeat(10_000), LogStream::Stdout);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn for_each_from_merges_services_by_seq_and_starts_at_from() {
        let mut b = LogBuffer::new(100);
        for i in 0..6 {
            b.push(
                if i % 2 == 0 { "a" } else { "b" }.into(),
                format!("{i}"),
                LogStream::Stdout,
            );
        }
        let mut seen = Vec::new();
        b.for_each_from(2, None, |e| seen.push(e.seq));
        assert_eq!(seen, [2, 3, 4, 5]);
        let mut a = Vec::new();
        b.for_each_from(0, Some("a"), |e| a.push(e.seq));
        assert_eq!(a, [0, 2, 4]);
        assert_eq!(b.get("b", 3).unwrap().content, "3");
        assert!(b.get("a", 3).is_none());
        assert_eq!(b.get_any(5).unwrap().content, "5");
    }

    #[test]
    fn oldest_seq_follows_eviction_per_service_and_overall() {
        let mut b = LogBuffer::new(2);
        for i in 0..5 {
            b.push("a".into(), format!("{i}"), LogStream::Stdout);
        }
        b.push("b".into(), "x".into(), LogStream::Stdout);
        assert_eq!(b.oldest_seq(Some("a")), 3);
        assert_eq!(b.oldest_seq(None), 3);
        assert_eq!(b.oldest_seq(Some("zzz")), b.next_seq());
    }

    #[test]
    fn level_detect_fast_path_agrees_with_the_regex_path() {
        for (line, want) in [
            ("plain text 12345", LogLevel::Info),
            ("ERROR x", LogLevel::Error),
            ("WaRn x", LogLevel::Warn),
            ("a debug b", LogLevel::Debug),
            ("terror in the night", LogLevel::Info),
            ("café ERROR ünï", LogLevel::Error),
            ("\x1b[31mFATAL\x1b[0m", LogLevel::Error),
        ] {
            assert_eq!(LogLevel::detect(line), want, "{line:?}");
        }
    }

    #[test]
    fn pump_assembles_partial_lines_cr_and_flushes_on_idle_and_eof() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            use tokio::io::AsyncWriteExt;
            let (mut w, r) = tokio::io::duplex(1024);
            let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let sink = out.clone();
            let h = spawn_line_pump(r, move |l| sink.lock().unwrap().push(l));
            w.write_all(b"he").await.unwrap();
            w.write_all(b"llo\r\nprog 1%\rprog 99%\rdone\nprompt> ")
                .await
                .unwrap();
            // Idle: the partial prompt must show up without a newline.
            tokio::time::sleep(PARTIAL_FLUSH_AFTER * 3).await;
            assert_eq!(*out.lock().unwrap(), ["hello", "done", "prompt> "]);
            w.write_all(b"tail-no-newline").await.unwrap();
            drop(w);
            h.await.unwrap();
            assert_eq!(out.lock().unwrap().last().unwrap(), "tail-no-newline");
        });
    }
}
