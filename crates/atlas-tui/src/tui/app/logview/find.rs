//! Incremental find / filter: hit counts, `n`/`N` stepping, scrolling to a
//! match, problem jumps (`]e`/`[e`) and the search-mode/regex toggles.
use super::super::{App, ToastVariant};
use super::{MatchPos, SearchMode};
use atlas_i18n::t;

impl App {
    /// Total find hits in the view (0 outside find mode).
    pub fn find_total(&self) -> usize {
        self.sync_logs();
        self.logs.index.borrow().total_hits()
    }

    /// `(current 1-based, total)` for the search bar; `(0, 0)` with no hits.
    pub fn find_counts(&self) -> (usize, usize) {
        self.sync_logs();
        let ix = self.logs.index.borrow();
        let total = if self.find_active() {
            ix.total_hits()
        } else {
            ix.len()
        };
        if total == 0 {
            (0, 0)
        } else {
            (self.logs.find_current.min(total - 1) + 1, total)
        }
    }

    /// Highlight target for the renderer: the current hit.
    pub fn current_match(&self) -> Option<MatchPos> {
        if !self.find_active() {
            return None;
        }
        self.sync_logs();
        let ix = self.logs.index.borrow();
        let n = ix.total_hits();
        (n > 0).then(|| ix.nth_hit(self.logs.find_current.min(n - 1)))?
    }

    /// The query changed: land on the newest hit (closest to where the eye is).
    pub(in crate::tui::app) fn find_requery(&mut self) {
        self.sync_logs();
        let n = self.logs.index.borrow().total_hits();
        self.logs.find_current = n.saturating_sub(1);
        self.reset_log_view();
        if n > 0 {
            self.scroll_to_current_match();
        }
    }

    /// `n` / `N`: step to the next / previous hit (wraps), pausing follow.
    pub(in crate::tui::app) fn find_step(&mut self, forward: bool) {
        self.sync_logs();
        let next = {
            let ix = self.logs.index.borrow();
            let cur = self
                .logs
                .find_current
                .min(ix.total_hits().saturating_sub(1));
            if forward {
                ix.next_hit(cur)
            } else {
                ix.prev_hit(cur)
            }
        };
        let Some(next) = next else { return };
        self.logs.find_current = next;
        self.scroll_to_current_match();
    }

    /// Scroll so entry `seq` sits in view (pauses follow). O(log N) via the
    /// row cache.
    pub(in crate::tui::app) fn scroll_to_seq(&mut self, seq: u64) {
        let vp = self.logs.log_viewport.get();
        if vp.text_width == 0 || vp.height == 0 {
            return;
        }
        self.sync_logs();
        let (rows_after, own_rows) = {
            let ix = self.logs.index.borrow();
            let Some(i) = ix.index_of(seq) else { return };
            let rows = self.logs.rows.borrow();
            if rows.len() != ix.len() {
                return;
            }
            let own = rows.rows_of(i);
            (rows.total() - rows.row_start(i) - own, own)
        };
        // Whole entry already on the bottom screen: stay following. Otherwise
        // centre it (clamped to what the viewport can actually scroll).
        let scroll = if rows_after + own_rows <= vp.height {
            0
        } else {
            (rows_after + own_rows).saturating_sub((vp.height + own_rows) / 2)
        };
        self.logs.log_scroll = scroll.min(vp.max_scroll);
        let paused = self.logs.log_scroll > 0;
        if paused && self.logs.follow_logs {
            self.logs.paused_since_seq = Some(self.logs.log_buffer.next_seq());
        }
        self.logs.follow_logs = !paused;
    }

    /// Scroll so the current match's row sits in view (pauses follow).
    pub(in crate::tui::app) fn scroll_to_current_match(&mut self) {
        if let Some((seq, _)) = self.current_match() {
            self.scroll_to_seq(seq);
        }
    }

    /// `]e` / `[e` / `]w` / `[w`: nearest error (or warn+error) after / before
    /// the last jump (or the viewport when none), wrapping around.
    pub(in crate::tui::app) fn jump_problem(&mut self, forward: bool, warn: bool) {
        self.sync_logs();
        let from = self.logs.problem_cursor.or_else(|| self.viewport_anchor());
        let target = self.logs.index.borrow().jump_level(from, forward, warn);
        let Some(seq) = target else {
            self.set_toast(
                if warn {
                    t!("tui_runtime_app_toast_no_problem_lines")
                } else {
                    t!("tui_runtime_app_toast_no_error_lines")
                },
                ToastVariant::Info,
            );
            return;
        };
        self.logs.problem_cursor = Some(seq);
        self.logs.link_cursor = None;
        self.scroll_to_seq(seq);
    }

    /// Seq of the entry at the top of the viewport (jump origin when no jump
    /// has happened yet): `]e` then goes to the first problem below the top.
    fn viewport_anchor(&self) -> Option<u64> {
        self.seq_at_viewport_row(|vp| vp.height)
    }

    /// `Tab` in the search bar: find ↔ filter.
    pub(in crate::tui::app) fn toggle_search_mode(&mut self) {
        self.logs.search_mode = match self.logs.search_mode {
            SearchMode::Find => SearchMode::Filter,
            SearchMode::Filter => SearchMode::Find,
        };
        self.find_requery();
    }

    /// `Ctrl+R` in the search bar.
    pub(in crate::tui::app) fn toggle_search_regex(&mut self) {
        self.logs.search_regex = !self.logs.search_regex;
        self.find_requery();
    }
}
