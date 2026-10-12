//! Log pane behavior on `App`: incremental find / filter with n/N, problem
//! jumps (`]e`/`[e`), link cursor + open (`o`), view toggles (wrap,
//! timestamps, JSON), horizontal pan, marks, restart separators, new-line
//! count while paused, and per-tab activity / error badges.
//!
//! Everything that looks at the whole buffer goes through `sync_logs`, which
//! advances an incremental `ViewIndex` + `RowCache`: a frame or a keypress
//! costs O(new lines + visible rows), never O(buffer).

use crate::runtime::logindex::{Matcher, ViewKey};
use crate::runtime::loglinks::Target;
use crate::runtime::logs::{LogEntry, LogLevel, now_ms};
use crate::runtime::timerange::{self, TimeRange, Window};
use crate::tui::widgets::log_viewer::{self, Cursor, Finder, ViewOpts};

use super::{App, Effect, LevelFilter, Message, ToastVariant};

/// What the search bar does with the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Keep every line, highlight matches, `n`/`N` jump between them.
    #[default]
    Find,
    /// Show only matching lines.
    Filter,
}

impl SearchMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Find => "find",
            Self::Filter => "filter",
        }
    }
}

/// Unseen-output markers for one tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TabBadge {
    /// New output since the tab was last viewed.
    pub activity: bool,
    /// Error lines since the tab was last viewed.
    pub errors: usize,
}

/// One find hit: entry `seq` + occurrence index inside that entry.
pub type MatchPos = (u64, usize);

impl App {
    /// Compiled query (None while empty or an invalid regex).
    pub fn finder(&self) -> Option<Finder> {
        Matcher::new(&self.logs.log_search_input, self.logs.search_regex)
    }

    /// The query is non-empty but does not compile (bad regex).
    pub fn search_invalid(&self) -> bool {
        !self.logs.log_search_input.is_empty() && self.finder().is_none()
    }

    /// Find is highlighting (not filtering) with a usable query.
    pub fn find_active(&self) -> bool {
        self.logs.search_mode == SearchMode::Find && self.finder().is_some()
    }

    /// Layout options for the active tab.
    pub fn view_opts(&self) -> ViewOpts {
        ViewOpts {
            // All and the tunnel tab mix sources; a service tab is one source.
            show_service: self.active_tab == 0,
            timestamps: self.logs.log_timestamps,
            wrap: self.logs.log_wrap,
            json: self.logs.log_json_pretty,
            hscroll: self.logs.log_hscroll,
        }
    }

    /// Service whose ring backs the active tab (`None` = All).
    pub fn view_service(&self) -> Option<&str> {
        if self.active_tab == 0 {
            None
        } else {
            self.tabs.get(self.active_tab).map(String::as_str)
        }
    }

    /// Definition of the active view: any change rebuilds the index once.
    fn view_key(&self) -> ViewKey {
        let q = &self.logs.log_search_input;
        let query = (!q.is_empty()).then(|| (q.clone(), self.logs.search_regex));
        let (filter, find) = match self.logs.search_mode {
            SearchMode::Filter => (query, None),
            SearchMode::Find => (None, query),
        };
        let mut hidden: Vec<String> = if self.active_tab == 0 {
            self.manager
                .hidden_service_names()
                .into_iter()
                .map(String::from)
                .collect()
        } else {
            Vec::new()
        };
        hidden.sort();
        let window = timerange::resolve(
            self.logs.time_range,
            &self.logs.log_buffer,
            self.view_service(),
            now_ms(),
        )
        // A nonexistent local time (DST gap) resolves to nothing: show nothing.
        .unwrap_or(Window {
            min_seq: u64::MAX,
            ..Window::default()
        });
        ViewKey {
            service: self.view_service().map(String::from),
            hidden,
            min_level: match self.logs.level_filter {
                LevelFilter::All => None,
                LevelFilter::Warn => Some(LogLevel::Warn),
                LevelFilter::Error => Some(LogLevel::Error),
            },
            filter,
            find,
            json: self.logs.log_json_pretty,
            window,
        }
    }

    /// Advance the view index (and row cache, when the pane has been laid out)
    /// to the current buffer. Cheap when nothing changed.
    pub fn sync_logs(&self) {
        let key = self.view_key();
        let mut ix = self.logs.index.borrow_mut();
        ix.sync(&self.logs.log_buffer, &key);
        let vp = self.logs.log_viewport.get();
        if vp.text_width > 0 {
            self.logs.rows.borrow_mut().sync(
                &self.logs.log_buffer,
                key.service.as_deref(),
                ix.seqs(0..ix.len()),
                vp.text_width,
                &self.view_opts(),
            );
        }
    }

    /// The text width the next frame will wrap at, once known.
    pub fn sync_rows_for(&self, width: u16) {
        let key = self.view_key();
        let ix = self.logs.index.borrow();
        self.logs.rows.borrow_mut().sync(
            &self.logs.log_buffer,
            key.service.as_deref(),
            ix.seqs(0..ix.len()),
            width,
            &self.view_opts(),
        );
    }

    /// Entries of the current view, oldest first (allocates; for copy/tests —
    /// the renderer reads the index directly).
    pub fn filtered_logs(&self) -> Vec<&LogEntry> {
        self.sync_logs();
        let ix = self.logs.index.borrow();
        let svc = self.view_service();
        ix.seqs(0..ix.len())
            .filter_map(|seq| match svc {
                Some(s) => self.logs.log_buffer.get(s, seq),
                None => self.logs.log_buffer.get_any(seq),
            })
            .collect()
    }

    /// Whether `e` shows up in the current tab + level (+ filter) view. Uses
    /// the already-synced index: O(log N).
    pub(super) fn log_visible(&self, e: &LogEntry) -> bool {
        self.sync_logs();
        self.logs.index.borrow().index_of(e.seq).is_some()
    }

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
    pub(super) fn find_requery(&mut self) {
        self.sync_logs();
        let n = self.logs.index.borrow().total_hits();
        self.logs.find_current = n.saturating_sub(1);
        self.reset_log_view();
        if n > 0 {
            self.scroll_to_current_match();
        }
    }

    /// `n` / `N`: step to the next / previous hit (wraps), pausing follow.
    pub(super) fn find_step(&mut self, forward: bool) {
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
    pub(super) fn scroll_to_seq(&mut self, seq: u64) {
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
    pub(super) fn scroll_to_current_match(&mut self) {
        if let Some((seq, _)) = self.current_match() {
            self.scroll_to_seq(seq);
        }
    }

    /// `]e` / `[e` / `]w` / `[w`: nearest error (or warn+error) after / before
    /// the last jump (or the viewport when none), wrapping around.
    pub(super) fn jump_problem(&mut self, forward: bool, warn: bool) {
        self.sync_logs();
        let from = self.logs.problem_cursor.or_else(|| self.viewport_anchor());
        let target = self.logs.index.borrow().jump_level(from, forward, warn);
        let what = if warn { "warning/error" } else { "error" };
        let Some(seq) = target else {
            self.set_toast(format!("No {what} lines"), ToastVariant::Info);
            return;
        };
        self.logs.problem_cursor = Some(seq);
        self.logs.link_cursor = None;
        self.scroll_to_seq(seq);
    }

    /// Seq of the entry at the top of the viewport (jump origin when no jump
    /// has happened yet): `]e` then goes to the first problem below the top.
    fn viewport_anchor(&self) -> Option<u64> {
        let vp = self.logs.log_viewport.get();
        let ix = self.logs.index.borrow();
        let rows = self.logs.rows.borrow();
        if rows.len() != ix.len() || ix.is_empty() {
            return None;
        }
        let total = rows.total();
        let top = total.saturating_sub(self.logs.log_scroll.min(vp.max_scroll) + vp.height);
        rows.locate(top).and_then(|(i, _)| ix.seq_at(i))
    }

    /// `Tab` in the search bar: find ↔ filter.
    pub(super) fn toggle_search_mode(&mut self) {
        self.logs.search_mode = match self.logs.search_mode {
            SearchMode::Find => SearchMode::Filter,
            SearchMode::Filter => SearchMode::Find,
        };
        self.find_requery();
    }

    /// `Ctrl+R` in the search bar.
    pub(super) fn toggle_search_regex(&mut self) {
        self.logs.search_regex = !self.logs.search_regex;
        self.find_requery();
    }

    // ---- time range -------------------------------------------------------

    /// `V`: next preset; after the last one the custom prompt opens, and
    /// leaving the prompt without a value returns to all time.
    pub(super) fn cycle_time_range(&mut self) {
        self.logs.time_error = None;
        if self.logs.time_input.is_some() {
            self.time_cancel();
            return;
        }
        if self.logs.time_range == TimeRange::SinceStackStart {
            // The prompt follows the last preset; the range stays until a value is committed.
            self.logs.time_input = Some(String::new());
            return;
        }
        self.set_time_range(self.logs.time_range.next());
    }

    pub(super) fn set_time_range(&mut self, range: TimeRange) {
        if self.logs.time_range != range {
            self.logs.time_range = range;
            self.reset_log_view();
        }
    }

    pub(super) fn time_input(&mut self, c: char) {
        if let Some(t) = &mut self.logs.time_input {
            t.push(c);
            self.logs.time_error = None;
        }
    }

    pub(super) fn time_backspace(&mut self) {
        if let Some(t) = &mut self.logs.time_input {
            t.pop();
            self.logs.time_error = None;
        }
    }

    /// Enter: apply the typed range, or keep the prompt open with the reason.
    /// An empty field applies nothing and closes (back to all time).
    pub(super) fn time_commit(&mut self) {
        let Some(text) = self.logs.time_input.clone() else {
            return;
        };
        if text.trim().is_empty() {
            self.time_cancel();
            return;
        }
        match timerange::parse_custom(&text) {
            Ok(range) => {
                self.logs.time_input = None;
                self.logs.time_error = None;
                self.set_time_range(range);
            }
            Err(why) => self.logs.time_error = Some(why),
        }
    }

    /// Esc: close the prompt; the range returns to all time.
    pub(super) fn time_cancel(&mut self) {
        self.logs.time_input = None;
        self.logs.time_error = None;
        self.set_time_range(TimeRange::All);
    }

    /// Lines in the current view that arrived since follow paused.
    pub fn new_lines_since_pause(&self) -> usize {
        if self.logs.follow_logs {
            return 0;
        }
        let Some(since) = self.logs.paused_since_seq else {
            return 0;
        };
        self.sync_logs();
        let ix = self.logs.index.borrow();
        let from = ix.lower_bound(since);
        (from..ix.len())
            .filter_map(|i| ix.seq_at(i))
            .filter(|&seq| {
                self.logs
                    .log_buffer
                    .get_any(seq)
                    .is_some_and(|e| !e.is_separator())
            })
            .count()
    }

    /// `W`
    pub(super) fn toggle_wrap(&mut self) {
        self.logs.log_wrap = !self.logs.log_wrap;
        self.logs.log_hscroll = 0;
        self.save_ui_state();
        self.set_toast(
            if self.logs.log_wrap {
                "Wrap on"
            } else {
                "Wrap off — ←/→ pan"
            },
            ToastVariant::Info,
        );
    }

    /// `T`
    pub(super) fn toggle_timestamps(&mut self) {
        self.logs.log_timestamps = !self.logs.log_timestamps;
        self.save_ui_state();
    }

    /// `J`
    pub(super) fn toggle_json(&mut self) {
        self.logs.log_json_pretty = !self.logs.log_json_pretty;
        self.save_ui_state();
    }

    /// ←/→ with wrap off: pan by a third of the view.
    pub(super) fn pan_logs(&mut self, right: bool) {
        if self.logs.log_wrap {
            return;
        }
        let vp = self.logs.log_viewport.get();
        let step = (vp.text_width as usize / 3).max(1);
        self.logs.log_hscroll = if right {
            (self.logs.log_hscroll + step).min(vp.max_hscroll)
        } else {
            self.logs.log_hscroll.saturating_sub(step)
        };
    }

    /// `-`: drop a `──── HH:MM:SS mark ────` line into the current tab's
    /// stream (All → every running service tab is not touched; it marks All only
    /// in memory under the pseudo-service `atlas`).
    pub(super) fn insert_mark(&mut self) {
        let name = match self.tabs.get(self.active_tab) {
            Some(t) if self.active_tab > 0 => t.clone(),
            _ => "atlas".to_string(),
        };
        let line = self.logs.log_buffer.push_mark(name.clone());
        if self.active_tab > 0 {
            self.manager.append_log_mark(&name, &line);
        }
        if !self.logs.follow_logs {
            self.bump_paused_scroll_for_last();
        }
    }

    /// A service (re)started: drop a `restarted` separator in its stream, but
    /// only when it already printed something (a first boot has no "before").
    pub(super) fn log_service_restarted(&mut self, name: &str) {
        if self.logs.log_buffer.filter_service(name).is_empty() {
            return;
        }
        // Disk logs already separate runs (`<svc>.log` vs `<svc>.prev.log`).
        self.logs.log_buffer.push_restart(name.to_string());
        if !self.logs.follow_logs {
            self.bump_paused_scroll_for_last();
        }
    }

    /// Paused view: grow the offset by the newest entry's rows so the content stays put.
    pub(super) fn bump_paused_scroll_for_last(&mut self) {
        let Some(entry) = self.logs.log_buffer.last() else {
            return;
        };
        if self.log_visible(entry) {
            let width = self.logs.log_viewport.get().text_width;
            let rows = log_viewer::entry_rows(entry, width, &self.view_opts());
            self.logs.log_scroll += rows;
        }
    }

    /// Record that the active tab was viewed now (clears its badge).
    pub(super) fn mark_tab_seen(&mut self) {
        let seq = self.logs.log_buffer.next_seq();
        if let Some(name) = self.tabs.get(self.active_tab).cloned() {
            self.logs.tab_seen.insert(name, seq);
        }
    }

    /// Unseen-output badge per tab (same indexing as `tabs`). The active tab
    /// never shows one: you are looking at it.
    pub fn tab_badges(&self) -> Vec<TabBadge> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, name)| {
                if i == self.active_tab {
                    return TabBadge::default();
                }
                let seen = self.logs.tab_seen.get(name).copied().unwrap_or(0);
                let (mut activity, mut errors) = (false, 0usize);
                // Only entries newer than `seen`: walk each ring from its
                // newest end and stop at the first old one.
                self.logs
                    .log_buffer
                    .for_each_from(seen, (i != 0).then_some(name.as_str()), |e| {
                        if !e.is_separator() {
                            activity = true;
                            if e.level == LogLevel::Error {
                                errors += 1;
                            }
                        }
                    });
                TabBadge { activity, errors }
            })
            .collect()
    }

    /// Click on a tab's `✗N` badge: open it filtered to errors.
    pub(super) fn open_tab_errors(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active_tab = idx;
            self.logs.level_filter = LevelFilter::Error;
            self.reset_log_view();
            self.save_ui_state();
        }
    }

    // ---- links ------------------------------------------------------------

    /// Entry the link cursor starts from: the one under the viewport's
    /// bottom row (what the eye is on while following).
    fn link_anchor_entry(&self) -> Option<u64> {
        self.sync_logs();
        let vp = self.logs.log_viewport.get();
        let ix = self.logs.index.borrow();
        let rows = self.logs.rows.borrow();
        if rows.len() != ix.len() || ix.is_empty() {
            return None;
        }
        let bottom = rows
            .total()
            .saturating_sub(self.logs.log_scroll.min(vp.max_scroll) + 1);
        rows.locate(bottom).and_then(|(i, _)| ix.seq_at(i))
    }

    /// `Tab` on the dashboard log pane: next/prev link, walking entries from
    /// the newest visible one upward (forward = toward newer).
    pub(super) fn step_link(&mut self, forward: bool) {
        self.sync_logs();
        let json = self.logs.log_json_pretty;
        let start = match &self.logs.link_cursor {
            Some(c) => c.clone(),
            None => {
                let Some(seq) = self.link_anchor_entry() else {
                    return;
                };
                Cursor {
                    seq,
                    link: usize::MAX,
                }
            }
        };
        let found = {
            let ix = self.logs.index.borrow();
            let Some(mut i) = ix.index_of(start.seq) else {
                return;
            };
            let at = |seq: u64| self.logs.log_buffer.get_any(seq);
            let mut link = start.link;
            let mut result = None;
            // Within the starting entry first, then outward, bounded so a
            // link-less buffer does not scan everything per keypress.
            for step in 0..4096 {
                if let Some(e) = ix.seq_at(i).and_then(at) {
                    let n = log_viewer::entry_links(e, json).len();
                    let candidate = match (forward, link) {
                        (true, usize::MAX) => (n > 0).then_some(0),
                        (true, l) => (l + 1 < n).then_some(l + 1),
                        (false, usize::MAX) => n.checked_sub(1),
                        (false, l) => l.checked_sub(1),
                    };
                    // First visit of the start entry with an existing cursor
                    // continues from it; later entries start at their edge.
                    if let Some(l) = candidate
                        && !(step == 0 && start.link == usize::MAX && !forward && n == 0)
                    {
                        result = Some(Cursor {
                            seq: e.seq,
                            link: l,
                        });
                        break;
                    }
                }
                // Move to the neighbouring entry; entering it from its far edge.
                if forward {
                    i += 1;
                    if i >= ix.len() {
                        break;
                    }
                    link = usize::MAX;
                } else {
                    if i == 0 {
                        break;
                    }
                    i -= 1;
                    link = usize::MAX;
                }
            }
            result
        };
        match found {
            Some(c) => {
                let seq = c.seq;
                self.logs.link_cursor = Some(c);
                self.scroll_to_seq(seq);
            }
            None => self.set_toast("No links in view", ToastVariant::Info),
        }
    }

    /// The link under the cursor, resolved against the project root.
    fn cursor_target(&self) -> Option<Target> {
        let c = self.logs.link_cursor.as_ref()?;
        let e = self.logs.log_buffer.get_any(c.seq)?;
        let l = log_viewer::entry_links(e, self.logs.log_json_pretty)
            .into_iter()
            .nth(c.link)?;
        Some(l.target)
    }

    /// `o`: open the focused link without blocking the event loop.
    pub(super) fn open_link(&mut self) {
        let Some(target) = self.cursor_target() else {
            self.set_toast("Tab selects a link first", ToastVariant::Info);
            return;
        };
        match target {
            Target::Url(u) => self.effects.push_back(Effect::OpenUrl(u)),
            Target::Path { path, line, col } => {
                match crate::runtime::loglinks::resolve(&self.manager.root_dir, &path) {
                    Some(full) => self.effects.push_back(Effect::OpenPath {
                        path: full.to_string_lossy().into_owned(),
                        line,
                        col,
                    }),
                    // Not a file we can find: the next best thing is the text.
                    None => self.copy_text(
                        format!("{path}:{line}"),
                        format!("{path} not found — copied {path}:{line}"),
                    ),
                }
            }
        }
    }

    /// `y`: copy the focused link's text.
    pub(super) fn copy_link(&mut self) {
        let Some(target) = self.cursor_target() else {
            self.set_toast("Tab selects a link first", ToastVariant::Info);
            return;
        };
        let text = match target {
            Target::Url(u) => u,
            Target::Path { path, line, col } => match col {
                Some(c) => format!("{path}:{line}:{c}"),
                None => format!("{path}:{line}"),
            },
        };
        self.copy_text(text.clone(), format!("Copied {text}"));
    }

    /// Search-bar key → message (used by input.rs while `log_searching`).
    pub(super) fn map_search_key(&self, key: crossterm::event::KeyEvent) -> Option<Message> {
        use crossterm::event::{KeyCode, KeyModifiers};
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => Some(Message::LogSearchClear),
            KeyCode::Enter => Some(Message::LogSearchCommit),
            KeyCode::Backspace => Some(Message::LogSearchBackspace),
            KeyCode::Tab => Some(Message::ToggleSearchMode),
            KeyCode::Char('r') if ctrl => Some(Message::ToggleSearchRegex),
            KeyCode::Char('c') if ctrl => Some(Message::Quit),
            KeyCode::Char(_) if ctrl || key.modifiers.contains(KeyModifiers::ALT) => None,
            KeyCode::Char(c) => Some(Message::LogSearchInput(c)),
            _ => None,
        }
    }

    /// Custom time-range prompt key → message (used by input.rs while the
    /// prompt is open; it owns the keyboard like the search bar).
    pub(super) fn map_time_key(&self, key: crossterm::event::KeyEvent) -> Option<Message> {
        use crossterm::event::{KeyCode, KeyModifiers};
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => Some(Message::TimeCancel),
            KeyCode::Enter => Some(Message::TimeCommit),
            KeyCode::Backspace => Some(Message::TimeBackspace),
            KeyCode::Char('c') if ctrl => Some(Message::Quit),
            KeyCode::Char(_) if ctrl || key.modifiers.contains(KeyModifiers::ALT) => None,
            KeyCode::Char(c) => Some(Message::TimeInput(c)),
            _ => None,
        }
    }
}

impl App {
    /// Dashboard keys that drive the log pane. `n`/`N` only count while a find
    /// query is active (otherwise they fall through to open_key/action bindings).
    pub(super) fn log_view_key(&self, c: char) -> Option<Message> {
        match c {
            'n' if self.find_active() => Some(Message::FindNext),
            'N' if self.find_active() => Some(Message::FindPrev),
            'W' => Some(Message::ToggleWrap),
            'T' => Some(Message::ToggleTimestamps),
            'V' => Some(Message::CycleTimeRange),
            'J' => Some(Message::ToggleJson),
            '-' => Some(Message::InsertMark),
            'z' => Some(Message::ToggleZoom),
            'o' if self.logs.link_cursor.is_some() => Some(Message::OpenLink),
            'y' if self.logs.link_cursor.is_some() => Some(Message::CopyLink),
            ']' | '[' => Some(Message::ChordStart(c)),
            '}' => Some(Message::StepLink { forward: true }),
            '{' => Some(Message::StepLink { forward: false }),
            _ => None,
        }
    }

    /// Second key of a `]` / `[` chord: `e` errors, `w` warnings+errors.
    pub(super) fn chord_key(&self, first: char, c: char) -> Option<Message> {
        let forward = first == ']';
        match c {
            'e' => Some(Message::JumpProblem {
                forward,
                warn: false,
            }),
            'w' => Some(Message::JumpProblem {
                forward,
                warn: true,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProjectConfig, ServiceDef};
    use crate::event::EventHandler;
    use crate::runtime::logs::LogStream;
    use crate::runtime::manager::ServiceManager;
    use crate::tui::widgets::log_viewer::LogViewport;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tokio::sync::mpsc;

    fn make_app() -> App {
        let (tx, rx) = mpsc::unbounded_channel();
        let cfg = ProjectConfig {
            name: "test".to_string(),
            services: HashMap::from([(
                "web".to_string(),
                ServiceDef {
                    command: Some("echo hello".to_string()),
                    port: Some(3000),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let mgr = ServiceManager::new(PathBuf::from("/tmp/atlas-logview-test"), &cfg, tx);
        let (_, events) = EventHandler::test_channel();
        let mut app = App::new(cfg, mgr, rx, events, false);
        app.tabs.push("api".into());
        app.logs.log_viewport.set(LogViewport {
            text_width: 80,
            height: 5,
            max_scroll: 1000,
            max_hscroll: 100,
        });
        app
    }

    fn line(app: &mut App, svc: &str, text: &str) {
        app.update(Message::LogLineFrom {
            name: svc.into(),
            line: text.into(),
            stream: LogStream::Stdout,
        });
    }

    fn type_query(app: &mut App, q: &str) {
        app.update(Message::StartLogSearch);
        for c in q.chars() {
            app.update(Message::LogSearchInput(c));
        }
    }

    #[test]
    fn find_counts_and_n_n_wrap_around_and_pause_follow() {
        let mut app = make_app();
        for i in 0..40 {
            line(
                &mut app,
                "web",
                &format!("row {i} {}", if i % 10 == 0 { "needle" } else { "hay" }),
            );
        }
        type_query(&mut app, "needle");
        // 4 hits; incremental find lands on the newest (4/4), every line stays visible.
        assert_eq!(app.filtered_logs().len(), 40);
        assert_eq!(app.find_counts(), (4, 4));
        app.update(Message::LogSearchCommit);

        app.update(Message::FindNext);
        assert_eq!(app.find_counts(), (1, 4), "n wraps to the first hit");
        assert!(
            !app.logs.follow_logs && app.logs.log_scroll > 0,
            "jump pauses follow"
        );
        app.update(Message::FindNext);
        assert_eq!(app.find_counts(), (2, 4));
        app.update(Message::FindPrev);
        app.update(Message::FindPrev);
        assert_eq!(app.find_counts(), (4, 4), "N wraps backwards");
        // The newest hit is 9 rows above the bottom of a 5-row view: it stays paused there.
        assert!(app.logs.log_scroll > 0 && !app.logs.follow_logs);
        assert_eq!(app.new_lines_since_pause(), 0);
        // Hit that already fits on the bottom screen resumes follow.
        let mut short = make_app();
        line(&mut short, "web", "needle");
        type_query(&mut short, "needle");
        short.update(Message::FindNext);
        assert_eq!(short.logs.log_scroll, 0);
        assert!(short.logs.follow_logs);
    }

    #[test]
    fn n_key_only_maps_while_find_is_active() {
        let mut app = make_app();
        assert!(app.log_view_key('n').is_none());
        line(&mut app, "web", "needle");
        type_query(&mut app, "needle");
        assert!(matches!(app.log_view_key('n'), Some(Message::FindNext)));
        assert!(matches!(app.log_view_key('N'), Some(Message::FindPrev)));
        // Filter mode has no n/N.
        app.update(Message::ToggleSearchMode);
        assert!(app.log_view_key('n').is_none());
    }

    #[test]
    fn filter_mode_shows_only_matching_lines_and_tab_toggles() {
        let mut app = make_app();
        line(&mut app, "web", "alpha");
        line(&mut app, "web", "beta");
        line(&mut app, "web", "alphabet");
        type_query(&mut app, "alpha");
        assert_eq!(app.filtered_logs().len(), 3, "find keeps everything");
        app.update(Message::ToggleSearchMode);
        assert_eq!(app.logs.search_mode, SearchMode::Filter);
        let c: Vec<_> = app
            .filtered_logs()
            .iter()
            .map(|e| e.content.clone())
            .collect();
        assert_eq!(c, ["alpha", "alphabet"]);
        assert_eq!(app.find_counts().1, 2);
    }

    #[test]
    fn smartcase_and_regex_toggle_change_matches() {
        let mut app = make_app();
        line(&mut app, "web", "Error: boom");
        line(&mut app, "web", "error: bang");
        app.update(Message::ToggleSearchMode); // filter mode for easy counting
        type_query(&mut app, "error");
        assert_eq!(app.filtered_logs().len(), 2, "lowercase ⇒ insensitive");
        type_query(&mut app, "Error");
        assert_eq!(app.filtered_logs().len(), 1, "uppercase ⇒ sensitive");
        type_query(&mut app, r"b(oo|an)\w");
        assert_eq!(app.filtered_logs().len(), 0, "literal text, no regex yet");
        app.update(Message::ToggleSearchRegex);
        assert_eq!(app.filtered_logs().len(), 2);
        // Invalid regex ⇒ flagged, nothing shown.
        type_query(&mut app, "(");
        assert!(app.search_invalid());
        assert!(app.filtered_logs().is_empty());
    }

    #[test]
    fn paused_chip_counts_new_lines_in_current_tab_only() {
        let mut app = make_app();
        line(&mut app, "web", "one");
        app.update(Message::ToggleFollow); // pause
        assert!(!app.logs.follow_logs);
        assert_eq!(app.new_lines_since_pause(), 0);
        line(&mut app, "web", "two");
        line(&mut app, "web", "three");
        line(&mut app, "api", "other tab on All still counts");
        assert_eq!(app.new_lines_since_pause(), 3);
        app.update(Message::SwitchTab(1)); // web tab (resets view)
        assert_eq!(app.new_lines_since_pause(), 0, "follow resumed");
        app.update(Message::ToggleFollow);
        line(&mut app, "api", "not in web tab");
        assert_eq!(app.new_lines_since_pause(), 0);
        app.update(Message::ScrollToBottom);
        assert!(app.logs.follow_logs && app.logs.paused_since_seq.is_none());
    }

    #[test]
    fn wrap_off_pans_with_arrows_and_resets_on_toggle() {
        let mut app = make_app();
        app.update(Message::PanLogs { right: true });
        assert_eq!(app.logs.log_hscroll, 0, "wrap on: no pan");
        app.update(Message::ToggleWrap);
        assert!(!app.logs.log_wrap);
        app.update(Message::PanLogs { right: true });
        assert_eq!(app.logs.log_hscroll, 80 / 3);
        for _ in 0..20 {
            app.update(Message::PanLogs { right: true });
        }
        assert_eq!(app.logs.log_hscroll, 100, "clamped to the widest line");
        app.update(Message::PanLogs { right: false });
        assert!(app.logs.log_hscroll < 100);
        app.update(Message::ToggleWrap);
        assert_eq!(app.logs.log_hscroll, 0);
    }

    #[test]
    fn single_service_tab_drops_prefix() {
        let mut app = make_app();
        assert!(app.view_opts().show_service);
        app.update(Message::SwitchTab(1));
        assert!(!app.view_opts().show_service);
    }

    #[test]
    fn mark_goes_into_current_tab_stream() {
        let mut app = make_app();
        app.update(Message::SwitchTab(1));
        app.update(Message::InsertMark);
        let logs = app.filtered_logs();
        assert_eq!(logs.len(), 1);
        assert!(logs[0].mark && logs[0].content.contains(" mark ────"));
        assert_eq!(logs[0].service, "web");
        // The mark never counts as an error / badge.
        assert!(app.logs.log_buffer.errors().is_empty());
    }

    #[test]
    fn badges_for_unseen_output_and_errors_reset_on_open() {
        let mut app = make_app();
        // Active tab All: web/api are unfocused.
        line(&mut app, "web", "all quiet");
        line(&mut app, "api", "error: boom");
        line(&mut app, "api", "fatal: again");
        let b = app.tab_badges();
        assert_eq!(b[0], TabBadge::default(), "active tab shows no badge");
        assert_eq!(
            b[1],
            TabBadge {
                activity: true,
                errors: 0
            }
        );
        assert_eq!(
            b[2],
            TabBadge {
                activity: true,
                errors: 2
            }
        );
        // Opening the tab clears it; switching away shows only newer news.
        app.update(Message::SwitchTab(2));
        assert_eq!(app.tab_badges()[2], TabBadge::default());
        app.update(Message::SwitchTab(0));
        assert_eq!(app.tab_badges()[2], TabBadge::default(), "nothing new yet");
        line(&mut app, "api", "error: third");
        assert_eq!(
            app.tab_badges()[2],
            TabBadge {
                activity: true,
                errors: 1
            }
        );
        // Click on the badge: tab opens filtered to errors.
        app.update(Message::OpenTabErrors(2));
        assert_eq!(app.active_tab, 2);
        assert_eq!(app.logs.level_filter, LevelFilter::Error);
        assert_eq!(app.tab_badges()[2], TabBadge::default());
    }

    #[test]
    fn json_toggle_changes_what_find_searches() {
        let mut app = make_app();
        line(
            &mut app,
            "web",
            r#"{"level":30,"msg":"hello","user":"ana"}"#,
        );
        app.update(Message::ToggleSearchMode);
        type_query(&mut app, "user=ana");
        assert!(app.filtered_logs().is_empty(), "raw JSON has no `user=ana`");
        app.update(Message::ToggleJson);
        app.update(Message::LogSearchInput('a')); // re-run the query
        app.update(Message::LogSearchBackspace);
        assert_eq!(app.filtered_logs().len(), 1);
    }

    fn screen(app: &App, w: u16, h: u16) -> Vec<String> {
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        term.draw(|f| app.view(f)).unwrap();
        let buf = term.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect())
            .collect()
    }

    #[test]
    fn full_render_shows_bar_counts_empty_state_and_badges() {
        let mut app = make_app();
        app.splash_done = true;
        app.size = (160, 40);
        for i in 0..6 {
            line(&mut app, "web", &format!("hello {i}"));
        }
        line(&mut app, "api", "error: kaput");
        type_query(&mut app, "hello");
        let rows = screen(&app, 160, 40).join("\n");
        assert!(rows.contains("find") && rows.contains("6/6"), "{rows}");
        assert!(rows.contains("n/N") || rows.contains("Tab find/filter"));
        // Unfocused api tab carries the error badge, web the activity marker.
        assert!(rows.contains("✗1"), "{rows}");
        assert!(rows.contains('◆'), "{rows}");

        // Filter + no hit ⇒ centered empty state naming the tab.
        app.update(Message::ToggleSearchMode);
        app.update(Message::LogSearchInput('x'));
        let rows = screen(&app, 160, 40).join("\n");
        assert!(rows.contains("0 matches in All"), "{rows}");

        // Toggles are echoed on the bar.
        app.update(Message::LogSearchClear);
        app.update(Message::ToggleWrap);
        app.update(Message::ToggleTimestamps);
        app.update(Message::ToggleJson);
        let rows = screen(&app, 160, 40).join("\n");
        assert!(
            rows.contains("wrap off") && rows.contains("no ts") && rows.contains("json"),
            "{rows}"
        );
        // Clean state: bar disappears.
        app.update(Message::ToggleWrap);
        app.update(Message::ToggleTimestamps);
        app.update(Message::ToggleJson);
        let rows = screen(&app, 160, 40).join("\n");
        assert!(!rows.contains("wrap off") && !rows.contains("no ts"));
    }

    #[test]
    fn badge_click_opens_tab_filtered_to_errors() {
        let mut app = make_app();
        app.splash_done = true;
        line(&mut app, "api", "error: kaput");
        let _ = screen(&app, 160, 40); // populate click map
        let hit = app
            .clicks
            .targets
            .borrow()
            .iter()
            .find(|(_, t)| {
                matches!(
                    t,
                    crate::tui::clicks::Target::Click(Message::OpenTabErrors(2))
                )
            })
            .map(|(r, _)| *r)
            .expect("badge click target");
        app.update(Message::MouseClick { x: hit.x, y: hit.y });
        assert_eq!(app.active_tab, 2);
        assert_eq!(app.logs.level_filter, LevelFilter::Error);
    }

    fn lines(app: &mut App, svc: &str, texts: &[&str]) {
        for t in texts {
            line(app, svc, t);
        }
    }

    #[test]
    fn problem_jumps_walk_errors_then_wrap_and_warn_variant_includes_warnings() {
        let mut app = make_app();
        lines(
            &mut app,
            "web",
            &[
                "ok",
                "ERROR one",
                "ok",
                "WARN two",
                "ok",
                "ERROR three",
                "ok",
            ],
        );
        let (e1, w, e3) = (1u64, 3u64, 5u64);
        let at = |app: &App| app.logs.problem_cursor;
        let jump = |app: &mut App, forward, warn| {
            app.update(Message::JumpProblem { forward, warn });
        };
        // From an explicit position: forward walks e1 → e3 → wraps to e1.
        app.logs.problem_cursor = Some(e1);
        jump(&mut app, true, false);
        assert_eq!(at(&app), Some(e3), "skips the warning when errors only");
        jump(&mut app, true, false);
        assert_eq!(at(&app), Some(e1), "wraps to the first error");
        jump(&mut app, false, false);
        assert_eq!(at(&app), Some(e3), "[e wraps backwards");
        // warn+error also stops on the warning between them, both directions.
        app.logs.problem_cursor = Some(e1);
        jump(&mut app, true, true);
        assert_eq!(at(&app), Some(w));
        jump(&mut app, true, true);
        assert_eq!(at(&app), Some(e3));
        jump(&mut app, false, true);
        assert_eq!(at(&app), Some(w));
        // The jump pauses follow only when the target is off the bottom screen;
        // with a 5-row view an early error is.
        app.logs.problem_cursor = Some(e3);
        jump(&mut app, true, false);
        assert_eq!(at(&app), Some(e1));
        assert!(app.logs.log_scroll > 0 && !app.logs.follow_logs);
        // With no cursor yet, the first `]e` starts from the viewport, not the buffer start.
        let mut fresh = make_app();
        lines(
            &mut fresh,
            "web",
            &["ERROR a", "ok", "ok", "ok", "ok", "ok", "ok", "ERROR b"],
        );
        fresh.update(Message::JumpProblem {
            forward: true,
            warn: false,
        });
        assert!(fresh.logs.problem_cursor.is_some());
    }

    #[test]
    fn problem_jump_with_none_toasts_instead_of_moving() {
        let mut app = make_app();
        lines(&mut app, "web", &["fine", "still fine"]);
        app.update(Message::JumpProblem {
            forward: true,
            warn: false,
        });
        assert!(app.logs.problem_cursor.is_none());
        assert!(
            app.toast
                .as_ref()
                .is_some_and(|(m, _, _)| m.contains("No error"))
        );
    }

    #[test]
    fn chord_keys_map_only_after_a_bracket() {
        let mut app = make_app();
        assert!(matches!(
            app.log_view_key(']'),
            Some(Message::ChordStart(']'))
        ));
        assert!(matches!(
            app.chord_key(']', 'e'),
            Some(Message::JumpProblem {
                forward: true,
                warn: false
            })
        ));
        assert!(matches!(
            app.chord_key('[', 'w'),
            Some(Message::JumpProblem {
                forward: false,
                warn: true
            })
        ));
        assert!(app.chord_key(']', 'x').is_none());
        // Pressing ] then e through the real key path.
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let key = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        let m = app.map_key(key(']')).unwrap();
        app.update(m);
        assert_eq!(app.logs.chord, Some(']'));
        let m = app.map_key(key('e')).unwrap();
        assert!(matches!(
            m,
            Message::JumpProblem {
                forward: true,
                warn: false
            }
        ));
        app.update(m);
        assert!(app.logs.chord.is_none());
        // A non-chord key cancels instead of acting.
        let m = app.map_key(key(']')).unwrap();
        app.update(m);
        let m = app.map_key(key('q')).unwrap();
        app.update(m);
        assert!(app.logs.chord.is_none());
        assert!(
            !matches!(app.layer, crate::tui::app::Layer::Quit),
            "q was swallowed by the cancel"
        );
    }

    #[test]
    fn restart_event_inserts_a_separator_only_after_prior_output() {
        let mut app = make_app();
        app.apply_manager_event(crate::runtime::manager::ManagerEvent::ServiceRestarted {
            name: "web".into(),
        });
        assert!(app.logs.log_buffer.is_empty(), "first boot has no 'before'");
        line(&mut app, "web", "run 1");
        app.apply_manager_event(crate::runtime::manager::ManagerEvent::ServiceRestarted {
            name: "web".into(),
        });
        line(&mut app, "web", "run 2");
        let v = app.filtered_logs();
        assert_eq!(v.len(), 3);
        assert!(v[1].restart && v[1].content.contains("restarted"));
        // Separators neither count as errors nor as paused "new lines".
        assert!(app.logs.log_buffer.errors().is_empty());
    }

    #[test]
    fn link_cursor_steps_across_entries_and_copies_the_target() {
        let mut app = make_app();
        lines(
            &mut app,
            "web",
            &[
                "no link",
                "see https://a.test/x",
                "plain",
                "src/m.rs:10:2 broke",
            ],
        );
        // Newest-first from the bottom: `{` selects the last link, `{` again the earlier one.
        app.update(Message::StepLink { forward: false });
        let c = app.logs.link_cursor.clone().unwrap();
        assert_eq!(
            app.logs.log_buffer.get_any(c.seq).unwrap().content,
            "src/m.rs:10:2 broke"
        );
        app.update(Message::StepLink { forward: false });
        let c2 = app.logs.link_cursor.clone().unwrap();
        assert!(
            app.logs
                .log_buffer
                .get_any(c2.seq)
                .unwrap()
                .content
                .contains("https://a.test/x")
        );
        app.update(Message::StepLink { forward: true });
        assert_eq!(
            app.logs.link_cursor.clone().unwrap(),
            c,
            "stepping forward returns toward newer"
        );
        // Copy queues the exact text.
        app.update(Message::CopyLink);
        let queued = format!("{:?}", app.effects.back());
        assert!(queued.contains("src/m.rs:10:2"), "{queued}");
        // `o` on a URL queues an open effect, not a copy.
        app.logs.link_cursor = Some(c2);
        app.update(Message::OpenLink);
        assert!(format!("{:?}", app.effects.back()).contains("OpenUrl"));
    }

    #[test]
    fn open_of_a_missing_path_falls_back_to_copying_it() {
        let mut app = make_app();
        line(&mut app, "web", "boom at ghost/none.rs:7");
        app.update(Message::StepLink { forward: false });
        app.update(Message::OpenLink);
        let queued = format!("{:?}", app.effects.back());
        assert!(
            queued.contains("CopyToClipboard") && queued.contains("ghost/none.rs:7"),
            "{queued}"
        );
    }

    #[test]
    fn link_keys_only_fire_with_a_cursor() {
        let mut app = make_app();
        assert!(app.log_view_key('o').is_none() && app.log_view_key('y').is_none());
        line(&mut app, "web", "https://a.test");
        app.update(Message::StepLink { forward: false });
        assert!(matches!(app.log_view_key('o'), Some(Message::OpenLink)));
        assert!(matches!(app.log_view_key('y'), Some(Message::CopyLink)));
    }

    #[test]
    fn index_stays_incremental_while_lines_arrive_during_find() {
        let mut app = make_app();
        type_query(&mut app, "needle");
        for i in 0..500 {
            line(
                &mut app,
                "web",
                &format!("row {i} {}", if i % 100 == 0 { "needle" } else { "" }),
            );
        }
        assert_eq!(app.find_counts().1, 5);
        // Pushing more lines advances the same index (no rebuild): totals follow.
        line(&mut app, "web", "needle again");
        assert_eq!(app.find_counts().1, 6);
        // Clearing resets everything.
        app.update(Message::ClearLogs);
        assert_eq!(app.find_counts(), (0, 0));
    }

    #[test]
    fn level_filter_keeps_markers_and_changes_the_view_without_stale_rows() {
        let mut app = make_app();
        lines(&mut app, "web", &["ok", "ERROR x", "ok"]);
        app.update(Message::CycleLevelFilter); // warn+
        assert_eq!(app.filtered_logs().len(), 1);
        app.update(Message::CycleLevelFilter);
        app.update(Message::CycleLevelFilter); // back to all
        assert_eq!(app.filtered_logs().len(), 3);
    }

    fn press(app: &mut App, c: char) {
        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(c),
            crossterm::event::KeyModifiers::NONE,
        );
        if let Some(msg) = app.map_key(key) {
            app.update(msg);
        }
    }

    #[test]
    fn v_cycles_presets_then_opens_the_custom_prompt_then_returns_to_all() {
        let mut app = make_app();
        let mut seen = vec![];
        for _ in 0..6 {
            press(&mut app, 'V');
            seen.push(app.logs.time_range.label());
        }
        assert_eq!(
            seen,
            [
                "last 1m",
                "last 5m",
                "last 15m",
                "last 1h",
                "since restart",
                "since start"
            ]
        );
        assert!(app.logs.time_input.is_none());
        press(&mut app, 'V');
        assert!(
            app.logs.time_input.is_some(),
            "prompt follows the last preset"
        );
        // While the prompt is open keys are text, not commands.
        press(&mut app, 'v');
        assert_eq!(app.logs.time_input.as_deref(), Some("v"));
        assert_eq!(app.logs.level_filter, LevelFilter::All);
        app.update(Message::TimeCancel);
        assert!(app.logs.time_input.is_none());
        assert!(app.logs.time_range.is_all());
    }

    #[test]
    fn custom_prompt_applies_valid_input_and_keeps_the_prompt_on_invalid() {
        let mut app = make_app();
        lines(&mut app, "web", &["one", "two"]);
        app.logs.time_range = TimeRange::SinceStackStart;
        press(&mut app, 'V');
        for c in "25:99".chars() {
            app.update(Message::TimeInput(c));
        }
        app.update(Message::TimeCommit);
        assert!(
            app.logs.time_input.is_some(),
            "invalid input keeps the prompt"
        );
        assert!(app.logs.time_error.is_some());
        assert_eq!(
            app.logs.time_range,
            TimeRange::SinceStackStart,
            "range untouched"
        );
        // Typing clears the error; a valid range closes the prompt and applies.
        for _ in 0..5 {
            app.update(Message::TimeBackspace);
        }
        assert!(app.logs.time_error.is_none());
        for c in "00:00-23:59".chars() {
            app.update(Message::TimeInput(c));
        }
        app.update(Message::TimeCommit);
        assert!(app.logs.time_input.is_none());
        assert_eq!(app.logs.time_range.label(), "00:00–23:59");
        assert_eq!(app.filtered_logs().len(), 2, "today's whole day shows both");
    }

    #[test]
    fn custom_range_in_the_past_hides_fresh_lines_and_shows_the_empty_state() {
        let mut app = make_app();
        app.splash_done = true;
        app.size = (160, 40);
        lines(&mut app, "web", &["one", "two"]);
        // A one-second window that ended at 00:00:00 today cannot hold "now"
        // unless the test runs in that very second.
        app.logs.time_range = TimeRange::Custom {
            from_s: 0,
            to_s: Some(0),
        };
        if crate::runtime::timerange::seconds_of_day(crate::runtime::logs::now_ms()) > 2 {
            assert!(app.filtered_logs().is_empty());
            let text = screen(&app, 160, 40).join("\n");
            assert!(text.contains("0 matches in"), "{text}");
            assert!(text.contains("time: 00:00–00:00"), "{text}");
        }
    }

    #[test]
    fn since_restart_on_a_service_tab_and_on_all_follow_their_own_markers() {
        let mut app = make_app();
        line(&mut app, "web", "web old");
        line(&mut app, "api", "api old");
        for name in ["web", "api"] {
            app.apply_manager_event(crate::runtime::manager::ManagerEvent::ServiceRestarted {
                name: name.into(),
            });
        }
        line(&mut app, "web", "web new");
        app.apply_manager_event(crate::runtime::manager::ManagerEvent::ServiceRestarted {
            name: "web".into(),
        });
        line(&mut app, "web", "web newest");
        line(&mut app, "api", "api new");
        app.logs.time_range = TimeRange::SinceRestart;
        let texts = |app: &App| {
            app.filtered_logs()
                .iter()
                .filter(|e| !e.is_separator())
                .map(|e| e.content.clone())
                .collect::<Vec<_>>()
        };
        // All: from the newest restart of any service (web's second).
        assert_eq!(texts(&app), ["web newest", "api new"]);
        // api tab: from api's own marker.
        app.active_tab = app.tabs.iter().position(|t| t == "api").unwrap();
        assert_eq!(texts(&app), ["api new"]);
        // web tab: from web's second restart.
        app.active_tab = app.tabs.iter().position(|t| t == "web").unwrap();
        assert_eq!(texts(&app), ["web newest"]);
        // A restart that happens while the range is active moves the window.
        app.apply_manager_event(crate::runtime::manager::ManagerEvent::ServiceRestarted {
            name: "web".into(),
        });
        assert!(texts(&app).is_empty());
        line(&mut app, "web", "web fresh");
        assert_eq!(texts(&app), ["web fresh"]);
    }

    #[test]
    fn range_composes_with_level_and_filter_and_badge_shows_next_to_level() {
        let mut app = make_app();
        app.splash_done = true;
        app.size = (160, 40);
        lines(
            &mut app,
            "web",
            &["ok one", "ERROR boom", "WARN odd", "ok two"],
        );
        app.logs.time_range = TimeRange::LastMinutes(5);
        app.update(Message::CycleLevelFilter); // warn+
        assert_eq!(app.filtered_logs().len(), 2);
        app.update(Message::CycleLevelFilter); // error
        assert_eq!(app.filtered_logs().len(), 1);
        let bar = screen(&app, 160, 40).join("\n");
        let (level, time) = (bar.find("level: error"), bar.find("time: last 5m"));
        assert!(level.is_some() && time.is_some(), "{bar}");
        assert!(level < time, "range badge sits after the level badge");
        // Clearing the range removes the badge and keeps the level filter.
        app.logs.time_range = TimeRange::All;
        assert!(!screen(&app, 160, 40).join("\n").contains("time:"));
        assert_eq!(app.filtered_logs().len(), 1);
    }

    #[test]
    fn sliding_window_ages_lines_out_without_new_output() {
        let mut app = make_app();
        lines(&mut app, "web", &["a", "b"]);
        app.logs.time_range = TimeRange::LastMinutes(1);
        assert_eq!(app.filtered_logs().len(), 2);
        // Two minutes later (same buffer, later clock) the index drops them
        // from the front without rescanning.
        let key = app.view_key();
        let mut ix = app.logs.index.borrow_mut();
        ix.sync_at(&app.logs.log_buffer, &key, now_ms() + 120_000);
        assert_eq!(ix.len(), 0);
    }

    /// Release bench: `cargo test -p atlas-tui --release -- --ignored --nocapture bench_one_million`
    #[test]
    #[ignore = "benchmark: run with --release --ignored --nocapture"]
    fn bench_one_million_lines_ingest_filter_search() {
        use crate::runtime::logindex::{ViewIndex, ViewKey};
        use crate::runtime::logs::{LogBuffer, LogStream};
        use std::time::Instant;
        const N: usize = 1_000_000;
        let samples = [
            "GET /api/users/42 200 12ms",
            "INFO request completed id=abc123 user=ana",
            "WARN slow query took 840ms table=orders",
            "ERROR connection refused 127.0.0.1:5432",
            "\x1b[32m✓\x1b[0m compiled src/main.rs:12:5 in 0.3s",
        ];
        // 8 services, byte budget small enough that eviction is exercised.
        let mut buf = LogBuffer::with_budget(200_000, 8 * 1024 * 1024);
        let svc: Vec<String> = (0..8).map(|i| format!("svc{i}")).collect();
        let t = Instant::now();
        for i in 0..N {
            buf.push(
                svc[i % 8].clone(),
                format!("{} #{i}", samples[i % samples.len()]),
                LogStream::Stdout,
            );
        }
        let ingest = t.elapsed();

        let key = |filter: Option<&str>, find: Option<&str>| ViewKey {
            service: None,
            hidden: vec![],
            min_level: None,
            filter: filter.map(|q| (q.to_string(), false)),
            find: find.map(|q| (q.to_string(), false)),
            json: false,
            window: Default::default(),
        };
        let mut ix = ViewIndex::new();
        let t = Instant::now();
        ix.sync(&buf, &key(None, Some("refused")));
        let search_full = t.elapsed();
        let hits = ix.total_hits();
        // Appending 10k more only scans the new lines.
        for i in 0..10_000 {
            buf.push(
                "svc0".into(),
                format!("ERROR refused #{i}"),
                LogStream::Stdout,
            );
        }
        let t = Instant::now();
        ix.sync(&buf, &key(None, Some("refused")));
        let search_incr = t.elapsed();
        let t = Instant::now();
        ix.sync(&buf, &key(Some("slow query"), None));
        let filter = t.elapsed();
        let rows = ix.len();
        let t = Instant::now();
        let mut jumps = 0;
        let mut ix2 = ViewIndex::new();
        ix2.sync(&buf, &key(None, None));
        let build_all = t.elapsed();
        let mut at = None;
        let t = Instant::now();
        for _ in 0..10_000 {
            at = ix2.jump_level(at, true, true);
            jumps += 1;
        }
        let jump = t.elapsed();
        let t = Instant::now();
        for n in 0..10_000 {
            let _ = ix.nth_hit(n % ix.total_hits().max(1));
        }
        let nth = t.elapsed();
        let bytes = buf.bytes();
        eprintln!(
            "1M ingest {ingest:?} ({:.0}k lines/s) | held {} lines {:.1} MiB | find full {search_full:?} ({hits} hits) \
             incr(10k) {search_incr:?} | filter {filter:?} ({rows} rows) | all-rows index {build_all:?} | \
             {jumps} jumps {jump:?} | 10k nth_hit {nth:?}",
            N as f64 / ingest.as_secs_f64() / 1000.0,
            buf.len(),
            bytes as f64 / 1048576.0,
        );
        assert!(
            bytes <= 8 * (8 * 1024 * 1024 + 4096),
            "memory bound: {bytes}"
        );
        assert!(buf.len() <= 8 * 200_000);
        assert!(ingest.as_secs_f64() < 15.0, "ingest {ingest:?}");
        assert!(
            search_incr.as_millis() < 250,
            "incremental find {search_incr:?}"
        );
    }

    /// Rendered rows of the whole screen (non-blank content rows count).
    fn content_rows(app: &App, w: u16, h: u16) -> usize {
        let rows = screen(app, w, h);
        // Rows below the tab bar / above the footer that carry log text.
        rows.iter()
            .filter(|r| r.contains("req") || r.contains("burst") || r.contains("boom"))
            .count()
    }

    /// Regression: with a large merged All tab, typing a find query, jumping and
    /// clearing it left the log area blank forever (row cache kept the rows of
    /// the previous view definition while the index held a different subset).
    #[test]
    fn large_all_tab_survives_find_jump_clear_without_going_blank() {
        let mut app = make_app();
        app.splash_done = true;
        app.size = (140, 40);
        app.logs.log_viewport.set(LogViewport {
            text_width: 0,
            height: 0,
            max_scroll: 0,
            max_hscroll: 0,
        });
        // Live conditions: frames are drawn *while* the burst arrives, the burst
        // overflows the 50k ring (eviction), and a second service keeps ticking.
        for i in 0..200_000 {
            line(&mut app, "web", &format!("burst-line-{i}"));
            if i % 2500 == 0 {
                line(
                    &mut app,
                    "api",
                    &format!("req {i} ERROR boom at src/lib.rs:10"),
                );
            }
            if i % 20_000 == 0 {
                let _ = screen(&app, 140, 40);
            }
        }
        // First frame lays the pane out (as the live app does).
        assert!(
            content_rows(&app, 140, 40) > 5,
            "{:?}",
            screen(&app, 140, 40)
        );
        type_query(&mut app, "boom");
        assert!(
            content_rows(&app, 140, 40) > 0,
            "blank after typing the query"
        );
        app.update(Message::LogSearchCommit);
        app.update(Message::FindNext);
        app.update(Message::JumpProblem {
            forward: true,
            warn: false,
        });
        assert!(content_rows(&app, 140, 40) > 0, "blank after jump");
        app.update(Message::LogSearchClear);
        let after_clear = content_rows(&app, 140, 40);
        assert_eq!(
            app.logs.index.borrow().len(),
            app.logs.rows.borrow().len(),
            "row cache must mirror the index after the query is cleared"
        );
        assert!(
            after_clear > 5,
            "blank after Esc: {:?}",
            screen(&app, 140, 40)
        );
        app.update(Message::ScrollToBottom);
        line(&mut app, "api", "req tail ERROR boom");
        assert!(content_rows(&app, 140, 40) > 5, "blank after G + new line");
    }
}
