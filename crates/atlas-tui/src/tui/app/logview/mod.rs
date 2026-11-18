//! Log pane behavior on `App`: incremental find / filter with n/N, problem
//! jumps (`]e`/`[e`), link cursor + open (`o`), view toggles (wrap,
//! timestamps, JSON), horizontal pan, marks, restart separators, new-line
//! count while paused, and per-tab activity / error badges.
//!
//! Everything that looks at the whole buffer goes through `sync_logs`, which
//! advances an incremental `ViewIndex` + `RowCache`: a frame or a keypress
//! costs O(new lines + visible rows), never O(buffer).

mod find;
mod keys;
mod links;
mod pane;
#[cfg(test)]
mod tests;
mod time;
mod view;

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
    pub fn label(self) -> String {
        match self {
            Self::Find => atlas_i18n::t!("tui_runtime_widgets_search_find"),
            Self::Filter => atlas_i18n::t!("tui_runtime_widgets_search_filter"),
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
