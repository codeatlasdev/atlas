use crate::config::SessionPlatform;
use crate::runtime::logs::LogStream;
use crate::runtime::service::ServiceState;

// --- Service Menu Action ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceMenuAction {
    Restart,
    Stop,
    Start,
}

// --- Messages (TEA) ---
#[derive(Debug, Clone)]
pub enum Message {
    Quit,
    Tick,
    Resize(u16, u16),
    SwitchTab(usize),
    NextTab,
    PrevTab,
    ServiceStateChanged {
        name: String,
        state: ServiceState,
    },
    LogLineFrom {
        name: String,
        line: String,
        stream: LogStream,
    },
    /// `r` on All: opens the confirm modal.
    RestartAll,
    /// Confirmed: restart every non-completed, non-disabled service.
    RestartAllConfirmed,
    ShowHelp,
    HideHelp,
    ShowQuit,
    HideQuit,
    ToggleQuitSelection,
    /// Attached client: leave, the runner (and its services) keeps going.
    Detach,
    /// Attached client: stop everything on the runner, then leave.
    StopRunner,
    Toast(String, ToastVariant),
    DismissToast,
    ScrollDown,
    ScrollUp,
    ScrollPageDown,
    ScrollPageUp,
    ScrollToBottom,
    ScrollToTop,
    HealthCheckDone,
    // Mouse
    MouseClick {
        x: u16,
        y: u16,
    },
    /// Left-button drag; only meaningful while a scrollbar drag is active.
    MouseDrag {
        y: u16,
    },
    /// Left button released: ends a scrollbar drag.
    MouseRelease,
    MouseScroll {
        x: u16,
        y: u16,
        down: bool,
    },
    /// Scrollbar click/drag target: absolute `log_scroll` (rows above the bottom).
    ScrollLogsTo(usize),
    /// Sidebar row click: show that service's tab (no-op if it has none).
    SelectService(String),
    /// Palette row click: select the row and run it, like Enter.
    PaletteClick(usize),
    /// Device picker row click: select the row and confirm it, like Enter.
    DevicePickerPick(usize),
    ResetCursor,
    // Command Palette
    ShowPalette,
    HidePalette,
    PaletteInput(char),
    PaletteBackspace,
    /// Ctrl+U in the palette input.
    PaletteClear,
    /// Ctrl+W in the palette input.
    PaletteDeleteWord,
    PaletteSelect,
    PaletteUp,
    PaletteDown,
    // Log management
    CopyLogs,
    CopyLogsForAI,
    CopyErrors,
    ClearLogs,
    StartLogSearch,
    LogSearchInput(char),
    LogSearchBackspace,
    LogSearchCommit,
    LogSearchClear,
    /// `V`: cycle the time range all → 1m → 5m → 15m → 1h → since restart →
    /// since start → custom prompt → all.
    CycleTimeRange,
    /// Custom time-range prompt: typed char, backspace, Enter (apply), Esc.
    TimeInput(char),
    TimeBackspace,
    TimeCommit,
    TimeCancel,
    ClipboardResult {
        success: String,
        ok: bool,
    },
    // Service control
    StopService(String),
    RestartService(String),
    RunAction(char),
    /// Run an action, skipping its `confirm` gate (already confirmed by the user).
    RunActionConfirmed(char),
    // Open service URL in browser (key bound via open_key in atlas.yaml)
    OpenService(char),
    // Tunnel toggle
    ToggleTunnelOnExit,
    ToggleFollow,
    // Service context menu
    ShowServiceMenu(String),
    HideServiceMenu,
    ServiceMenuAction {
        name: String,
        action: ServiceMenuAction,
    },
    // Session tabs (session: true actions)
    SessionStarted {
        label: String,
    },
    SessionEnded {
        label: String,
    },
    SessionInput {
        label: String,
        bytes: Vec<u8>,
    },
    StopSession(String),
    // Attach/detach mode
    AttachSession,
    DetachSession,
    // Device picker
    ShowDevicePicker {
        action_key: char,
        platform: SessionPlatform,
    },
    DevicePickerUp,
    DevicePickerDown,
    DevicePickerConfirm,
    DevicePickerCancel,
    DevicesDetected {
        action_key: char,
        devices: Vec<(String, String)>,
    },
    RunSessionWithDevice {
        key: char,
        device_id: Option<String>,
    },
    /// Mobile build phase changed — updates sidebar badge.
    MobileSessionPhaseChanged {
        label: String,
        phase: crate::runtime::manager::MobileBuildPhase,
    },
    /// A mobile device disconnected mid-session.
    DeviceDisconnected {
        label: String,
        device: String,
    },
    /// Generic confirmation modal answered: run `on_confirm` or just close.
    ConfirmAccept,
    ConfirmCancel,
    /// Update modal accepted: install `version` after the TUI exits.
    UpdateOnExit(String),
    /// Bracketed paste routed to the active text input.
    Paste(String),
    /// Ctrl+Z: restore the terminal and stop the process.
    Suspend,
    /// `M`: toggle terminal mouse capture so the terminal can select text.
    ToggleMouseCapture,
    /// `e`: cycle the log level filter All → Warn+ → Error.
    CycleLevelFilter,
    /// `z`: hide the sidebar and give the logs the full width.
    ToggleZoom,
    /// Help overlay scroll (rows; MIN/MAX jump to the ends).
    HelpScroll(isize),
    /// Footer/strip click: jump to a tab by name.
    GotoTab(String),
    /// Search bar `Tab`: find ↔ filter.
    ToggleSearchMode,
    /// Search bar `Ctrl+R`: literal ↔ regex.
    ToggleSearchRegex,
    /// `n` / `N` while a find query is active.
    FindNext,
    FindPrev,
    /// `W`: wrap ↔ truncate.
    ToggleWrap,
    /// `T`: timestamps on/off.
    ToggleTimestamps,
    /// `J`: raw ↔ compact JSON lines.
    ToggleJson,
    /// Wrap off: pan the log columns (←/→).
    PanLogs {
        right: bool,
    },
    /// `-`: insert a mark line into the current tab's stream.
    InsertMark,
    /// Click on a tab's `✗N` badge: open it filtered to errors.
    OpenTabErrors(usize),
    /// `]e` / `[e` (`]w` / `[w` with `warn`): jump to the next / previous
    /// error (or warning-or-error) line.
    JumpProblem {
        forward: bool,
        warn: bool,
    },
    /// `Tab`/`Shift+Tab` on a link-bearing view: move the link cursor.
    StepLink {
        forward: bool,
    },
    /// `o`: open the link under the cursor (URL → browser, path → editor).
    OpenLink,
    /// `y`: copy the link under the cursor.
    CopyLink,
    /// First half of a two-key chord (`]` / `[`) was pressed.
    ChordStart(char),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToastVariant {
    /// Static fact (static icon).
    Info,
    /// Operation in flight (spinner).
    Progress,
    Success,
    Error,
    Warning,
}

// --- Command Palette ---
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    /// Stable id for the recent list (`restart:api`, `action:c`, `builtin:help`).
    pub id: String,
    pub name: String,
    pub description: String,
    pub shortcut: Option<String>,
    pub action: Message,
    /// Action `group:` label; rendered as a dim prefix.
    pub group: Option<String>,
    /// Char indices of `name` matched by the query (highlighted).
    pub matched: Vec<usize>,
}

// --- Running operation (sidebar OPERATIONS block) ---
/// How long a failed operation stays visible if no new action runs.
const FAILED_OPERATION_TTL: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Operation {
    pub label: String,
    pub started: std::time::Instant,
    /// Last non-empty output line of the action.
    pub detail: String,
    /// Set when the action failed: (when it failed, last line at failure).
    pub failed: Option<(std::time::Instant, String)>,
}

impl Operation {
    pub fn start(label: String) -> Self {
        Self {
            label,
            started: std::time::Instant::now(),
            detail: String::new(),
            failed: None,
        }
    }

    /// Failed operations expire after 30s; running ones never do.
    pub fn expired(&self) -> bool {
        self.failed
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() > FAILED_OPERATION_TTL)
    }

    /// `mm:ss` since start (frozen at failure time once failed).
    pub fn elapsed_label(&self) -> String {
        let secs = match &self.failed {
            Some((at, _)) => at.duration_since(self.started),
            None => self.started.elapsed(),
        }
        .as_secs();
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

// --- Layers (focus management) ---
// Hand-written PartialEq below: `Confirm::on_confirm` holds a `Message`, which is not `Eq`.
#[derive(Debug, Clone)]
pub enum Layer {
    Dashboard,
    Help,
    Quit,
    CommandPalette,
    ServiceMenu(String),
    /// Attached to a session — all keys forwarded to process stdin.
    /// Ctrl+B detaches back to Dashboard.
    SessionAttached(String),
    /// Device picker modal before spawning a session.
    DevicePicker {
        action_key: char,
        platform: SessionPlatform,
        devices: Vec<(String, String)>, // (id, display_name)
        selected: usize,
        /// True while adb/xcrun detection is still running.
        detecting: bool,
    },
    /// Generic yes/no modal. Enter/`y` dispatches `on_confirm`; Esc/`n` closes.
    Confirm {
        title: String,
        body: String,
        on_confirm: Box<Message>,
    },
}

impl PartialEq for Layer {
    fn eq(&self, other: &Self) -> bool {
        use Layer::*;
        match (self, other) {
            (Dashboard, Dashboard)
            | (Help, Help)
            | (Quit, Quit)
            | (CommandPalette, CommandPalette) => true,
            (ServiceMenu(a), ServiceMenu(b)) | (SessionAttached(a), SessionAttached(b)) => a == b,
            (
                DevicePicker {
                    action_key: k1,
                    platform: p1,
                    devices: d1,
                    selected: s1,
                    detecting: t1,
                },
                DevicePicker {
                    action_key: k2,
                    platform: p2,
                    devices: d2,
                    selected: s2,
                    detecting: t2,
                },
            ) => k1 == k2 && p1 == p2 && d1 == d2 && s1 == s2 && t1 == t2,
            (
                Confirm {
                    title: t1,
                    body: b1,
                    on_confirm: m1,
                },
                Confirm {
                    title: t2,
                    body: b2,
                    on_confirm: m2,
                },
            ) => t1 == t2 && b1 == b2 && format!("{m1:?}") == format!("{m2:?}"),
            _ => false,
        }
    }
}

#[derive(Debug)]
pub(super) enum Effect {
    /// Restart exactly these services (completed one-shots excluded by the caller).
    RestartServices(Vec<String>),
    CheckHealth,
    CopyToClipboard {
        text: String,
        success: String,
    },
    /// Open a URL from a log line in the system browser.
    OpenUrl(String),
    /// Open `path:line[:col]` from a log line in an editor (copies when none applies).
    OpenPath {
        path: String,
        line: u32,
        col: Option<u32>,
    },
    StopService(String),
    RestartService(String),
    RunAction(char),
    OpenService(char),
    StartAppServices(Vec<String>),
    StopSession(String),
    SendSessionInput {
        label: String,
        bytes: Vec<u8>,
    },
    DetectDevices {
        action_key: char,
        platform: SessionPlatform,
    },
    RunSessionWithDevice {
        key: char,
        device_id: Option<String>,
    },
    Suspend,
    SetMouseCapture(bool),
}
