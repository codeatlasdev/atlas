//! Sub-states of `App`: the log pane and the interactive-session bookkeeping.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use super::logview::SearchMode;
use crate::config::SessionPlatform;
use crate::runtime::logindex::ViewIndex;
use crate::runtime::logs::{LogBuffer, LogLevel};
use crate::runtime::manager::MobileBuildPhase;
use crate::runtime::timerange::TimeRange;
use crate::tui::widgets::log_viewer::{Cursor, LogViewport};
use crate::tui::widgets::rowcache::RowCache;

/// Lines kept per service in the in-memory log buffer (the 8 MiB per-service
/// byte budget in `LogBuffer` bounds memory first for long lines).
const LOG_BUFFER_CAPACITY: usize = 50_000;

/// Minimum severity shown in the log pane (`v` cycles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LevelFilter {
    #[default]
    All,
    /// Warn and Error.
    Warn,
    Error,
}

impl LevelFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Warn,
            Self::Warn => Self::Error,
            Self::Error => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Warn => "warn+",
            Self::Error => "error",
        }
    }

    pub fn allows(self, level: LogLevel) -> bool {
        match self {
            Self::All => true,
            Self::Warn => matches!(level, LogLevel::Warn | LogLevel::Error),
            Self::Error => level == LogLevel::Error,
        }
    }
}

/// Everything the log pane remembers: buffer, scroll/follow, view toggles, search.
pub struct LogPane {
    pub log_buffer: LogBuffer,
    /// Wrapped rows scrolled up from the bottom (0 = newest visible).
    pub log_scroll: usize,
    /// Stick to the bottom as new lines arrive. Off = view is frozen in place.
    pub follow_logs: bool,
    /// Geometry of the last log render; scroll handlers clamp against it.
    pub log_viewport: Cell<LogViewport>,
    /// Sequence number at which follow paused (counts "N new").
    pub paused_since_seq: Option<u64>,
    /// Per-tab sequence number at the last time the tab was viewed (badges).
    pub tab_seen: HashMap<String, u64>,
    /// `W`: hard-wrap long lines (off = truncate, ←/→ pan).
    pub log_wrap: bool,
    /// `T`: show `HH:MM:SS` on each line.
    pub log_timestamps: bool,
    /// `J`: compact JSON-object lines into `level msg key=val …`.
    pub log_json_pretty: bool,
    /// Columns panned while wrap is off.
    pub log_hscroll: usize,
    /// Minimum severity shown in the log pane.
    pub level_filter: LevelFilter,
    /// Time window of the view (`V` cycles; relative ranges slide).
    pub time_range: TimeRange,
    /// Custom-range prompt: `Some(text)` while it is open.
    pub time_input: Option<String>,
    /// Why the last custom input was rejected (shown in the prompt).
    pub time_error: Option<String>,
    pub log_search_input: String,
    pub log_searching: bool,
    /// Search bar behavior: highlight + n/N (find) or only matching lines (filter).
    pub search_mode: SearchMode,
    /// Query is a regex (`Ctrl+R`); smartcase either way.
    pub search_regex: bool,
    /// Index of the current find match (n/N moves it).
    pub find_current: usize,
    /// Incremental projection of the buffer for the active view (tab + level +
    /// query). Interior mutability: `view()` takes `&App` and syncs it.
    pub index: RefCell<ViewIndex>,
    /// Wrapped heights of `index`'s rows for the last drawn layout.
    pub rows: RefCell<RowCache>,
    /// Link under the cursor (`o` opens, `y` copies).
    pub link_cursor: Option<Cursor>,
    /// Seq of the last problem jump (`]e`/`[e` continue from here).
    pub problem_cursor: Option<u64>,
    /// First half of a `]`/`[` chord is pending.
    pub chord: Option<char>,
}

impl Default for LogPane {
    fn default() -> Self {
        Self {
            log_buffer: LogBuffer::new(LOG_BUFFER_CAPACITY),
            log_scroll: 0,
            follow_logs: true,
            log_viewport: Cell::default(),
            paused_since_seq: None,
            tab_seen: HashMap::new(),
            log_wrap: true,
            log_timestamps: true,
            log_json_pretty: false,
            log_hscroll: 0,
            level_filter: LevelFilter::All,
            time_range: TimeRange::All,
            time_input: None,
            time_error: None,
            log_search_input: String::new(),
            log_searching: false,
            search_mode: SearchMode::Find,
            search_regex: false,
            find_current: 0,
            index: RefCell::new(ViewIndex::new()),
            rows: RefCell::new(RowCache::new()),
            link_cursor: None,
            problem_cursor: None,
            chord: None,
        }
    }
}

/// Active `session: true` actions and their mobile-build details.
#[derive(Default)]
pub struct Sessions {
    /// Tab names that are active sessions: the input handler forwards keys to
    /// the session's stdin instead of the TUI.
    pub tabs: HashSet<String>,
    /// Current mobile build phase per session label — drives the sidebar badge.
    pub phases: HashMap<String, MobileBuildPhase>,
    /// Platform per active mobile session label — drives footer hints and picker header.
    pub platforms: HashMap<String, SessionPlatform>,
}
