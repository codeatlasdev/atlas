//! The agent dock: a small always-on-top pill (route `/dock`) that sends text to the agents Atlas runs, from any app
//! and any Space, with the main window closed. See `docs/desktop.md`, "Agent dock".
//!
//! Like the tray popover the dock's webview owns no stores. The main window publishes the agents it knows
//! ([`dock_publish`], shape `apps/desktop/src/lib/dock.ts` `DockSnapshot`); Rust caches them, forwards them as
//! `dock://snapshot`, and delivers text itself ([`dock_send`] → `agent::send`), so a send never depends on a
//! webview that is hidden, throttled or unfocused.
//!
//! Window: on macOS the Tauri window becomes a non-activating `NSPanel` (`dock_mac.rs`): showing it, clicking it
//! and typing in it never activates Atlas or takes the focus of the app you are in. Elsewhere it is a plain
//! always-on-top window without a taskbar entry.
//!
//! Agent instances (`agent_mode`): the dock window is built but never shown by itself. `dock_show` is the only way,
//! and it goes through `agent_display::park_and_show` (the virtual display and the watchdog) there, never
//! `Window::show`, so the dock cannot reach the user's displays (guard test in `agent_display.rs`).
//!
//! Hotkeys are `tauri-plugin-global-shortcut` chords (Carbon `RegisterEventHotKey` on macOS: no Accessibility
//! permission). The summon key is a setting; the action table ([`Action`]) is where later hotkeys (push-to-talk,
//! capture) join it.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "dock";
/// Key of the prefs in `settings.json`.
const SETTINGS_KEY: &str = "dock";
/// Gap kept between the pill and the bottom of the display, logical px.
const BOTTOM_GAP: f64 = 28.0;
/// Margin kept to the display edges, logical px.
const MARGIN: f64 = 8.0;
const PILL: (f64, f64) = (320.0, 40.0);

/// The dock settings (`settings.json` → `dock`, camelCase; read by the page and Settings → Agents).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DockPrefs {
    /// The pill is shown while Atlas runs. Off: no window at all. This is the persisted "hidden" choice: it
    /// survives restarts, and [`Action::Toggle`], the pill's hide button and the tray item all flip it.
    pub enabled: bool,
    /// Summon key in the keymap's stroke grammar (`ctrl-alt-space`); empty = no hotkey.
    pub summon: String,
    /// Hide / show key. Registered even while the pill is hidden: it is the way back.
    pub toggle: String,
}

impl Default for DockPrefs {
    fn default() -> Self {
        Self {
            enabled: true,
            summon: "ctrl-alt-space".into(),
            toggle: "ctrl-alt-d".into(),
        }
    }
}

impl DockPrefs {
    /// The prefs in `settings.json`; anything unreadable is the default (a bad file never hides the dock's way back).
    pub fn load() -> Self {
        let s = crate::shell_env::load_settings();
        s.extra
            .get(SETTINGS_KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Refuse what could not work, with a sentence for the person.
    pub fn validate(&self) -> Result<(), String> {
        crate::hotkeys::accelerator(&self.summon)?;
        crate::hotkeys::accelerator(&self.toggle)?;
        if !self.summon.trim().is_empty() && self.summon.trim() == self.toggle.trim() {
            return Err("The summon and hide keys are the same; pick two different keys".into());
        }
        Ok(())
    }

    /// The same prefs with the pill shown if it was hidden and hidden if it was shown.
    pub fn toggled(&self) -> Self {
        Self {
            enabled: !self.enabled,
            ..self.clone()
        }
    }
}

/// A hotkey the dock owns. Capture hotkeys join it in `capture_hotkeys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Open (or close) the composer.
    Summon,
    /// Hide the pill, or bring it back.
    Toggle,
}

/// Top-left (physical px) of a `size` (w, h, logical) window on `screen` (x, y, w, h, physical): horizontally
/// centred, [`BOTTOM_GAP`] above the bottom edge, and never outside the screen. `anchor` is a position the user
/// dragged it to (physical top-left); it wins when it still lies on this screen.
pub fn place(
    anchor: Option<(f64, f64)>,
    size: (f64, f64),
    screen: (f64, f64, f64, f64),
    scale: f64,
) -> (f64, f64) {
    let (w, h) = (size.0 * scale, size.1 * scale);
    let (sx, sy, sw, sh) = screen;
    let (margin, gap) = (MARGIN * scale, BOTTOM_GAP * scale);
    let clamp = |x: f64, y: f64| {
        (
            x.clamp(sx + margin, (sx + sw - w - margin).max(sx + margin)),
            y.clamp(sy + margin, (sy + sh - h - margin).max(sy + margin)),
        )
    };
    match anchor {
        Some((x, y)) if x >= sx && x < sx + sw && y >= sy && y < sy + sh => clamp(x, y),
        _ => clamp(sx + (sw - w) / 2.0, sy + sh - h - gap),
    }
}

/// Top-left (physical px) of a window resized from `old` (x, y, w, h physical) to `new_size` (w, h physical): the
/// bottom-centre stays where it was, so the pill grows upward into a composer instead of off the bottom of the
/// screen, then the result is kept inside `screen` (x, y, w, h physical).
pub fn regrow(
    old: (f64, f64, f64, f64),
    new_size: (f64, f64),
    screen: (f64, f64, f64, f64),
    scale: f64,
) -> (f64, f64) {
    let centre = old.0 + old.2 / 2.0;
    let bottom = old.1 + old.3;
    let (w, h) = new_size;
    place(
        Some((centre - w / 2.0, bottom - h)),
        (w / scale, h / scale),
        screen,
        scale,
    )
}

impl DockState {
    /// A copy of the current prefs (the capture module checks its keys against the dock's).
    pub fn prefs(&self) -> DockPrefs {
        self.prefs.lock().clone()
    }

    /// The pill is shown (the persisted choice, not whether the window is on screen this instant).
    pub fn shown(&self) -> bool {
        self.prefs.lock().enabled
    }
}

/// What the dock keeps between calls.
#[derive(Default)]
pub struct DockState {
    prefs: Mutex<DockPrefs>,
    snapshot: Mutex<Value>,
    /// The accelerators registered now, released before the next set.
    registered: crate::hotkeys::Registered,
    /// Where the user dragged the pill (physical top-left).
    anchor: Mutex<Option<(f64, f64)>>,
    /// The window has been shown by this process (the first time it is placed).
    placed: Mutex<bool>,
    /// The page has mounted its listeners (`dock_ready`): a hotkey before that would show an unpainted window and
    /// its `dock://summon` would reach nobody.
    ready: std::sync::atomic::AtomicBool,
    /// Captures sent to the composer (`attach`) that the page has not taken yet.
    attachments: Mutex<Attachments>,
}

/// Captures waiting for the dock page: a queue the page drains (`dock_take_attachments`), because the dock page may
/// not have mounted its listeners when a capture is sent (it was never shown, or is still booting), and an event
/// to nobody is lost. Order is kept; one path appears once.
#[derive(Debug, Default)]
pub struct Attachments(Vec<Attachment>);

/// A capture waiting for the composer (`dock://attach`, `dock_take_attachments`), camelCase for the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Attachment {
    pub path: String,
    pub kind: String,
}

impl Attachments {
    /// Queue `a` unless the same path is already waiting.
    pub fn push(&mut self, a: Attachment) {
        if !self.0.iter().any(|x| x.path == a.path) {
            self.0.push(a);
        }
    }

    /// Everything waiting, oldest first; the queue is empty afterwards.
    pub fn take(&mut self) -> Vec<Attachment> {
        std::mem::take(&mut self.0)
    }
}

/// The agents of the last snapshot, as Rust reads them: only what delivery needs.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotAgent {
    tab_id: String,
    pty: String,
}

#[derive(Debug, Deserialize)]
struct SnapshotAgents {
    #[serde(default)]
    agents: Vec<SnapshotAgent>,
}

/// The pty of agent tab `tab` in `snapshot`.
fn pty_of(snapshot: &Value, tab: &str) -> Option<String> {
    serde_json::from_value::<SnapshotAgents>(snapshot.clone())
        .ok()?
        .agents
        .into_iter()
        .find(|a| a.tab_id == tab)
        .map(|a| a.pty)
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let (theme, script) = crate::boot::window_boot(crate::boot::load_cache().as_ref());
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("dock".into()));
    // Windows: every webview of the app passes the same browser args (webview_win::ARGS).
    #[cfg(windows)]
    let builder = builder.additional_browser_args(crate::webview_win::ARGS);
    let win = builder
        .theme(theme)
        .initialization_script(script)
        .title("Atlas Dock")
        .inner_size(PILL.0, PILL.1)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .visible(false)
        .focused(false)
        .accept_first_mouse(true)
        .build()?;
    #[cfg(target_os = "macos")]
    if let Ok(ns) = win.ns_window() {
        crate::dock_mac::make_panel(ns);
    }
    Ok(win)
}

/// The monitor under the cursor, else the primary one: (x, y, w, h physical) and its scale.
fn screen_under_cursor(app: &AppHandle) -> Option<((f64, f64, f64, f64), f64)> {
    let m = app
        .cursor_position()
        .ok()
        .and_then(|c| app.monitor_from_point(c.x, c.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let (p, s) = (m.position(), m.size());
    Some((
        (
            f64::from(p.x),
            f64::from(p.y),
            f64::from(s.width),
            f64::from(s.height),
        ),
        m.scale_factor(),
    ))
}

/// Put the window where it belongs for its current size: where the user left it, else bottom-centre of the display
/// under the cursor.
fn reposition(app: &AppHandle, win: &WebviewWindow, to_cursor: bool) {
    let state = app.state::<DockState>();
    let Some((screen, scale)) = screen_under_cursor(app) else {
        return;
    };
    let size = win
        .outer_size()
        .map(|s| (f64::from(s.width) / scale, f64::from(s.height) / scale))
        .unwrap_or(PILL);
    let anchor = if to_cursor {
        None
    } else {
        *state.anchor.lock()
    };
    let (x, y) = place(anchor, size, screen, scale);
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

/// [`show`] on the main thread, from whichever thread asks (Tauri runs commands on workers; AppKit window calls trap
/// off the main thread).
fn show_on_main(app: &AppHandle) {
    let a = app.clone();
    let _ = app.run_on_main_thread(move || show(&a));
}

/// Order the dock in. The user's app shows it (non-activating, see `dock_mac.rs`); an agent instance only through
/// the guarded virtual display path.
fn show(app: &AppHandle) {
    let Some(win) = app.get_webview_window(LABEL) else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        return;
    }
    if crate::agent_mode::Mode::current().focus_allowed() {
        reposition(app, &win, !*app.state::<DockState>().placed.lock());
        *app.state::<DockState>().placed.lock() = true;
        let _ = win.show();
    } else {
        #[cfg(target_os = "macos")]
        if let Some(w) = app.get_window(LABEL) {
            crate::agent_display::guard_user_displays(app);
            crate::agent_display::park_and_show(app, &w);
        }
    }
}

fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// The hotkeys the dock wants registered for `prefs`: the summon key only while the pill is shown, the hide key
/// always (it is the way back from hidden). Empty strokes are not hotkeys.
fn wanted_keys(prefs: &DockPrefs) -> Vec<(Action, String)> {
    let mut out = Vec::new();
    if prefs.enabled && !prefs.summon.trim().is_empty() {
        out.push((Action::Summon, prefs.summon.clone()));
    }
    if !prefs.toggle.trim().is_empty() {
        out.push((Action::Toggle, prefs.toggle.clone()));
    }
    out
}

/// Make the registered hotkeys the ones `prefs` asks for. A failure leaves nothing registered and says why.
fn apply_hotkeys(app: &AppHandle, prefs: &DockPrefs) -> Result<(), String> {
    let state = app.state::<DockState>();
    // An agent instance never owns a global hotkey: it would take the key from the user's own app.
    let keys = if !crate::agent_mode::Mode::current().focus_allowed() {
        Vec::new()
    } else {
        wanted_keys(prefs)
            .into_iter()
            .map(|(action, stroke)| {
                let f: Box<dyn Fn(&AppHandle) + Send + Sync> =
                    Box::new(move |app| run(app, action));
                (stroke, f)
            })
            .collect()
    };
    state.registered.apply(app, keys)
}

/// Show the dock and open its composer (the summon hotkey, a capture sent to an agent). Nothing happens before the
/// page has painted (`dock_ready`): its `dock://summon` would reach nobody.
///
/// AppKit window calls (`show` places the panel, or parks it on the agent display) trap off the main thread, and
/// this is called from async command threads (`attach`, from the card's "Send to agent"): the work runs on the main
/// thread (agent run, 2026-10-26: SIGTRAP in `setFrame:display:` under `dock::show`).
pub fn summon(app: &AppHandle) {
    if !app
        .state::<DockState>()
        .ready
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return;
    }
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        show(&a);
        let Some(win) = a.get_webview_window(LABEL) else {
            return;
        };
        #[cfg(target_os = "macos")]
        crate::dock_mac::set_key_allowed(true);
        let _ = win.emit("dock://summon", ());
    });
}

/// A capture for the composer: queued (the page may not be listening yet), announced with `dock://attach`, and the
/// composer opens. A hidden dock is shown for it: attaching silently to a pill nobody can see would lose the capture.
pub fn attach(app: &AppHandle, a: Attachment) {
    app.state::<DockState>().attachments.lock().push(a);
    let _ = app.emit_to(LABEL, "dock://attach", ());
    summon(app);
}

/// A hotkey fired.
fn run(app: &AppHandle, action: Action) {
    match action {
        Action::Summon => summon(app),
        Action::Toggle => {
            let _ = toggle(app);
        }
    }
}

/// Hide the pill when it is shown, show it when it is hidden, and remember the choice (`settings.json`). Shared by
/// the hide key, the pill's hide button, its context menu, the tray item and the tray popover.
pub fn toggle(app: &AppHandle) -> Result<DockPrefs, String> {
    let next = app.state::<DockState>().prefs.lock().toggled();
    set_prefs(app, next)
}

const MENU_OPEN: &str = "dock:open";
const MENU_HIDE: &str = "dock:hide";

/// One entry of the pill's right-click menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub id: &'static str,
    pub label: String,
}

/// A stroke of the keymap grammar as the glyphs a Mac menu shows (`ctrl-alt-d` → `⌃⌥D`). Empty stays empty.
pub fn glyphs(stroke: &str) -> String {
    let stroke = stroke.trim().to_lowercase();
    if stroke.is_empty() {
        return String::new();
    }
    let mut parts: Vec<&str> = stroke.split('-').collect();
    let key = if stroke.ends_with("--") {
        parts.truncate(parts.len() - 2);
        "-".to_string()
    } else {
        parts.pop().unwrap_or_default().to_string()
    };
    let mut out = String::new();
    for m in ["ctrl", "alt", "shift", "cmd"] {
        if parts.contains(&m) {
            out.push(match m {
                "ctrl" => '⌃',
                "alt" => '⌥',
                "shift" => '⇧',
                _ => '⌘',
            });
        }
    }
    let mut chars = key.chars();
    if let Some(c) = chars.next() {
        out.extend(c.to_uppercase().chain(chars))
    }
    out
}

/// The pill's right-click menu.
pub fn pill_menu(prefs: &DockPrefs) -> Vec<MenuEntry> {
    let hint = glyphs(&prefs.toggle);
    vec![
        MenuEntry {
            id: MENU_OPEN,
            label: "Open Atlas".into(),
        },
        MenuEntry {
            id: MENU_HIDE,
            label: if hint.is_empty() {
                "Hide Agent Dock".into()
            } else {
                format!("Hide Agent Dock  {hint}")
            },
        },
    ]
}

/// Dock: pop the native right-click menu at the cursor. It is a native menu on purpose: a webview menu would need
/// the panel to become key, which the idle pill must never be.
#[tauri::command]
pub fn dock_menu(app: AppHandle, state: State<'_, DockState>) -> Result<(), String> {
    use tauri::menu::{ContextMenu, Menu, MenuItem, PredefinedMenuItem};
    let win = app.get_window(LABEL).ok_or("the dock is not built")?;
    let items = pill_menu(&state.prefs.lock());
    let menu = Menu::new(&app).map_err(|e| e.to_string())?;
    for (i, e) in items.iter().enumerate() {
        if i > 0 {
            let sep = PredefinedMenuItem::separator(&app).map_err(|e| e.to_string())?;
            menu.append(&sep).map_err(|e| e.to_string())?;
        }
        let item = MenuItem::with_id(&app, e.id, &e.label, true, None::<&str>)
            .map_err(|e| e.to_string())?;
        menu.append(&item).map_err(|e| e.to_string())?;
    }
    menu.popup(win).map_err(|e| e.to_string())
}

/// A pill menu item was chosen (wired once in `main`: `on_menu_event`).
pub fn on_menu(app: &AppHandle, id: &str) {
    match id {
        MENU_OPEN => crate::show_main(app),
        MENU_HIDE => {
            let _ = toggle(app);
        }
        _ => {}
    }
}

/// Validate, register, persist and apply `prefs` (pill shown or hidden), then tell the dock page. Nothing changes on
/// error: the previous hotkeys are put back.
fn set_prefs(app: &AppHandle, prefs: DockPrefs) -> Result<DockPrefs, String> {
    let state = app.state::<DockState>();
    prefs.validate()?;
    let before = state.prefs.lock().clone();
    apply_hotkeys(app, &prefs).inspect_err(|_| {
        // keep what worked before
        let _ = apply_hotkeys(app, &before);
    })?;
    let mut patch = serde_json::Map::new();
    patch.insert(
        SETTINGS_KEY.to_string(),
        serde_json::to_value(&prefs).map_err(|e| e.to_string())?,
    );
    crate::shell_env::settings_patch(patch).inspect_err(|_| {
        let _ = apply_hotkeys(app, &before);
    })?;
    *state.prefs.lock() = prefs.clone();
    if prefs.enabled {
        show_on_main(app);
    } else {
        hide(app);
    }
    let _ = app.emit_to(LABEL, "dock://prefs", &prefs);
    let _ = app.emit("dock://visible", prefs.enabled);
    crate::tray::set_dock_label(app, prefs.enabled);
    Ok(prefs)
}

/// Build the window, register the hotkey and show the pill (the user's app, when enabled).
pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let prefs = DockPrefs::load();
    app.manage(DockState {
        prefs: Mutex::new(prefs.clone()),
        snapshot: Mutex::new(Value::Null),
        ..DockState::default()
    });
    build(app)?;
    if let Err(e) = apply_hotkeys(app, &prefs) {
        eprintln!("dock: hotkey: {e}");
    }
    Ok(())
}

/// The page is painted: show it (the pill is always there while Atlas runs; Settings can turn it off).
#[tauri::command]
pub fn dock_ready(app: AppHandle) {
    app.state::<DockState>()
        .ready
        .store(true, std::sync::atomic::Ordering::Release);
    if app.state::<DockState>().prefs.lock().enabled
        && crate::agent_mode::Mode::current().focus_allowed()
    {
        show_on_main(&app);
    }
}

/// Show the dock (Settings, the tray menu, an agent instance's debug eval). Agent instances: the guarded path.
#[tauri::command]
pub fn dock_show(app: AppHandle) {
    show_on_main(&app);
}

/// Main window → dock: the agents it knows (`DockSnapshot`), cached for the next time the dock paints.
#[tauri::command]
pub fn dock_publish(app: AppHandle, state: State<'_, DockState>, snapshot: Value) {
    *state.snapshot.lock() = snapshot.clone();
    let _ = app.emit_to(LABEL, "dock://snapshot", snapshot);
}

/// Dock: the last published snapshot (`null` before the main window sent one).
#[tauri::command]
pub fn dock_snapshot(state: State<'_, DockState>) -> Value {
    state.snapshot.lock().clone()
}

/// Dock: the captures sent to the composer since it last asked, oldest first (`dock://attach` says there are some).
#[tauri::command]
pub fn dock_take_attachments(state: State<'_, DockState>) -> Vec<Attachment> {
    state.attachments.lock().take()
}

/// Dock: its settings.
#[tauri::command]
pub fn dock_prefs(state: State<'_, DockState>) -> DockPrefs {
    state.prefs.lock().clone()
}

/// Settings → dock: validate, save, apply (hotkeys, pill) and tell the dock page. Nothing changes on error.
#[tauri::command]
pub fn dock_prefs_set(app: AppHandle, prefs: DockPrefs) -> Result<DockPrefs, String> {
    set_prefs(&app, prefs)
}

/// The pill's hide button and context menu, the tray popover: flip hidden / shown and remember it.
#[tauri::command]
pub fn dock_toggle(app: AppHandle) -> Result<DockPrefs, String> {
    toggle(&app)
}

/// Dock: fit the window to its content (logical px) and keep it on the screen; the pill stays where it is.
#[tauri::command]
pub fn dock_resize(app: AppHandle, state: State<'_, DockState>, width: f64, height: f64) {
    let Some(win) = app.get_webview_window(LABEL) else {
        return;
    };
    let (width, height) = (width.clamp(120.0, 900.0), height.clamp(32.0, 700.0));
    let old = win.outer_position().ok().zip(win.outer_size().ok());
    let monitor = win.current_monitor().ok().flatten();
    let _ = win.set_size(LogicalSize::new(width, height));
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let screen = (
        f64::from(m.position().x),
        f64::from(m.position().y),
        f64::from(m.size().width),
        f64::from(m.size().height),
    );
    let (x, y) = match old {
        Some((p, s)) => regrow(
            (
                f64::from(p.x),
                f64::from(p.y),
                f64::from(s.width),
                f64::from(s.height),
            ),
            (width * scale, height * scale),
            screen,
            scale,
        ),
        None => place(*state.anchor.lock(), (width, height), screen, scale),
    };
    let _ = win.set_position(PhysicalPosition::new(x, y));
}

/// Dock: the user let go of the drag; remember where it is.
#[tauri::command]
pub fn dock_dragged(app: AppHandle, state: State<'_, DockState>) {
    if let Some(p) = app
        .get_webview_window(LABEL)
        .and_then(|w| w.outer_position().ok())
    {
        *state.anchor.lock() = Some((f64::from(p.x), f64::from(p.y)));
    }
}

/// Dock: the composer is open (it may take keys) or closed (the pill never takes focus from the app you are in).
/// Agent instances never take key status (guard test in `agent_display.rs`).
#[tauri::command]
pub fn dock_keys(app: AppHandle, on: bool) {
    if !crate::agent_mode::Mode::current().focus_allowed() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        crate::dock_mac::set_key_allowed(on);
        if on {
            let a = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(w) = a.get_window(LABEL) {
                    crate::dock_mac::make_key(&w);
                }
            });
        }
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_focusable(on);
    }
}

/// Dock: deliver `text` to agent tab `tab` through hostd, whatever window is open or focused.
#[tauri::command]
pub async fn dock_send(
    app: AppHandle,
    state: State<'_, DockState>,
    tab: String,
    text: String,
) -> Result<(), String> {
    let pty = pty_of(&state.snapshot.lock(), &tab)
        .ok_or("That agent is not running any more. Pick another.")?;
    crate::agent::send::send_text(&app, &pty, &text).await
}

/// Dock: bring Atlas forward on agent tab `tab` (or just show it), like a click in the tray popover.
#[tauri::command]
pub fn dock_open(app: AppHandle, tab: Option<String>) {
    crate::show_main(&app);
    let _ = app.emit_to(
        "main",
        "tray://action",
        serde_json::json!({ "kind": "agents", "tab": tab }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/lib/__fixtures__/dock-snapshot.json");
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    /// Contract with the frontend: `__fixtures__/dock-snapshot.json` is the REAL output of the producer
    /// (`buildSnapshot`, pinned by `dock.test.ts`). Delivery reads the pty of a tab from exactly this shape.
    #[test]
    fn the_published_snapshot_resolves_each_tab_to_its_pty() {
        let s = fixture();
        assert_eq!(pty_of(&s, "tab-api").as_deref(), Some("pty-1"));
        assert_eq!(pty_of(&s, "tab-web").as_deref(), Some("pty-2"));
        assert_eq!(pty_of(&s, "tab-plan").as_deref(), Some("pty-3"));
    }

    #[test]
    fn a_closed_tab_or_no_snapshot_yet_has_no_pty() {
        assert_eq!(pty_of(&fixture(), "closed"), None);
        assert_eq!(pty_of(&Value::Null, "tab-api"), None);
        assert_eq!(pty_of(&json!({"agents": "nope"}), "tab-api"), None);
        assert_eq!(pty_of(&json!({"at": 1}), "tab-api"), None);
    }

    fn att(path: &str) -> Attachment {
        Attachment {
            path: path.into(),
            kind: "screenshot".into(),
        }
    }

    /// A capture sent while the dock page is not listening (never shown, still booting) must wait for it, in order,
    /// and be delivered exactly once: the page drains the queue on mount and on every `dock://attach`.
    #[test]
    fn captures_sent_before_the_page_listens_wait_in_order_and_are_taken_once() {
        let mut q = Attachments::default();
        q.push(att("/a.png"));
        q.push(att("/b.mp4"));
        assert_eq!(q.take(), [att("/a.png"), att("/b.mp4")]);
        assert!(
            q.take().is_empty(),
            "a second take finds nothing: no capture is attached twice"
        );
    }

    /// Sending the same capture twice (a double click on the card's button) is one chip, not two.
    #[test]
    fn the_same_capture_is_queued_once() {
        let mut q = Attachments::default();
        q.push(att("/a.png"));
        q.push(att("/a.png"));
        assert_eq!(q.take().len(), 1);
        q.push(att("/a.png"));
        assert_eq!(q.take().len(), 1, "after it was taken it may be sent again");
    }

    /// Contract with the page: `dock_take_attachments` items are `{path, kind}` with `kind` the lowercase capture
    /// kind (`Kind`'s serialization), which `DockAttachment` reads.
    #[test]
    fn an_attachment_serializes_to_the_shape_the_page_reads() {
        let v = serde_json::to_value(att("/a.png")).unwrap();
        assert_eq!(v, json!({"path": "/a.png", "kind": "screenshot"}));
    }

    /// Contract the other way: the page reads these exact keys from `dock_prefs` / `dock://prefs`.
    #[test]
    fn prefs_serialize_with_the_keys_the_page_reads() {
        let v = serde_json::to_value(DockPrefs::default()).unwrap();
        assert_eq!(
            v,
            json!({"enabled": true, "summon": "ctrl-alt-space", "toggle": "ctrl-alt-d"})
        );
    }

    /// The hide/show hotkey is a separate stroke and must never collide with the summon key (the plugin would
    /// refuse the second registration and the user would lose one of them with a confusing error).
    #[test]
    fn the_toggle_and_summon_keys_must_differ() {
        let same = DockPrefs {
            toggle: "ctrl-alt-space".into(),
            ..DockPrefs::default()
        };
        assert!(same.validate().unwrap_err().contains("same"));
        let none = DockPrefs {
            toggle: String::new(),
            summon: String::new(),
            ..DockPrefs::default()
        };
        assert!(none.validate().is_ok(), "two empty keys are not a clash");
        assert!(DockPrefs::default().validate().is_ok());
    }

    /// A file written before the toggle key existed has no `toggle`: it gets the default, and `enabled` (the
    /// persisted hidden state) is kept as the user left it.
    #[test]
    fn an_older_prefs_file_gets_the_default_toggle_and_keeps_the_hidden_state() {
        let p: DockPrefs =
            serde_json::from_value(json!({"enabled": false, "summon": "cmd-alt-k"})).unwrap();
        assert!(!p.enabled);
        assert_eq!(p.toggle, "ctrl-alt-d");
        assert_eq!(p.summon, "cmd-alt-k");
    }

    /// Hiding is a pref flip, nothing else: the other fields survive, and flipping twice is the identity.
    #[test]
    fn toggling_flips_only_enabled() {
        let p = DockPrefs {
            summon: "cmd-alt-k".into(),
            ..DockPrefs::default()
        };
        let hidden = p.toggled();
        assert!(!hidden.enabled);
        assert_eq!(hidden.summon, "cmd-alt-k");
        assert_eq!(hidden.toggled(), p);
    }

    /// What Settings writes (`settings.json` → `dock`) and an older or hand-edited file read back.
    #[test]
    fn prefs_read_partial_and_broken_files_as_defaults() {
        let only_off: DockPrefs = serde_json::from_value(json!({"enabled": false})).unwrap();
        assert!(!only_off.enabled);
        assert_eq!(
            only_off.summon, "ctrl-alt-space",
            "a missing key keeps its default"
        );
        let none: DockPrefs = serde_json::from_value(json!({})).unwrap();
        assert_eq!(none, DockPrefs::default());
        assert!(serde_json::from_value::<DockPrefs>(json!({"enabled": "yes"})).is_err());
    }

    #[test]
    fn the_default_keys_are_valid_accelerators() {
        let d = DockPrefs::default();
        assert_eq!(
            crate::hotkeys::accelerator(&d.summon).unwrap().as_deref(),
            Some("Control+Alt+Space")
        );
        assert_eq!(
            crate::hotkeys::accelerator(&d.toggle).unwrap().as_deref(),
            Some("Control+Alt+d")
        );
        assert!(d.validate().is_ok());
    }

    /// The hide key is the only way back while the pill is hidden, so it stays registered then; the summon key
    /// has nothing to open while the pill is hidden and is released (it would steal ⌃⌥Space from other apps).
    #[test]
    fn a_hidden_dock_keeps_only_the_key_that_brings_it_back() {
        let on = DockPrefs::default();
        let keys =
            |p: &DockPrefs| -> Vec<Action> { wanted_keys(p).into_iter().map(|(a, _)| a).collect() };
        assert_eq!(keys(&on), [Action::Summon, Action::Toggle]);
        assert_eq!(keys(&on.toggled()), [Action::Toggle]);
        let no_toggle = DockPrefs {
            toggle: String::new(),
            ..DockPrefs::default()
        };
        assert_eq!(keys(&no_toggle), [Action::Summon]);
        // hidden with no hide key: nothing to register (the tray item is the way back)
        assert!(wanted_keys(&no_toggle.toggled()).is_empty());
    }

    /// The pill's right-click menu: hide is always there (it is the point), and the label carries the hotkey that
    /// does the same, so the menu teaches it.
    #[test]
    fn the_pill_menu_can_hide_the_dock_and_shows_its_hotkey() {
        let items = pill_menu(&DockPrefs::default());
        let hide = items.iter().find(|i| i.id == MENU_HIDE).expect("hide item");
        assert_eq!(hide.label, "Hide Agent Dock  ⌃⌥D");
        let none = DockPrefs {
            toggle: String::new(),
            ..DockPrefs::default()
        };
        let hide = pill_menu(&none)
            .into_iter()
            .find(|i| i.id == MENU_HIDE)
            .unwrap();
        assert_eq!(hide.label, "Hide Agent Dock", "no hotkey, no hint");
        assert!(items.iter().any(|i| i.id == MENU_OPEN));
    }

    #[test]
    fn a_stroke_reads_as_mac_glyphs() {
        assert_eq!(glyphs("ctrl-alt-d"), "⌃⌥D");
        assert_eq!(glyphs("cmd-shift-s"), "⇧⌘S");
        assert_eq!(glyphs("ctrl-alt-space"), "⌃⌥Space");
        assert_eq!(glyphs(""), "");
    }

    const SCREEN: (f64, f64, f64, f64) = (0.0, 0.0, 3024.0, 1964.0);

    #[test]
    fn the_pill_sits_bottom_centre_above_the_edge() {
        let (x, y) = place(None, PILL, SCREEN, 2.0);
        assert_eq!(x, (3024.0 - 640.0) / 2.0);
        assert_eq!(y, 1964.0 - 80.0 - 56.0);
    }

    #[test]
    fn a_dragged_position_is_kept_while_it_is_on_that_screen_and_clamped_inside_it() {
        assert_eq!(
            place(Some((100.0, 200.0)), PILL, SCREEN, 2.0),
            (100.0, 200.0)
        );
        let (x, y) = place(Some((3000.0, 1960.0)), PILL, SCREEN, 2.0);
        assert_eq!(
            (x, y),
            (3024.0 - 640.0 - 16.0, 1964.0 - 80.0 - 16.0),
            "kept inside the margin"
        );
    }

    /// An unplugged monitor leaves the saved position outside every screen: the pill comes back instead of vanishing.
    #[test]
    fn a_position_from_a_disconnected_monitor_falls_back_to_bottom_centre() {
        let gone = Some((-2592.0, -208.0));
        assert_eq!(
            place(gone, PILL, SCREEN, 2.0),
            place(None, PILL, SCREEN, 2.0)
        );
    }

    #[test]
    fn a_window_taller_than_the_screen_still_has_a_position() {
        let (x, y) = place(None, (520.0, 4000.0), SCREEN, 1.0);
        assert!(x >= 0.0 && y >= 0.0);
    }

    /// The pill sits at the bottom of the screen: opening the composer must grow it UP, with the same centre line,
    /// not push it off the bottom edge.
    #[test]
    fn growing_keeps_the_bottom_centre_and_grows_upward() {
        let pill = (1352.0, 1828.0, 640.0, 80.0);
        let (x, y) = regrow(pill, (1040.0, 300.0), SCREEN, 2.0);
        assert_eq!(x + 520.0, 1352.0 + 320.0, "same centre line");
        assert_eq!(y + 300.0, 1828.0 + 80.0, "same bottom edge");
    }

    #[test]
    fn growing_near_a_screen_edge_is_pulled_back_inside_it() {
        let at_right = (3024.0 - 640.0 - 16.0, 1828.0, 640.0, 80.0);
        let (x, _) = regrow(at_right, (1040.0, 300.0), SCREEN, 2.0);
        assert!(x + 1040.0 <= 3024.0 - 16.0, "{x}");
        let (_, y) = regrow((1352.0, 20.0, 640.0, 80.0), (1040.0, 300.0), SCREEN, 2.0);
        assert!(
            y >= 16.0,
            "a pill at the top edge cannot grow upward off the screen: {y}"
        );
    }

    #[test]
    fn shrinking_back_returns_to_the_pill_position() {
        let pill = (1352.0, 1828.0, 640.0, 80.0);
        let big = regrow(pill, (1040.0, 300.0), SCREEN, 2.0);
        let back = regrow((big.0, big.1, 1040.0, 300.0), (640.0, 80.0), SCREEN, 2.0);
        assert_eq!(back, (1352.0, 1828.0));
    }

    #[test]
    fn the_second_display_is_placed_in_its_own_coordinates() {
        let second = (3024.0, 0.0, 2560.0, 1440.0);
        let (x, y) = place(None, PILL, second, 1.0);
        assert_eq!(x, 3024.0 + (2560.0 - 320.0) / 2.0);
        assert_eq!(y, 1440.0 - 40.0 - 28.0);
    }
}
