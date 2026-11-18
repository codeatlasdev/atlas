//! Per-pane behavior: view toggles, pan, marks, restart separators, the
//! new-lines-while-paused count and per-tab activity / error badges.
use crate::runtime::logs::LogLevel;
use crate::tui::widgets::log_viewer;
use atlas_i18n::t;

use super::super::{App, LevelFilter, ToastVariant};
use super::TabBadge;

impl App {
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
    pub(in crate::tui::app) fn toggle_wrap(&mut self) {
        self.logs.log_wrap = !self.logs.log_wrap;
        self.logs.log_hscroll = 0;
        self.save_ui_state();
        self.set_toast(
            if self.logs.log_wrap {
                t!("tui_runtime_app_toast_wrap_on")
            } else {
                t!("tui_runtime_app_toast_wrap_off")
            },
            ToastVariant::Info,
        );
    }

    /// `T`
    pub(in crate::tui::app) fn toggle_timestamps(&mut self) {
        self.logs.log_timestamps = !self.logs.log_timestamps;
        self.save_ui_state();
    }

    /// `J`
    pub(in crate::tui::app) fn toggle_json(&mut self) {
        self.logs.log_json_pretty = !self.logs.log_json_pretty;
        self.save_ui_state();
    }

    /// ←/→ with wrap off: pan by a third of the view.
    pub(in crate::tui::app) fn pan_logs(&mut self, right: bool) {
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
    pub(in crate::tui::app) fn insert_mark(&mut self) {
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
    pub(in crate::tui::app) fn log_service_restarted(&mut self, name: &str) {
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
    pub(in crate::tui::app) fn bump_paused_scroll_for_last(&mut self) {
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
    pub(in crate::tui::app) fn mark_tab_seen(&mut self) {
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
    pub(in crate::tui::app) fn open_tab_errors(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active_tab = idx;
            self.logs.level_filter = LevelFilter::Error;
            self.reset_log_view();
            self.save_ui_state();
        }
    }
}
