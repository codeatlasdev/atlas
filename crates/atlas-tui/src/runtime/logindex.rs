//! Search and view indexing over a `LogBuffer`.
//!
//! * `Matcher` — one compiled query (literal or regex, smartcase) shared by the
//!   TUI find bar and the MCP `atlas_logs_search` tool.
//! * `ViewIndex` — the rows of one *view* (tab + level filter + time window +
//!   optional filter query) plus every find hit in them, built
//!   **incrementally**: the buffer is scanned by sequence number, so each new
//!   line is looked at once and a frame never rescans history. A change of the
//!   view definition (`ViewKey`) or an eviction/clear in the buffer rebuilds
//!   it once. A sliding time window ("last 5 min") is advanced with
//!   `advance_clock`, which only pops the rows that aged out of the front.
//!
//! Hits are addressed as `(seq, occurrence)`; `n`-th / next / prev are
//! `O(log N)` over a prefix-sum of per-row hit counts.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::ops::Range;

use regex::{Regex, RegexBuilder};

use super::logs::{LogBuffer, LogEntry, LogLevel, visible_chars};
use super::timerange::Window;

/// A sliding time window is re-evaluated at most this often.
pub const CLOCK_STEP_MS: u64 = 1000;

/// A compiled query: literal or regex, smartcase (an all-lowercase query is
/// case-insensitive).
#[derive(Debug, Clone)]
pub struct Matcher {
    re: Regex,
    /// The query text this was built from (identity for cache keys).
    query: String,
    is_regex: bool,
}

impl Matcher {
    /// `None` for an empty query or an invalid regex.
    pub fn new(query: &str, regex: bool) -> Option<Self> {
        Self::compile(query, regex).ok()
    }

    /// Like `new` but keeps the regex error for callers that report it (MCP).
    pub fn compile(query: &str, regex: bool) -> Result<Self, String> {
        Self::build(query, regex, !query.chars().any(char::is_uppercase))
    }

    /// Always case-insensitive (the MCP search contract).
    pub fn compile_insensitive(query: &str, regex: bool) -> Result<Self, String> {
        Self::build(query, regex, true)
    }

    fn build(query: &str, regex: bool, insensitive: bool) -> Result<Self, String> {
        if query.is_empty() {
            return Err("query must not be empty".into());
        }
        let pat: Cow<'_, str> = if regex {
            query.into()
        } else {
            regex::escape(query).into()
        };
        RegexBuilder::new(&pat)
            .case_insensitive(insensitive)
            .build()
            .map(|re| Self {
                re,
                query: query.to_string(),
                is_regex: regex,
            })
            .map_err(|e| format!("invalid regex: {e}"))
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn is_regex(&self) -> bool {
        self.is_regex
    }

    /// Match byte ranges in `text` (empty matches skipped).
    pub fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        self.re
            .find_iter(text)
            .filter(|m| !m.is_empty())
            .map(|m| m.range())
            .collect()
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.re.find_iter(text).any(|m| !m.is_empty())
    }

    /// Number of non-empty matches in `text`.
    pub fn count(&self, text: &str) -> usize {
        self.re.find_iter(text).filter(|m| !m.is_empty()).count()
    }
}

/// What the message column of `entry` shows to the search (JSON-compacted when
/// `json`), with escape sequences removed.
pub fn search_text(entry: &LogEntry, json: bool) -> String {
    let (text, _) = crate::tui::widgets::log_viewer::display_content(entry, json);
    visible_chars(&text).collect()
}

/// Cheap prefilter: plain text of a line without allocating when it has no
/// escape/control bytes (the overwhelmingly common case).
fn plain_text(entry: &LogEntry, json: bool) -> Cow<'_, str> {
    if !json && !entry.content.bytes().any(|b| b < 0x20 || b == 0x7f) {
        Cow::Borrowed(entry.content.as_str())
    } else {
        Cow::Owned(search_text(entry, json))
    }
}

/// Which entries a view shows and what it searches. Any change rebuilds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewKey {
    /// `None` = every non-hidden service (the All tab).
    pub service: Option<String>,
    /// Services excluded from the All tab.
    pub hidden: Vec<String>,
    /// Minimum level (`None` = all). Marks always pass.
    pub min_level: Option<LogLevel>,
    /// Filter mode: only lines matching this survive (`query`, `regex`).
    pub filter: Option<(String, bool)>,
    /// Find mode: hits are counted for this (`query`, `regex`).
    pub find: Option<(String, bool)>,
    /// Search the JSON-compacted text.
    pub json: bool,
    /// Time bounds (`Window::default()` = unbounded). Resolved by the caller;
    /// a sliding window keeps only its length, so it never changes the key.
    pub window: Window,
}

#[derive(Debug, Clone, Copy)]
struct Row {
    seq: u64,
    /// Epoch millis of the line (the sliding window drops by it).
    ts: u64,
    /// Find hits inside this row.
    hits: u32,
    problem: Problem,
}

/// What a row is to the `]e` / `[e` jumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
    None,
    Warn,
    Error,
}

/// Oldest timestamp among the live rows: an ordered set of `(ts, seq)`, so
/// the minimum, an insert and the removal of any row are all O(log n). Lines
/// mostly arrive in time order, but a replayed backlog or an interior eviction
/// makes "the front row is the oldest" false, hence a real ordered set.
#[derive(Debug, Default)]
struct MinTs(std::collections::BTreeSet<(u64, u64)>);

impl MinTs {
    fn push(&mut self, seq: u64, ts: u64) {
        self.0.insert((ts, seq));
    }

    fn min(&self) -> Option<u64> {
        self.0.first().map(|&(ts, _)| ts)
    }

    fn remove(&mut self, row: &Row) {
        self.0.remove(&(row.ts, row.seq));
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

/// Sequence numbers of a row range: exact-size, double-ended and cheap to clone.
#[derive(Debug, Clone)]
pub struct Seqs<'a>(std::collections::vec_deque::Iter<'a, Row>);

impl Iterator for Seqs<'_> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        self.0.next().map(|r| r.seq)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for Seqs<'_> {}

impl DoubleEndedIterator for Seqs<'_> {
    fn next_back(&mut self) -> Option<u64> {
        self.0.next_back().map(|r| r.seq)
    }
}

/// Incremental projection of a `LogBuffer` for one `ViewKey`.
#[derive(Debug)]
pub struct ViewIndex {
    key: Option<ViewKey>,
    filter: Option<Matcher>,
    find: Option<Matcher>,
    rows: VecDeque<Row>,
    /// `prefix[i]` = total hits before `rows[i]`, counted from the first row
    /// ever indexed (monotonic; subtract `prefix[0]` for view-relative ordinals).
    /// Length = rows + 1.
    prefix: VecDeque<u64>,
    /// Every entry with `seq < next_seq` has been considered.
    next_seq: u64,
    /// Buffer generation this index was built from.
    generation: u64,
    /// Evictions of the buffer this index has already applied.
    evicted_seen: u64,
    /// Warn and error rows in order (`true` = error): `]e` / `[e` targets.
    problems: VecDeque<(u64, bool)>,
    errors: usize,
    warns: usize,
    /// The filter query does not compile: the view is empty by definition.
    reject_all: bool,
    /// The clock the sliding window was last evaluated at (epoch millis).
    clock_ms: u64,
    /// Oldest timestamp among the rows (sliding-window minimum).
    ts_min: MinTs,
    /// How many times `compact` ran (tests assert the straggler path is hit).
    #[cfg(test)]
    compactions: usize,
}

impl Default for ViewIndex {
    fn default() -> Self {
        Self {
            key: None,
            filter: None,
            find: None,
            rows: VecDeque::new(),
            prefix: VecDeque::from([0]),
            next_seq: 0,
            generation: u64::MAX,
            evicted_seen: 0,
            problems: VecDeque::new(),
            errors: 0,
            warns: 0,
            reject_all: false,
            clock_ms: 0,
            ts_min: MinTs::default(),
            #[cfg(test)]
            compactions: 0,
        }
    }
}

impl ViewIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bring the index up to date with `buf` for `key`. Cost: O(new lines +
    /// lines that aged out of a sliding window) when only appends happened;
    /// one full rebuild when the key changed or the buffer evicted/cleared
    /// lines the index still holds.
    pub fn sync(&mut self, buf: &LogBuffer, key: &ViewKey) {
        self.sync_at(buf, key, super::logs::now_ms());
    }

    /// `sync` at an explicit wall-clock `now_ms` (the clock only matters to a
    /// sliding window).
    pub fn sync_at(&mut self, buf: &LogBuffer, key: &ViewKey, now_ms: u64) {
        // A view change, a clear, or an eviction log that no longer reaches
        // back to what this index has applied: rebuild once from the buffer.
        let rebuild = self.key.as_ref() != Some(key)
            || self.generation != buf.generation()
            || buf.evicted_since(self.evicted_seen).is_none();
        if rebuild {
            self.reset(key.clone());
            self.generation = buf.generation();
            self.evicted_seen = buf.evicted_total();
            self.clock_ms = now_ms;
        }
        // Appends only advance the high-water mark; eviction is handled by
        // `trim` below so memory follows the ring, not the history.
        let from = self.next_seq;
        let to = buf.next_seq();
        if from < to {
            if !self.reject_all {
                self.scan(buf, key, from);
            }
            self.next_seq = to;
        }
        self.trim(buf);
        self.advance_clock(now_ms);
    }

    /// Move a sliding window's cutoff to `now_ms`, dropping the rows that aged
    /// out. Pops from the front only (lines arrive in time order), so it costs
    /// O(dropped). No-op without a sliding window, and re-evaluated at most
    /// once per `CLOCK_STEP_MS` so a frame never pays for it.
    pub fn advance_clock(&mut self, now_ms: u64) {
        let Some(last_ms) = self.key.as_ref().and_then(|k| k.window.last_ms) else {
            return;
        };
        if now_ms < self.clock_ms + CLOCK_STEP_MS {
            return;
        }
        self.clock_ms = now_ms;
        let cutoff = now_ms.saturating_sub(last_ms);
        // Lines arrive in time order, so aged-out rows are almost always a
        // prefix: pop it. `oldest_ts` says in O(1) when one is hiding behind a
        // newer row (replayed backlog, a clock step) and only then compact.
        let mut dropped = None;
        while self.rows.front().is_some_and(|r| r.ts < cutoff) {
            if let Some(row) = self.rows.pop_front() {
                self.ts_min.remove(&row);
                dropped = Some(row.seq);
            }
            self.prefix.pop_front();
        }
        if let Some(last_seq) = dropped {
            self.drop_problems_through(last_seq);
        }
        if self.ts_min.min().is_some_and(|ts| ts < cutoff) {
            self.compact(|r| r.ts >= cutoff);
        }
    }

    /// Append `row`: rows, prefix sum, jump targets and the oldest-timestamp
    /// tracker move together.
    fn push_row(&mut self, row: Row) {
        let last = self.prefix.back().copied().unwrap_or(0);
        self.prefix.push_back(last + u64::from(row.hits));
        match row.problem {
            Problem::Error => {
                self.problems.push_back((row.seq, true));
                self.errors += 1;
            }
            Problem::Warn => {
                self.problems.push_back((row.seq, false));
                self.warns += 1;
            }
            Problem::None => {}
        }
        self.ts_min.push(row.seq, row.ts);
        self.rows.push_back(row);
    }

    /// Rebuild rows, prefix sums, problem jumps and the timestamp tracker from
    /// the rows `keep` accepts. O(rows); only reached for out-of-order stragglers.
    fn compact(&mut self, keep: impl Fn(&Row) -> bool) {
        #[cfg(test)]
        {
            self.compactions += 1;
        }
        let kept: Vec<Row> = self.rows.iter().copied().filter(|r| keep(r)).collect();
        let base = self.base();
        self.rows.clear();
        self.prefix.clear();
        self.prefix.push_back(base);
        self.problems.clear();
        self.errors = 0;
        self.warns = 0;
        self.ts_min.clear();
        for r in kept {
            self.push_row(r);
        }
    }

    /// Consider every entry with `seq >= from` once.
    fn scan(&mut self, buf: &LogBuffer, key: &ViewKey, from: u64) {
        let json = key.json;
        let min = key.min_level;
        let window = key.window;
        let cutoff = window.cutoff(self.clock_ms);
        buf.for_each_from(from.max(window.min_seq), key.service.as_deref(), |e| {
            if key.service.is_none() && key.hidden.contains(&e.service) {
                return;
            }
            if !window.allows(e.seq, e.timestamp, cutoff) {
                return;
            }
            let synthetic = e.mark || e.restart;
            if !synthetic && min.is_some_and(|m| e.level.rank() < m.rank()) {
                return;
            }
            let mut text: Option<Cow<'_, str>> = None;
            if let Some(f) = &self.filter {
                if synthetic {
                    return;
                }
                if !f.is_match(text.get_or_insert_with(|| plain_text(e, json))) {
                    return;
                }
            }
            let hits = match (&self.find, synthetic) {
                (Some(f), false) => f.count(text.get_or_insert_with(|| plain_text(e, json))) as u32,
                _ => 0,
            };
            let problem = match e.level {
                _ if synthetic => Problem::None,
                LogLevel::Error => Problem::Error,
                LogLevel::Warn => Problem::Warn,
                _ => Problem::None,
            };
            self.push_row(Row {
                seq: e.seq,
                ts: e.timestamp,
                hits,
                problem,
            });
        });
    }

    fn reset(&mut self, key: ViewKey) {
        self.filter = key.filter.as_ref().and_then(|(q, r)| Matcher::new(q, *r));
        self.find = key.find.as_ref().and_then(|(q, r)| Matcher::new(q, *r));
        self.reject_all = key.filter.is_some() && self.filter.is_none();
        self.rows.clear();
        self.prefix.clear();
        self.prefix.push_back(0);
        self.next_seq = 0;
        self.problems.clear();
        self.errors = 0;
        self.warns = 0;
        self.ts_min.clear();
        self.key = Some(key);
    }

    /// Drop exactly the rows whose entries the buffer evicted since the last
    /// sync. Each ring evicts from its own front, so on the All tab a dead row
    /// can sit anywhere among a quiet service's older lines: the buffer reports
    /// the evicted `seq`s and each is located by binary search. O(evicted ·
    /// log rows) plus a memmove; a reader that fell behind the eviction log
    /// rebuilds from the buffer once.
    fn trim(&mut self, buf: &LogBuffer) {
        let gone: Vec<u64> = buf
            .evicted_since(self.evicted_seen)
            .map(Iterator::collect)
            .unwrap_or_default();
        self.evicted_seen = buf.evicted_total();
        self.remove_batch(gone);
    }

    /// Remove the rows of `gone` (the view may not hold some: filtered out or
    /// aged out of a window already). Entries at the front of the view go in
    /// O(1); the rest are dropped in one pass over the tail that starts at the
    /// first of them, so the cost per sync does not grow with how many a
    /// burst evicted.
    fn remove_batch(&mut self, mut gone: Vec<u64>) {
        gone.sort_unstable();
        let mut at = 0;
        loop {
            // Skip evicted seqs the view never held (below its front).
            while at < gone.len() && self.rows.front().is_some_and(|r| gone[at] < r.seq) {
                at += 1;
            }
            let Some(front) = self.rows.front().map(|r| r.seq) else {
                return;
            };
            if gone.get(at) != Some(&front) {
                break;
            }
            let row = self.rows.pop_front().expect("front checked");
            self.ts_min.remove(&row);
            self.prefix.pop_front();
            self.drop_problems_through(row.seq);
            at += 1;
        }
        let interior = &gone[at..];
        let Some(&first) = interior.first() else {
            return;
        };
        let from = self.lower_bound(first);
        if from >= self.rows.len() {
            return;
        }
        let tail: Vec<Row> = self.rows.drain(from..).collect();
        // `prefix[i]` precedes `rows[i]`: keep entries 0..=from, rebuild after.
        self.prefix.truncate(from + 1);
        for r in tail {
            if interior.binary_search(&r.seq).is_ok() {
                self.ts_min.remove(&r);
                if r.problem != Problem::None
                    && let Ok(p) = self.problems.binary_search_by_key(&r.seq, |&(s, _)| s)
                {
                    self.problems.remove(p);
                    match r.problem {
                        Problem::Error => self.errors -= 1,
                        _ => self.warns -= 1,
                    }
                }
            } else {
                let last = self.prefix.back().copied().unwrap_or(0);
                self.prefix.push_back(last + u64::from(r.hits));
                self.rows.push_back(r);
            }
        }
    }

    /// Forget warn/error jump targets with `seq <= last` (their rows are gone).
    fn drop_problems_through(&mut self, last: u64) {
        while let Some(&(s, is_err)) = self.problems.front() {
            if s > last {
                break;
            }
            self.problems.pop_front();
            if is_err {
                self.errors -= 1;
            } else {
                self.warns -= 1;
            }
        }
    }

    // ---- rows -----------------------------------------------------------

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Sequence number of row `i` (oldest = 0).
    pub fn seq_at(&self, i: usize) -> Option<u64> {
        self.rows.get(i).map(|r| r.seq)
    }

    /// Row index of `seq` (exact), if that line is in the view.
    pub fn index_of(&self, seq: u64) -> Option<usize> {
        let i = self.rows.partition_point(|r| r.seq < seq);
        (self.rows.get(i)?.seq == seq).then_some(i)
    }

    /// First row index with `seq >= s`.
    pub fn lower_bound(&self, seq: u64) -> usize {
        self.rows.partition_point(|r| r.seq < seq)
    }

    // ---- find hits ------------------------------------------------------

    /// Total hits in the view (0 without a find query).
    pub fn total_hits(&self) -> usize {
        (self.prefix.back().copied().unwrap_or(0) - self.base()) as usize
    }

    /// Hits already dropped off the front.
    fn base(&self) -> u64 {
        self.prefix.front().copied().unwrap_or(0)
    }

    /// The n-th hit (0-based, oldest first) as `(seq, occurrence)`.
    pub fn nth_hit(&self, n: usize) -> Option<(u64, usize)> {
        if n >= self.total_hits() {
            return None;
        }
        let base = self.base();
        let n = n as u64 + base;
        // First row whose running total exceeds n.
        let at = |i: usize| self.prefix[i];
        let (mut lo, mut hi) = (1usize, self.prefix.len() - 1);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if at(mid) > n {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let row = lo - 1;
        Some((self.rows[row].seq, (n - at(row)) as usize))
    }

    /// Ordinal (0-based) of the hit `(seq, occ)`; `None` if it is not a hit of this view.
    pub fn hit_ordinal(&self, seq: u64, occ: usize) -> Option<usize> {
        let i = self.index_of(seq)?;
        (occ < self.rows[i].hits as usize).then(|| (self.prefix[i] - self.base()) as usize + occ)
    }

    /// Ordinal of the first hit at or after row-sequence `seq` (`total_hits`
    /// when there is none) — where a cursor sitting at `seq` lands.
    pub fn hit_ordinal_from_seq(&self, seq: u64) -> usize {
        (self.prefix[self.lower_bound(seq)] - self.base()) as usize
    }

    /// Next hit after ordinal `cur` (wraps); `None` without hits.
    pub fn next_hit(&self, cur: usize) -> Option<usize> {
        let n = self.total_hits();
        (n > 0).then(|| (cur.min(n - 1) + 1) % n)
    }

    /// Previous hit before ordinal `cur` (wraps); `None` without hits.
    pub fn prev_hit(&self, cur: usize) -> Option<usize> {
        let n = self.total_hits();
        (n > 0).then(|| (cur.min(n - 1) + n - 1) % n)
    }

    // ---- level jumps ----------------------------------------------------

    /// Nearest error (`warn` = also warnings) strictly after/before `from`,
    /// wrapping around the view. `from = None` starts at the newest edge
    /// (backward) or oldest edge (forward). `None` when the view has none.
    pub fn jump_level(&self, from: Option<u64>, forward: bool, warn: bool) -> Option<u64> {
        let want = |&(_, is_err): &(u64, bool)| warn || is_err;
        let n = self.problems.len();
        // Index of the first problem with seq > from (forward) / the slot to
        // search backwards from.
        let split = match from {
            Some(s) => self.problems.partition_point(|&(q, _)| q <= s),
            None if forward => 0,
            None => n,
        };
        let found = if forward {
            self.problems
                .range(split..)
                .chain(self.problems.range(..split))
                .find(|p| want(p))
        } else {
            let before = match from {
                // Strictly before `from`: skip an entry equal to it.
                Some(s) => self.problems.partition_point(|&(q, _)| q < s),
                None => n,
            };
            self.problems
                .range(..before)
                .rev()
                .chain(self.problems.range(before..).rev())
                .find(|p| want(p))
        };
        found.map(|&(s, _)| s)
    }

    pub fn error_count(&self) -> usize {
        self.errors
    }

    pub fn warn_count(&self) -> usize {
        self.warns
    }

    /// Row indices `[start, end)` as seqs, for the renderer (O(range)).
    pub fn seqs(&self, range: Range<usize>) -> Seqs<'_> {
        Seqs(
            self.rows
                .range(range.start.min(self.rows.len())..range.end.min(self.rows.len())),
        )
    }

    /// Hits inside row `i`.
    pub fn hits_at(&self, i: usize) -> usize {
        self.rows.get(i).map_or(0, |r| r.hits as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::logs::LogStream;

    fn key() -> ViewKey {
        ViewKey {
            service: None,
            hidden: vec![],
            min_level: None,
            filter: None,
            find: None,
            json: false,
            window: Window::default(),
        }
    }

    fn buf(lines: &[(&str, &str)]) -> LogBuffer {
        let mut b = LogBuffer::new(10_000);
        for (s, l) in lines {
            b.push((*s).into(), (*l).into(), LogStream::Stdout);
        }
        b
    }

    #[test]
    fn chatty_service_eviction_leaves_no_dead_rows_on_the_all_tab() {
        // `quiet` keeps its old lines while `chatty` (ring of 3) evicts: the
        // evicted seqs are *greater* than quiet's oldest, so trimming by the
        // global oldest seq would keep them.
        let mut b = LogBuffer::with_budget(3, usize::MAX);
        let mut k = key();
        k.find = Some(("x".into(), false));
        let mut ix = ViewIndex::new();
        b.push("quiet".into(), "x ERROR old".into(), LogStream::Stdout); // 0
        for i in 0..10 {
            b.push("chatty".into(), format!("x chatty {i}"), LogStream::Stdout);
            ix.sync(&b, &k);
            let live: Vec<u64> = b.all().iter().map(|e| e.seq).collect();
            assert_eq!(rows(&ix), live, "after chatty line {i}");
            assert!(rows(&ix).iter().all(|&s| b.get_any(s).is_some()));
        }
        // quiet's line (seq 0) is still there, chatty holds only its last 3.
        assert_eq!(rows(&ix), [0, 8, 9, 10]);
        assert_eq!(ix.total_hits(), 4, "hits of evicted rows are gone too");
        assert_eq!(ix.nth_hit(1), Some((8, 0)));
        assert_eq!((ix.error_count(), ix.warn_count()), (1, 0));
        // An evicted error leaves the jump list: only quiet's remains.
        b.push("quiet".into(), "x ERROR newer".into(), LogStream::Stdout); // 11
        b.push("quiet".into(), "x ERROR newest".into(), LogStream::Stdout); // 12
        for _ in 0..3 {
            b.push("quiet".into(), "x fine".into(), LogStream::Stdout);
            ix.sync(&b, &k);
        }
        let live: Vec<u64> = b.all().iter().map(|e| e.seq).collect();
        assert_eq!(rows(&ix), live);
        let errors = b
            .all()
            .iter()
            .filter(|e| e.level == LogLevel::Error)
            .count();
        assert_eq!(ix.error_count(), errors);
        assert_eq!(
            ix.jump_level(None, true, false),
            b.all()
                .iter()
                .find(|e| e.level == LogLevel::Error)
                .map(|e| e.seq)
        );
    }

    #[test]
    fn index_that_fell_behind_the_eviction_log_rebuilds() {
        let mut b = LogBuffer::with_budget(2, usize::MAX);
        let mut ix = ViewIndex::new();
        b.push("quiet".into(), "old".into(), LogStream::Stdout);
        ix.sync(&b, &key());
        // Far more evictions than the log remembers between two syncs.
        for i in 0..(super::super::logs::EVICTION_LOG_FOR_TESTS + 10) {
            b.push("chatty".into(), format!("l{i}"), LogStream::Stdout);
        }
        ix.sync(&b, &key());
        let live: Vec<u64> = b.all().iter().map(|e| e.seq).collect();
        assert_eq!(rows(&ix), live);
    }

    #[test]
    fn matcher_smartcase_literal_and_regex() {
        let m = Matcher::new("error", false).unwrap();
        assert!(m.is_match("An ERROR happened"));
        let m = Matcher::new("Error", false).unwrap();
        assert!(!m.is_match("an error"));
        // Literal: regex metacharacters are text.
        assert!(Matcher::new("a.b", false).unwrap().is_match("xa.by"));
        assert!(!Matcher::new("a.b", false).unwrap().is_match("axb"));
        assert!(Matcher::new("a.b", true).unwrap().is_match("axb"));
        assert!(Matcher::new("(", true).is_none());
        assert!(
            Matcher::compile("(", true)
                .unwrap_err()
                .contains("invalid regex")
        );
        assert!(Matcher::new("", false).is_none());
        // Empty matches never count as hits.
        assert_eq!(Matcher::new("x*", true).unwrap().count("abc"), 0);
        assert_eq!(Matcher::new("ab", false).unwrap().count("ab ab ab"), 3);
    }

    #[test]
    fn index_follows_appends_without_rescanning() {
        let mut b = buf(&[("a", "one needle"), ("a", "two"), ("a", "needle needle")]);
        let mut k = key();
        k.find = Some(("needle".into(), false));
        let mut ix = ViewIndex::new();
        ix.sync(&b, &k);
        assert_eq!((ix.len(), ix.total_hits()), (3, 3));
        b.push("a".into(), "needle again".into(), LogStream::Stdout);
        ix.sync(&b, &k);
        assert_eq!((ix.len(), ix.total_hits()), (4, 4));
        assert_eq!(ix.nth_hit(0), Some((0, 0)));
        assert_eq!(ix.nth_hit(1), Some((2, 0)));
        assert_eq!(ix.nth_hit(2), Some((2, 1)));
        assert_eq!(ix.nth_hit(3), Some((3, 0)));
        assert_eq!(ix.nth_hit(4), None);
        assert_eq!(ix.hit_ordinal(2, 1), Some(2));
        assert_eq!(ix.hit_ordinal(1, 0), None);
    }

    #[test]
    fn next_prev_wrap_and_start_from_position() {
        let b = buf(&[("a", "x"), ("a", "x"), ("a", "x")]);
        let mut k = key();
        k.find = Some(("x".into(), false));
        let mut ix = ViewIndex::new();
        ix.sync(&b, &k);
        assert_eq!(ix.next_hit(2), Some(0), "next wraps");
        assert_eq!(ix.prev_hit(0), Some(2), "prev wraps");
        assert_eq!(ix.next_hit(0), Some(1));
        assert_eq!(ix.prev_hit(2), Some(1));
        assert_eq!(ix.hit_ordinal_from_seq(1), 1);
        assert_eq!(ix.hit_ordinal_from_seq(99), 3);
        // No hits: no movement.
        let mut k2 = key();
        k2.find = Some(("zzz".into(), false));
        ix.sync(&b, &k2);
        assert_eq!((ix.next_hit(0), ix.prev_hit(0)), (None, None));
    }

    #[test]
    fn filter_view_keeps_only_matching_rows_and_marks_are_dropped() {
        let mut b = buf(&[("a", "alpha"), ("a", "beta"), ("b", "alphabet")]);
        b.push_mark("a".into());
        let mut k = key();
        k.filter = Some(("alpha".into(), false));
        let mut ix = ViewIndex::new();
        ix.sync(&b, &k);
        assert_eq!(ix.seq_at(0), Some(0));
        assert_eq!(ix.len(), 2);
        // Single-service view.
        let mut k = key();
        k.service = Some("a".into());
        ix.sync(&b, &k);
        assert_eq!(ix.len(), 3, "alpha, beta, mark");
        // Hidden services are excluded from All.
        let mut k = key();
        k.hidden = vec!["b".into()];
        ix.sync(&b, &k);
        assert_eq!(ix.len(), 3);
    }

    #[test]
    fn level_filter_and_jumps_wrap() {
        let b = buf(&[
            ("a", "ok"),
            ("a", "ERROR one"),
            ("a", "ok"),
            ("a", "WARN two"),
            ("a", "ok"),
            ("a", "ERROR three"),
        ]);
        let mut ix = ViewIndex::new();
        ix.sync(&b, &key());
        assert_eq!((ix.error_count(), ix.warn_count()), (2, 1));
        // Errors only.
        assert_eq!(ix.jump_level(None, true, false), Some(1));
        assert_eq!(ix.jump_level(Some(1), true, false), Some(5));
        assert_eq!(ix.jump_level(Some(5), true, false), Some(1), "wraps");
        assert_eq!(ix.jump_level(Some(5), false, false), Some(1));
        assert_eq!(ix.jump_level(Some(1), false, false), Some(5), "wraps back");
        assert_eq!(ix.jump_level(None, false, false), Some(5));
        // Warn + error.
        assert_eq!(ix.jump_level(Some(1), true, true), Some(3));
        assert_eq!(ix.jump_level(Some(3), true, true), Some(5));
        assert_eq!(ix.jump_level(Some(5), true, true), Some(1));
        assert_eq!(ix.jump_level(Some(5), false, true), Some(3));
        assert_eq!(ix.jump_level(Some(1), false, true), Some(5));
        // min_level view only holds those rows.
        let mut k = key();
        k.min_level = Some(LogLevel::Error);
        ix.sync(&b, &k);
        assert_eq!(ix.len(), 2);
        // No problems anywhere → None.
        let quiet = buf(&[("a", "fine")]);
        ix.sync(&quiet, &key());
        assert_eq!(ix.jump_level(None, true, true), None);
    }

    #[test]
    fn eviction_trims_rows_and_hits_stay_consistent() {
        let mut b = LogBuffer::new(3);
        let mut k = key();
        k.find = Some(("n".into(), false));
        let mut ix = ViewIndex::new();
        for i in 0..10 {
            b.push("a".into(), format!("n{i}"), LogStream::Stdout);
            ix.sync(&b, &k);
        }
        assert_eq!((ix.len(), ix.total_hits()), (3, 3));
        assert_eq!(ix.seq_at(0), Some(7));
        assert_eq!(ix.nth_hit(0), Some((7, 0)));
        assert_eq!(ix.nth_hit(2), Some((9, 0)));
        assert_eq!(ix.hit_ordinal(9, 0), Some(2));
    }

    #[test]
    fn clear_rebuilds_from_empty() {
        let mut b = buf(&[("a", "x"), ("a", "x")]);
        let mut ix = ViewIndex::new();
        ix.sync(&b, &key());
        assert_eq!(ix.len(), 2);
        b.clear();
        ix.sync(&b, &key());
        assert!(ix.is_empty());
        b.push("a".into(), "y".into(), LogStream::Stdout);
        ix.sync(&b, &key());
        assert_eq!(ix.len(), 1);
    }

    #[test]
    fn json_flag_changes_the_searched_text() {
        let b = buf(&[("a", r#"{"level":30,"msg":"hi","user":"ana"}"#)]);
        let mut k = key();
        k.filter = Some(("user=ana".into(), false));
        let mut ix = ViewIndex::new();
        ix.sync(&b, &k);
        assert!(ix.is_empty());
        k.json = true;
        ix.sync(&b, &k);
        assert_eq!(ix.len(), 1);
    }

    #[test]
    fn invalid_filter_shows_nothing() {
        let b = buf(&[("a", "x")]);
        let mut k = key();
        k.filter = Some(("(".into(), true));
        let mut ix = ViewIndex::new();
        ix.sync(&b, &k);
        assert!(ix.is_empty());
    }

    #[test]
    fn index_survives_restart_markers_and_counts_them_as_rows() {
        let mut b = buf(&[("a", "boot")]);
        b.push_restart("a".into());
        b.push("a".into(), "ERROR after".into(), LogStream::Stdout);
        let mut ix = ViewIndex::new();
        ix.sync(&b, &key());
        assert_eq!(ix.len(), 3);
        assert_eq!(ix.error_count(), 1);
    }

    // ---- time windows ---------------------------------------------------

    use crate::runtime::timerange::{TimeRange, resolve};

    fn put(b: &mut LogBuffer, svc: &str, text: &str, ts: u64) {
        b.push_at(svc.into(), text.into(), LogStream::Stdout, ts);
    }

    fn rows(ix: &ViewIndex) -> Vec<u64> {
        ix.seqs(0..ix.len()).collect()
    }

    #[test]
    fn sliding_window_drops_aged_lines_from_the_front_and_keeps_counts() {
        let mut b = LogBuffer::new(1000);
        put(&mut b, "a", "old ERROR", 1_000); // 0
        put(&mut b, "a", "WARN mid", 61_000); // 1
        put(&mut b, "a", "fresh", 100_000); // 2
        let mut k = key();
        k.window = Window {
            last_ms: Some(60_000),
            ..Window::default()
        };
        let mut ix = ViewIndex::new();
        ix.sync_at(&b, &k, 100_000); // cutoff 40_000 → seq 0 out
        assert_eq!(rows(&ix), [1, 2]);
        assert_eq!((ix.error_count(), ix.warn_count()), (0, 1));
        // Less than a step later: not re-evaluated.
        ix.sync_at(&b, &k, 100_000 + CLOCK_STEP_MS - 1);
        assert_eq!(rows(&ix), [1, 2]);
        // The WARN line ages out (cutoff 61_001 > 61_000).
        ix.sync_at(&b, &k, 121_001);
        assert_eq!(rows(&ix), [2]);
        assert_eq!((ix.error_count(), ix.warn_count()), (0, 0));
        assert_eq!(ix.jump_level(None, true, true), None);
        // Everything ages out; a new line still shows up.
        ix.sync_at(&b, &k, 500_000);
        assert!(ix.is_empty());
        put(&mut b, "a", "newer", 500_000);
        ix.sync_at(&b, &k, 500_000);
        assert_eq!(rows(&ix), [3]);
    }

    #[test]
    fn window_composes_with_level_filter_and_find_hits_follow_dropped_rows() {
        let mut b = LogBuffer::new(1000);
        put(&mut b, "a", "needle ERROR", 10_000); // 0
        put(&mut b, "a", "needle needle ERROR", 70_000); // 1
        put(&mut b, "a", "needle info", 71_000); // 2
        let mut k = key();
        k.min_level = Some(LogLevel::Error);
        k.find = Some(("needle".into(), false));
        k.window = Window {
            last_ms: Some(120_000),
            ..Window::default()
        };
        let mut ix = ViewIndex::new();
        ix.sync_at(&b, &k, 80_000);
        assert_eq!((ix.len(), ix.total_hits()), (2, 3));
        ix.sync_at(&b, &k, 135_000); // cutoff 15_000: row 0 gone
        assert_eq!((ix.len(), ix.total_hits()), (1, 2));
        assert_eq!(ix.nth_hit(0), Some((1, 0)));
        assert_eq!(ix.hit_ordinal(1, 1), Some(1));
    }

    #[test]
    fn since_restart_cuts_at_the_marker_and_follows_new_restarts() {
        let mut b = LogBuffer::new(1000);
        put(&mut b, "a", "one", 1_000);
        put(&mut b, "b", "other", 2_000);
        b.push_restart("a".into());
        put(&mut b, "a", "two", 3_000);
        let mut ix = ViewIndex::new();
        let mut k = key();
        k.window = resolve(TimeRange::SinceRestart, &b, None, 4_000).unwrap();
        ix.sync_at(&b, &k, 4_000);
        // The marker itself opens the window.
        assert_eq!(rows(&ix), [2, 3]);
        // A service tab is bounded by its own marker only.
        let mut kb = key();
        kb.service = Some("b".into());
        kb.window = resolve(TimeRange::SinceRestart, &b, Some("b"), 4_000).unwrap();
        ix.sync_at(&b, &kb, 4_000);
        assert_eq!(rows(&ix), [1]);
        // A later restart changes the resolved window → one rebuild.
        b.push_restart("b".into());
        put(&mut b, "b", "again", 5_000);
        kb.window = resolve(TimeRange::SinceRestart, &b, Some("b"), 5_000).unwrap();
        ix.sync_at(&b, &kb, 5_000);
        assert_eq!(rows(&ix), [4, 5]);
    }

    #[test]
    fn custom_range_bounds_both_ends() {
        let mut b = LogBuffer::new(1000);
        for (i, ts) in [100u64, 200, 300, 400].into_iter().enumerate() {
            put(&mut b, "a", &format!("l{i}"), ts);
        }
        let mut k = key();
        k.window = Window {
            from_ms: Some(200),
            to_ms: Some(300),
            ..Window::default()
        };
        let mut ix = ViewIndex::new();
        ix.sync_at(&b, &k, 1_000);
        assert_eq!(rows(&ix), [1, 2], "both ends inclusive");
    }

    /// Tiny deterministic generator (no rand dependency).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// What the view must show, computed from scratch.
    fn brute(b: &LogBuffer, k: &ViewKey, now: u64) -> Vec<u64> {
        let cutoff = k.window.cutoff(now);
        let filter = k.filter.as_ref().and_then(|(q, re)| Matcher::new(q, *re));
        b.all()
            .into_iter()
            .filter(|e| k.service.as_deref().is_none_or(|s| s == e.service))
            .filter(|e| k.service.is_some() || !k.hidden.contains(&e.service))
            .filter(|e| k.window.allows(e.seq, e.timestamp, cutoff))
            .filter(|e| e.is_separator() || k.min_level.is_none_or(|m| e.level.rank() >= m.rank()))
            .filter(|e| {
                filter
                    .as_ref()
                    .is_none_or(|m| !e.is_separator() && m.is_match(&e.content))
            })
            .map(|e| e.seq)
            .collect()
    }

    #[test]
    fn incremental_window_matches_brute_force_on_random_streams() {
        let words = ["alpha", "beta ERROR", "gamma WARN", "delta", "needle x"];
        let mut total_compactions = 0;
        for seed in 1..=12u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            // Small per-service rings and skewed volumes: the chatty service
            // evicts constantly while the quiet ones keep their old lines, so
            // dead rows sit in the middle of the All view.
            let mut b = LogBuffer::new(4 + rng.below(30) as usize);
            let mut ix = ViewIndex::new();
            let mut now = 1_000_000u64;
            let mut k = key();
            let mut range = TimeRange::All;
            let mut seen_compaction = false;
            let svcs = ["a", "a", "a", "a", "a", "a", "b", "b", "b", "c"];
            for step in 0..200 {
                now += rng.below(4_000); // time never goes back
                match rng.below(12) {
                    0 => {
                        b.push_restart(svcs[rng.below(10) as usize].into());
                    }
                    1 => {
                        b.push_mark(svcs[rng.below(10) as usize].into());
                    }
                    _ => put(
                        &mut b,
                        svcs[rng.below(10) as usize],
                        words[rng.below(5) as usize],
                        now,
                    ),
                }
                if step % 25 == 0 {
                    // Redefine the view now and then (the one-time rebuild).
                    k.service = [None, Some("a".to_string())][rng.below(2) as usize].clone();
                    k.hidden = if rng.below(3) == 0 {
                        vec!["c".into()]
                    } else {
                        vec![]
                    };
                    k.min_level =
                        [None, Some(LogLevel::Warn), Some(LogLevel::Error)][rng.below(3) as usize];
                    k.filter = (rng.below(4) == 0).then(|| ("needle".to_string(), false));
                    k.find =
                        (k.filter.is_none() && rng.below(2) == 0).then(|| ("a".to_string(), false));
                    range = match rng.below(5) {
                        0 => TimeRange::All,
                        1 => TimeRange::LastMinutes(1),
                        2 => TimeRange::SinceRestart,
                        3 => TimeRange::SinceStackStart,
                        _ => TimeRange::LastMinutes(5),
                    };
                }
                // Like the app: the window is re-resolved on every sync.
                k.window = resolve(range, &b, k.service.as_deref(), now).unwrap();
                ix.sync_at(&b, &k, now);
                // Throttled clock: compare at the clock the index last used.
                let want = brute(&b, &k, ix.clock_ms);
                assert_eq!(rows(&ix), want, "seed {seed} step {step}");
                let want_err = want
                    .iter()
                    .filter(|&&s| {
                        b.get_any(s)
                            .is_some_and(|e| !e.is_separator() && e.level == LogLevel::Error)
                    })
                    .count();
                assert_eq!(ix.error_count(), want_err, "seed {seed} step {step}");
                if let Some((q, re)) = &k.find {
                    let m = Matcher::new(q, *re).unwrap();
                    let hits: usize = want
                        .iter()
                        .filter_map(|&s| b.get_any(s))
                        .filter(|e| !e.is_separator())
                        .map(|e| m.count(&e.content))
                        .sum();
                    assert_eq!(ix.total_hits(), hits, "seed {seed} step {step}");
                }
                // Every row the view hands the renderer still exists.
                assert!(
                    rows(&ix).iter().all(|&s| b.get_any(s).is_some()),
                    "seed {seed} step {step}: a row points at an evicted entry"
                );
                seen_compaction |= ix.compactions > 0;
            }
            total_compactions += usize::from(seen_compaction);
        }
        // Marks/restarts carry the real clock, so straggler rows do occur.
        assert!(total_compactions > 0, "the compaction path was never taken");
    }
}
