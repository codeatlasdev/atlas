//! Wrapped-height cache for the log view.
//!
//! The renderer needs "how many screen rows is every entry" to place the
//! viewport; recomputing that for the whole buffer each frame is O(N·len).
//! `RowCache` keeps `heights` and their prefix sums for the rows of a
//! `ViewIndex`, keyed by the layout (`width` + `ViewOpts`). Appends extend it,
//! front eviction pops it, any layout change rebuilds it once. Lookups
//! (`total`, "which row is at screen row r", "where does entry i start") are
//! O(log N).

use std::collections::VecDeque;

use crate::runtime::logs::LogBuffer;

use super::log_viewer::{ViewOpts, entry_rows};

#[derive(Debug, Default)]
pub struct RowCache {
    width: u16,
    opts: Option<ViewOpts>,
    /// Sequence number of the entry each cached height belongs to.
    seqs: VecDeque<u64>,
    /// `cum[i]` = rows of entries `..i` (monotonic, base `cum[0]`).
    cum: VecDeque<usize>,
    /// Buffer generation + index identity this was built for.
    generation: u64,
}

impl RowCache {
    pub fn new() -> Self {
        Self {
            cum: VecDeque::from([0]),
            ..Self::default()
        }
    }

    /// Bring the cache in line with the index rows (`seqs`, oldest first) for
    /// `width`/`opts`.
    ///
    /// The cache only ever *extends at the back* and *trims at the front*; both
    /// are valid only while its rows are a contiguous slice of the index's
    /// rows. A change of the view definition (tab, level, query, hidden
    /// services) swaps the index for a different subset with the same width
    /// and opts, so every sync verifies that its cached rows are still a
    /// prefix-aligned slice of `seqs` and rebuilds from scratch when not.
    pub fn sync(
        &mut self,
        buf: &LogBuffer,
        service: Option<&str>,
        seqs: impl ExactSizeIterator<Item = u64> + DoubleEndedIterator + Clone,
        width: u16,
        opts: &ViewOpts,
    ) {
        let expected = seqs.len();
        self.sync_once(buf, service, seqs.clone(), width, opts);
        // Last line of defence: a cache that does not mirror the index row for
        // row (an index mutation this cache could not see) is rebuilt once.
        if self.seqs.len() != expected || self.seqs.back().copied() != seqs.clone().next_back() {
            self.seqs.clear();
            self.cum.clear();
            self.cum.push_back(0);
            self.sync_once(buf, service, seqs, width, opts);
        }
    }

    fn sync_once(
        &mut self,
        buf: &LogBuffer,
        service: Option<&str>,
        seqs: impl Iterator<Item = u64>,
        width: u16,
        opts: &ViewOpts,
    ) {
        let mut seqs = seqs.peekable();
        let first = seqs.peek().copied();
        let mut stale = self.width != width
            || self.opts.as_ref() != Some(opts)
            || self.generation != buf.generation()
            || first.is_none();
        if !stale && let (Some(cf), Some(f)) = (self.seqs.front().copied(), first) {
            // Front trim only when `first` is one of our rows; otherwise the
            // index is a different subset (or has moved past everything we hold).
            stale = f < cf || (f > cf && self.seqs.binary_search(&f).is_err());
        }
        if stale {
            self.seqs.clear();
            self.cum.clear();
            self.cum.push_back(0);
            self.width = width;
            self.opts = Some(*opts);
            self.generation = buf.generation();
        }
        // Drop rows the index no longer holds (front eviction).
        if let Some(f) = first {
            while self.seqs.front().is_some_and(|&s| s < f) {
                self.seqs.pop_front();
                self.cum.pop_front();
            }
        }
        // Append rows newer than the cached tail.
        let have = self.seqs.back().copied();
        for seq in seqs.skip_while(|s| have.is_some_and(|h| *s <= h)) {
            let rows = match service.map_or_else(|| buf.get_any(seq), |s| buf.get(s, seq)) {
                Some(e) => entry_rows(e, width, opts),
                None => 1,
            };
            let last = self.cum.back().copied().unwrap_or(0);
            self.seqs.push_back(seq);
            self.cum.push_back(last + rows);
        }
    }

    /// True when the cache holds exactly the rows `index_len` says the view has
    /// (renderers and scroll math bail out on a mismatch instead of guessing).
    pub fn matches(&self, index_len: usize) -> bool {
        self.seqs.len() == index_len
    }

    pub fn len(&self) -> usize {
        self.seqs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seqs.is_empty()
    }

    /// Total wrapped rows of the view.
    pub fn total(&self) -> usize {
        self.cum.back().copied().unwrap_or(0) - self.cum.front().copied().unwrap_or(0)
    }

    fn base(&self) -> usize {
        self.cum.front().copied().unwrap_or(0)
    }

    /// Screen row (from the top of the view) where entry `i` starts.
    pub fn row_start(&self, i: usize) -> usize {
        self.cum[i.min(self.seqs.len())] - self.base()
    }

    /// Rows entry `i` occupies.
    pub fn rows_of(&self, i: usize) -> usize {
        self.cum[i + 1] - self.cum[i]
    }

    /// Index of the entry containing screen row `row`, and how many of its
    /// rows lie above `row`. `None` when `row >= total`.
    pub fn locate(&self, row: usize) -> Option<(usize, usize)> {
        if row >= self.total() {
            return None;
        }
        let target = row + self.base();
        // Last i with cum[i] <= target.
        let (mut lo, mut hi) = (0usize, self.seqs.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.cum[mid + 1] <= target {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Some((lo, target - self.cum[lo]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::logs::LogStream;

    fn fill(n: usize, len: usize) -> LogBuffer {
        let mut b = LogBuffer::new(100_000);
        for i in 0..n {
            b.push(
                "a".into(),
                format!("{i}{}", "x".repeat(len)),
                LogStream::Stdout,
            );
        }
        b
    }

    fn seqs(b: &LogBuffer) -> Vec<u64> {
        b.all().iter().map(|e| e.seq).collect()
    }

    /// Reference: recompute everything from scratch.
    fn naive(b: &LogBuffer, w: u16, o: &ViewOpts) -> Vec<usize> {
        b.all().iter().map(|e| entry_rows(e, w, o)).collect()
    }

    #[test]
    fn incremental_matches_full_recompute_across_appends() {
        let opts = ViewOpts::default();
        let mut b = fill(5, 90);
        let mut c = RowCache::new();
        c.sync(&b, None, seqs(&b).into_iter(), 40, &opts);
        for i in 0..20 {
            b.push("a".into(), "y".repeat(i * 7), LogStream::Stdout);
            c.sync(&b, None, seqs(&b).into_iter(), 40, &opts);
            let want = naive(&b, 40, &opts);
            assert_eq!(c.len(), want.len());
            assert_eq!(c.total(), want.iter().sum::<usize>());
            for (i, w) in want.iter().enumerate() {
                assert_eq!(c.rows_of(i), *w, "row {i}");
            }
        }
    }

    #[test]
    fn width_or_opts_change_rebuilds() {
        let b = fill(10, 100);
        let mut c = RowCache::new();
        let mut opts = ViewOpts::default();
        c.sync(&b, None, seqs(&b).into_iter(), 40, &opts);
        let t40 = c.total();
        c.sync(&b, None, seqs(&b).into_iter(), 120, &opts);
        assert!(c.total() < t40);
        assert_eq!(c.total(), naive(&b, 120, &opts).iter().sum::<usize>());
        opts.wrap = false;
        c.sync(&b, None, seqs(&b).into_iter(), 120, &opts);
        assert_eq!(c.total(), 10, "no wrap: one row each");
    }

    #[test]
    fn eviction_pops_the_front() {
        let opts = ViewOpts::default();
        let mut b = LogBuffer::new(5);
        let mut c = RowCache::new();
        for i in 0..30 {
            b.push("a".into(), format!("line {i}"), LogStream::Stdout);
            c.sync(&b, Some("a"), seqs(&b).into_iter(), 80, &opts);
        }
        assert_eq!(c.len(), 5);
        assert_eq!(c.total(), 5);
        assert_eq!(c.row_start(0), 0);
    }

    #[test]
    fn locate_maps_screen_rows_to_entries() {
        let opts = ViewOpts {
            timestamps: false,
            show_service: false,
            ..ViewOpts::default()
        };
        let mut b = LogBuffer::new(100);
        b.push("a".into(), "x".repeat(25), LogStream::Stdout); // 3 rows @10
        b.push("a".into(), "y".into(), LogStream::Stdout); // 1
        b.push("a".into(), "z".repeat(10), LogStream::Stdout); // 1
        let mut c = RowCache::new();
        c.sync(&b, None, seqs(&b).into_iter(), 10, &opts);
        assert_eq!(c.total(), 5);
        assert_eq!(c.locate(0), Some((0, 0)));
        assert_eq!(c.locate(2), Some((0, 2)));
        assert_eq!(c.locate(3), Some((1, 0)));
        assert_eq!(c.locate(4), Some((2, 0)));
        assert_eq!(c.locate(5), None);
        assert_eq!(c.row_start(2), 4);
    }

    /// Regression (live blank log pane): the view definition changes (query on,
    /// then off) while width/opts/generation stay put. The index swaps to a
    /// different subset; the cache must follow, not keep the old rows.
    #[test]
    fn view_subset_swap_with_same_layout_rebuilds() {
        let opts = ViewOpts::default();
        let mut b = LogBuffer::new(100_000);
        for i in 0..1000 {
            let text = if i % 100 == 0 {
                format!("{i} boom")
            } else {
                format!("{i} fine")
            };
            b.push("a".into(), text, LogStream::Stdout);
        }
        let all: Vec<u64> = b.all().iter().map(|e| e.seq).collect();
        let hits: Vec<u64> = b
            .all()
            .iter()
            .filter(|e| e.content.contains("boom"))
            .map(|e| e.seq)
            .collect();
        let mut c = RowCache::new();
        c.sync(&b, None, all.clone().into_iter(), 60, &opts);
        assert!(c.matches(1000));
        // Filter to 10 rows (front seq is shared: the cache is NOT stale by front check).
        c.sync(&b, None, hits.clone().into_iter(), 60, &opts);
        assert!(c.matches(10), "cache len {} != 10", c.len());
        assert_eq!(c.total(), 10);
        // Back to everything.
        c.sync(&b, None, all.clone().into_iter(), 60, &opts);
        assert!(c.matches(1000), "cache len {} != 1000", c.len());
        assert_eq!(c.total(), naive(&b, 60, &opts).iter().sum::<usize>());
        assert_eq!(c.locate(c.total() - 1).map(|(i, _)| i), Some(999));
    }

    /// The index evicting its front while a *different* subset keeps the same
    /// front seq must not leave the cache holding rows the index dropped.
    #[test]
    fn eviction_plus_subset_swap_keeps_mirror() {
        let opts = ViewOpts::default();
        let mut b = LogBuffer::new(50);
        let mut c = RowCache::new();
        for i in 0..400 {
            b.push(
                "a".into(),
                format!("{i} {}", if i % 7 == 0 { "boom" } else { "ok" }),
                LogStream::Stdout,
            );
            let all: Vec<u64> = b.all().iter().map(|e| e.seq).collect();
            let view: Vec<u64> = if i % 3 == 0 {
                b.all()
                    .iter()
                    .filter(|e| e.content.contains("boom"))
                    .map(|e| e.seq)
                    .collect()
            } else {
                all
            };
            c.sync(&b, None, view.clone().into_iter(), 40, &opts);
            assert!(
                c.matches(view.len()),
                "step {i}: {} vs {}",
                c.len(),
                view.len()
            );
            let want: usize = view
                .iter()
                .map(|&s| entry_rows(b.get_any(s).unwrap(), 40, &opts))
                .sum();
            assert_eq!(c.total(), want, "step {i}");
        }
    }
}
