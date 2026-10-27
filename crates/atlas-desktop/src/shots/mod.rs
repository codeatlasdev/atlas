//! Screenshots, screen recording and the quick-access overlay of the agent dock (see `docs/desktop.md`,
//! "Capture"). The window capture of Atlas's own windows (`capture/`) is a separate, older module with another
//! purpose (scripted recordings of the app); this one captures the user's screen.
//!
//! Layout: the pure rules (`geom`, `naming`, `prefs`, `history`, `gif`, `session`, `windows`, `surface`) are tested
//! everywhere; the macOS parts (`sck`, `mac`, this file's runner) sit beside them and every other platform answers
//! [`unavailable`]. The pages (`apps/desktop/src/routes/shots/*`) own rendering and pointer handling only; every
//! pixel is read here. Contract: the commands below and the events `shots://*`.

pub mod clock;
pub mod editor;
pub mod geom;
pub mod gif;
pub mod history;
pub mod keys;
#[cfg(target_os = "macos")]
pub mod mac;
pub mod naming;
#[cfg(target_os = "macos")]
pub mod ocr;
pub mod perf;
pub mod pin;
pub mod pixels;
pub mod prefs;
pub mod quick;
pub mod recopts;
pub mod record;
#[cfg(target_os = "macos")]
pub mod sck;
pub mod session;
pub mod surface;
pub mod timer;
#[cfg(target_os = "macos")]
pub mod video;
pub mod windows;

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use geom::Rect;
use history::{Entry, History};
use prefs::{CapturePrefs, Hotkey};
use record::RecordState;
use session::{Commit, Event, Pick};

/// Why a capture command did nothing on this platform.
#[cfg(not(target_os = "macos"))]
pub fn unavailable<T>() -> Result<T, String> {
    Err("Screen capture is not available on this platform yet (macOS only for now).".into())
}

/// What the capture module keeps between calls.
pub struct Shots {
    prefs: Mutex<CapturePrefs>,
    state: Mutex<session::State>,
    history: Mutex<History>,
    registered: crate::hotkeys::Registered,
    /// The frozen frames the overlay samples for its magnifier, raw RGBA per display id.
    #[cfg(target_os = "macos")]
    frozen: Mutex<std::collections::HashMap<u32, Frozen>>,
    /// What each overlay window shows, by display id; set when the overlay opens, cleared when it closes.
    overlay: Mutex<std::collections::HashMap<u32, OverlayInit>>,
    /// What the recording HUD shows now.
    record: Mutex<RecordState>,
    /// The region the overlay committed for a recording, until its countdown starts.
    pending_record: Mutex<Option<(u32, Option<Rect>, bool)>>,
    /// The last region any mode committed, for "capture previous area" (kept in memory only: a restart forgets it,
    /// the displays may have changed).
    last_area: Mutex<Option<(Rect, u32)>>,
    /// The recording in progress.
    #[cfg(target_os = "macos")]
    live: Mutex<Option<record_mac::Live>>,
}

#[cfg(target_os = "macos")]
struct Frozen {
    rgba: Vec<u8>,
    size: (u32, u32),
}

/// Sent to the overlay page (camelCase).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayInit {
    pub display: DisplayInfo,
    pub mode: Pick,
    pub windows: Vec<OverlayWindow>,
    pub cursor: Cursor,
    /// What the All-in-One HUD shows first.
    pub hud: Hud,
}

/// What the overlay's magnifier shows: the block as a PNG `data:` URL and the colour of its middle pixel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loupe {
    pub url: String,
    /// `#rrggbb` of the pixel under the pointer, so the page decodes no image to print it.
    pub hex: String,
}

/// The HUD's starting point: the mode it remembers, the self-timer, whether "previous area" has one.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hud {
    pub last_mode: prefs::LastMode,
    pub timer_secs: u32,
    pub has_previous: bool,
    /// `still.capture_on_release`: a dragged region commits on release.
    pub capture_on_release: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: u32,
    pub frame: Rect,
    pub scale: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayWindow {
    pub id: u32,
    pub frame: Rect,
    pub title: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Cursor {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickState {
    pub entry: Entry,
    pub thumb: String,
    pub dismiss_secs: u32,
    pub copied: bool,
}

fn history_path() -> std::path::PathBuf {
    atlas_core::paths::atlas_home().join("desktop/capture-history.json")
}

impl Shots {
    pub fn load() -> Self {
        let mut h = std::fs::read_to_string(history_path())
            .map(|t| History::parse(&t))
            .unwrap_or_default();
        h.prune(|p| p.exists());
        Self {
            prefs: Mutex::new(CapturePrefs::load()),
            state: Mutex::new(session::State::Idle),
            history: Mutex::new(h),
            registered: crate::hotkeys::Registered::default(),
            #[cfg(target_os = "macos")]
            frozen: Mutex::new(Default::default()),
            overlay: Mutex::new(Default::default()),
            record: Mutex::new(RecordState::idle()),
            pending_record: Mutex::new(None),
            last_area: Mutex::new(None),
            #[cfg(target_os = "macos")]
            live: Mutex::new(None),
        }
    }

    /// The capture keys in force (the tray submenu shows them).
    pub fn keys(&self) -> prefs::Keys {
        self.prefs.lock().keys.clone()
    }

    fn save_history(&self) {
        let h = self.history.lock();
        if let Ok(text) = serde_json::to_string(&*h) {
            if let Some(dir) = history_path().parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            // A history that cannot be written is not worth failing a capture over.
            let _ = std::fs::write(history_path(), text);
        }
    }
}

/// The strokes the dock owns, which a capture key must not reuse.
fn dock_strokes(app: &AppHandle) -> Vec<(&'static str, String)> {
    app.try_state::<crate::dock::DockState>()
        .map(|d| {
            let p = d.prefs();
            vec![("Message an agent", p.summon), ("Hide the dock", p.toggle)]
        })
        .unwrap_or_default()
}

type Handler = Box<dyn Fn(&AppHandle) + Send + Sync>;

/// Register the capture hotkeys for `prefs` (none when disabled or in an agent instance).
fn apply_hotkeys(app: &AppHandle, prefs: &CapturePrefs) -> Result<(), String> {
    let state = app.state::<Shots>();
    let keys: Vec<(String, Handler)> =
        if !prefs.enabled || !crate::agent_mode::Mode::current().focus_allowed() {
            Vec::new()
        } else {
            Hotkey::ALL
                .iter()
                .map(|h| {
                    let h = *h;
                    let f: Handler = Box::new(move |app| feed(app, Event::Hotkey(h)));
                    (prefs.keys.get(h).to_string(), f)
                })
                .collect()
        };
    state.registered.apply(app, keys)
}

/// Build the state, register the hotkeys. Called once from `main`.
pub fn start(app: &AppHandle) {
    app.manage(Shots::load());
    let prefs = app.state::<Shots>().prefs.lock().clone();
    if let Err(e) = apply_hotkeys(app, &prefs) {
        eprintln!("shots: hotkeys: {e}");
    }
}

/// What a recording's countdown needs from `commit` in `state`: `(display, region or whole display, gif)`. `None`
/// when the commit does not start a recording (a screenshot, a text read, a window, no overlay).
pub fn pending_of(state: &session::State, commit: &Commit) -> Option<(u32, Option<Rect>, bool)> {
    use session::Action as A;
    let session::State::Picking(p) = state else {
        return None;
    };
    let action = if *p == Pick::All {
        match commit {
            Commit::Region { action, .. } | Commit::Screen { action, .. } => *action,
            Commit::Window { .. } => return None,
        }
    } else {
        A::of_pick(*p)
    };
    let gif = match action {
        A::Record => false,
        A::Gif => true,
        A::Capture | A::Text => return None,
    };
    match commit {
        Commit::Region { rect, display, .. } => Some((*display, Some(*rect), gif)),
        // only the HUD has a screen button
        Commit::Screen { display, .. } if *p == Pick::All => Some((*display, None, gif)),
        Commit::Screen { .. } | Commit::Window { .. } => None,
    }
}

/// The HUD mode a commit leaves behind, so the next All-in-One opens on it.
pub fn last_mode_of(c: &Commit) -> prefs::LastMode {
    use prefs::LastMode as L;
    use session::Action as A;
    let (action, whole) = match c {
        Commit::Region { action, .. } => (*action, L::Region),
        Commit::Window { action, .. } => (*action, L::Window),
        Commit::Screen { action, .. } => (*action, L::Screen),
    };
    match action {
        A::Capture => whole,
        A::Record => L::Record,
        A::Gif => L::Gif,
        A::Text => L::Text,
    }
}

/// Run `event` through the state machine and execute its effects off the caller's thread (ScreenCaptureKit blocks).
///
/// `Hotkey::Previous` is turned into [`Event::Previous`] here (the machine owns no stored area); with none stored
/// the person is told instead.
pub fn feed(app: &AppHandle, event: Event) {
    let shots = app.state::<Shots>();
    let event = match event {
        Event::Hotkey(Hotkey::Previous) => match previous_event(&shots.last_area.lock()) {
            Ok(e) => e,
            Err(msg) => {
                let _ = tauri::Emitter::emit(app, "shots://error", msg);
                return;
            }
        },
        e => e,
    };
    let (next, effects) = {
        let mut st = shots.state.lock();
        if let Event::Commit(c) = &event {
            // A record/gif commit starts a countdown in the next step; the effect carries no region, so keep it.
            if let Some(p) = pending_of(&st, c) {
                *shots.pending_record.lock() = Some(p);
            }
            if matches!(*st, session::State::Picking(_)) {
                remember_area(&mut shots.last_area.lock(), c);
                remember_mode(app, last_mode_of(c));
            }
        }
        let (n, fx) = session::step(&st, event);
        *st = n.clone();
        (n, fx)
    };
    let _ = next;
    if effects.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        for fx in effects {
            #[cfg(target_os = "macos")]
            runner::run(&app, fx);
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (&app, fx);
            }
        }
    });
}

/// Remember the HUD's last mode in the prefs (memory and `settings.json`), only when it changed. A write that
/// fails is not worth interrupting a capture for.
fn remember_mode(app: &AppHandle, mode: prefs::LastMode) {
    let shots = app.state::<Shots>();
    let mut p = shots.prefs.lock();
    if p.still.last_mode == mode {
        return;
    }
    p.still.last_mode = mode;
    let mut patch = serde_json::Map::new();
    if let Ok(v) = serde_json::to_value(&*p) {
        patch.insert("capture".into(), v);
        let _ = crate::shell_env::settings_patch(patch);
    }
}

// ───────────────────────────── commands ─────────────────────────────

#[tauri::command]
pub fn shots_prefs(state: State<'_, Shots>) -> CapturePrefs {
    state.prefs.lock().clone()
}

#[tauri::command]
pub fn shots_presets() -> Vec<prefs::Preset> {
    prefs::presets()
}

#[tauri::command]
pub fn shots_prefs_set(app: AppHandle, prefs: CapturePrefs) -> Result<CapturePrefs, String> {
    let state = app.state::<Shots>();
    let dock = dock_strokes(&app);
    let others: Vec<(&str, &str)> = dock.iter().map(|(n, s)| (*n, s.as_str())).collect();
    prefs.validate(&others)?;
    let before = state.prefs.lock().clone();
    apply_hotkeys(&app, &prefs).inspect_err(|_| {
        let _ = apply_hotkeys(&app, &before);
    })?;
    let mut patch = serde_json::Map::new();
    patch.insert(
        "capture".into(),
        serde_json::to_value(&prefs).map_err(|e| e.to_string())?,
    );
    crate::shell_env::settings_patch(patch).inspect_err(|_| {
        let _ = apply_hotkeys(&app, &before);
    })?;
    if !prefs.folder.trim().is_empty() {
        std::fs::create_dir_all(prefs.dir())
            .map_err(|e| format!("{}: {e}", prefs.dir().display()))?;
    }
    state.history.lock().trim(prefs.history as usize);
    state.save_history();
    *state.prefs.lock() = prefs.clone();
    crate::tray::refresh_capture(&app, &prefs.keys);
    Ok(prefs)
}

/// Debug builds only (`desktop:dev`, agent instances): start the capture `mode` as its hotkey would, because an
/// agent instance owns no global hotkey. `atlas desktop debug eval 'return await window.__TAURI_INTERNALS__.invoke("shots_debug_start",{mode:"fullscreen"})'`.
///
/// `record` and `gif` normally need the overlay to pick a region, and an agent instance never builds it (it covers
/// real displays): there the whole first display is "dragged" for you, `rect` (global points `[x, y, w, h]`)
/// narrowing it, and the session goes through the same states and effects as a real drag.
#[cfg(debug_assertions)]
#[tauri::command]
pub fn shots_debug_start(
    app: AppHandle,
    mode: String,
    rect: Option<[f64; 4]>,
) -> Result<(), String> {
    let h = Hotkey::from_name(&mode).ok_or_else(|| format!("unknown capture mode {mode:?}"))?;
    #[cfg(target_os = "macos")]
    if matches!(h, Hotkey::Record | Hotkey::Gif | Hotkey::Ocr)
        && !crate::agent_mode::Mode::current().focus_allowed()
    {
        let content = sck::Content::fetch()?;
        let d = content.displays.first().ok_or("no display")?;
        let r = rect.map_or(d.frame, |[x, y, w, h]| Rect::new(x, y, w, h));
        let pick = match h {
            Hotkey::Gif => Pick::Gif,
            Hotkey::Ocr => Pick::Text,
            _ => Pick::Record,
        };
        *app.state::<Shots>().state.lock() = session::State::Picking(pick);
        feed(
            &app,
            Event::Commit(Commit::Region {
                rect: r,
                display: d.id,
                action: session::Action::of_pick(pick),
            }),
        );
        return Ok(());
    }
    let _ = rect;
    feed(&app, Event::Hotkey(h));
    Ok(())
}

/// Debug builds only: how long freezing the first display takes the way the overlay does it (`overlay.freeze`:
/// ScreenCaptureKit straight to RGBA) and the way it used to (`overlay.freeze-png`: PNG file, read, decode), recorded
/// in the perf table (`shots_perf`). The overlay itself is never built in an agent instance, so this is how the
/// freeze, the biggest part of hotkey-to-overlay, is measured there.
#[cfg(all(debug_assertions, target_os = "macos"))]
#[tauri::command]
pub fn shots_debug_freeze(app: AppHandle) -> Result<(), String> {
    let content = sck::Content::fetch()?;
    let d = content.displays.first().ok_or("no display")?;
    let own = runner::own_window_ids(&app, &content);
    let target = || sck::Target::Display {
        content: &content,
        id: d.id,
        region: None,
    };
    let t = std::time::Instant::now();
    sck::still_rgba(&target(), &own, sck::StillOpts::default())?;
    perf::mark("overlay.freeze", t);
    let png = std::env::temp_dir().join(format!(
        "atlas-shots-freeze-probe-{}.png",
        std::process::id()
    ));
    let t = std::time::Instant::now();
    sck::still_with(&target(), &own, &png, sck::StillOpts::default())?;
    let bytes = std::fs::read(&png).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&png);
    pixels::decode_png(&bytes?)?;
    perf::mark("overlay.freeze-png", t);
    Ok(())
}

#[derive(Serialize)]
pub struct Permission {
    pub granted: bool,
}

#[tauri::command]
pub fn shots_permission() -> Result<Permission, String> {
    #[cfg(target_os = "macos")]
    return Ok(Permission {
        granted: sck::permission_granted(),
    });
    #[cfg(not(target_os = "macos"))]
    unavailable()
}

#[tauri::command]
pub fn shots_permission_request() -> Result<Permission, String> {
    #[cfg(target_os = "macos")]
    return Ok(Permission {
        granted: sck::request_permission(),
    });
    #[cfg(not(target_os = "macos"))]
    unavailable()
}

#[tauri::command]
pub fn shots_open_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::open_url(
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
    );
    #[cfg(not(target_os = "macos"))]
    unavailable()
}

/// The HUD's timer button: Off → 3 → 5 → 10 → Off, saved like any other capture setting, and announced to every
/// display's overlay (`shots://hud-timer`, the seconds). Returns the new seconds.
#[tauri::command]
pub fn shots_timer_cycle(app: AppHandle) -> Result<u32, String> {
    let shots = app.state::<Shots>();
    let mut p = shots.prefs.lock();
    let next = prefs::next_timer(p.still.timer_secs);
    let mut patch = serde_json::Map::new();
    let mut updated = p.clone();
    updated.still.timer_secs = next;
    patch.insert(
        "capture".into(),
        serde_json::to_value(&updated).map_err(|e| e.to_string())?,
    );
    crate::shell_env::settings_patch(patch)?;
    p.still.timer_secs = next;
    drop(p);
    let _ = tauri::Emitter::emit(&app, "shots://hud-timer", next);
    Ok(next)
}

/// Open System Settings > Keyboard > Keyboard Shortcuts so the person can turn macOS's own screenshot keys off
/// themselves. Atlas never writes `com.apple.symbolichotkeys` or changes the setting: it only opens the pane.
#[tauri::command]
pub fn shots_open_shortcuts_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::open_url("x-apple.systempreferences:com.apple.Keyboard-Settings.extension");
    #[cfg(not(target_os = "macos"))]
    unavailable()
}

/// The HUD's "Previous area" button (and any page that wants it): the same as pressing that capture key.
#[tauri::command]
pub fn shots_overlay_previous(app: AppHandle) {
    feed(&app, Event::Hotkey(Hotkey::Previous));
}

#[tauri::command]
pub async fn shots_pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |p| {
        let _ = tx.send(p.map(|p| p.to_string()));
    });
    rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn shots_history(state: State<'_, Shots>) -> History {
    let mut h = state.history.lock();
    if h.prune(|p| p.exists()) > 0 {
        drop(h);
        state.save_history();
        return state.history.lock().clone();
    }
    h.clone()
}

#[tauri::command]
pub fn shots_history_remove(state: State<'_, Shots>, path: String) {
    state.history.lock().remove(std::path::Path::new(&path));
    state.save_history();
}

#[tauri::command]
pub fn shots_history_reveal(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::reveal(std::path::Path::new(&path));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        unavailable()
    }
}

#[tauri::command]
pub fn shots_history_open(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return mac::open(std::path::Path::new(&path));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        unavailable()
    }
}

// overlay page

#[tauri::command]
pub fn shots_overlay_init(
    window: tauri::WebviewWindow,
    state: State<'_, Shots>,
) -> Result<OverlayInit, String> {
    let id: u32 = window
        .label()
        .strip_prefix(surface::OVERLAY_PREFIX)
        .and_then(|d| d.parse().ok())
        .ok_or("only a capture overlay window can ask for its display")?;
    state
        .overlay
        .lock()
        .get(&id)
        .cloned()
        .ok_or_else(|| "no capture is in progress".to_string())
}

/// The overlay page's commit payload (`kind`: `region` | `window` | `screen`, plus `action`: `capture` | `record` |
/// `gif` | `text`, default `capture`) as a session [`Commit`].
pub fn parse_commit(commit: &serde_json::Value) -> Result<Commit, String> {
    let action = match commit.get("action") {
        None | Some(serde_json::Value::Null) => session::Action::Capture,
        Some(a) => serde_json::from_value(a.clone()).map_err(|e| format!("action: {e}"))?,
    };
    let display = || {
        commit["display"]
            .as_u64()
            .map(|d| d as u32)
            .ok_or("display is required")
    };
    Ok(match commit["kind"].as_str() {
        Some("region") => Commit::Region {
            rect: serde_json::from_value(commit["rect"].clone())
                .map_err(|e| format!("rect: {e}"))?,
            display: display()?,
            action,
        },
        Some("window") => Commit::Window {
            id: commit["windowId"].as_u64().ok_or("windowId is required")? as u32,
            action,
        },
        Some("screen") => Commit::Screen {
            display: display()?,
            action,
        },
        _ => return Err("commit.kind must be \"region\", \"window\" or \"screen\"".into()),
    })
}

/// Keep the region of a region commit as "the previous area"; windows and screens leave it alone.
pub fn remember_area(last: &mut Option<(Rect, u32)>, c: &Commit) {
    if let Some(a) = session::region_of(c) {
        *last = Some(a);
    }
}

/// The event for "capture previous area", or the sentence to show when there is none yet.
pub fn previous_event(last: &Option<(Rect, u32)>) -> Result<Event, String> {
    last.map(|(rect, display)| Event::Previous { rect, display })
        .ok_or_else(|| "No previous area yet: capture a region first.".to_string())
}

#[tauri::command]
pub fn shots_overlay_commit(app: AppHandle, commit: serde_json::Value) -> Result<(), String> {
    feed(&app, Event::Commit(parse_commit(&commit)?));
    Ok(())
}

#[tauri::command]
pub fn shots_overlay_cancel(app: AppHandle) {
    feed(&app, Event::Cancel);
}

/// The 15×15 px block around `(x, y)` (global points) of the frozen frame as a PNG data URL, with the colour of its
/// middle pixel; `None` when there is no frame (the page hides the magnifier).
#[tauri::command]
pub fn shots_overlay_loupe(state: State<'_, Shots>, x: f64, y: f64) -> Option<Loupe> {
    #[cfg(target_os = "macos")]
    {
        loupe::sample(&state, x, y)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, x, y);
        None
    }
}

// quick access

#[tauri::command]
pub fn shots_quick_hover(hovering: bool) {
    let _ = hovering;
}

#[tauri::command]
pub fn shots_record_state(state: State<'_, Shots>) -> RecordState {
    state.record.lock().clone()
}

// ───────────────────────────── macOS runner ─────────────────────────────

#[cfg(target_os = "macos")]
mod keys_mac;
#[cfg(target_os = "macos")]
mod loupe;
#[cfg(target_os = "macos")]
mod record_mac;
#[cfg(target_os = "macos")]
mod runner;

#[cfg(test)]
mod tests {
    use super::*;

    /// The page reads exactly these keys from `shots_overlay_init` (serialize the real value).
    #[test]
    fn overlay_init_serializes_with_the_keys_the_page_reads() {
        let v = serde_json::to_value(OverlayInit {
            display: DisplayInfo {
                id: 3,
                frame: Rect::new(0.0, 0.0, 1512.0, 982.0),
                scale: 2.0,
            },
            mode: Pick::Region,
            windows: vec![OverlayWindow {
                id: 9,
                frame: Rect::new(1.0, 2.0, 3.0, 4.0),
                title: "t".into(),
            }],
            cursor: Cursor { x: 5.0, y: 6.0 },
            hud: Hud {
                last_mode: prefs::LastMode::Region,
                timer_secs: 0,
                has_previous: false,
                capture_on_release: true,
            },
        })
        .unwrap();
        assert_eq!(v["mode"], "region");
        assert_eq!(v["hud"]["captureOnRelease"], true);
        assert_eq!(v["display"]["id"], 3);
        assert_eq!(v["display"]["frame"]["w"], 1512.0);
        assert_eq!(v["windows"][0]["title"], "t");
        assert_eq!(v["cursor"]["x"], 5.0);
    }

    /// The page reads exactly `url` and `hex` from `shots_overlay_loupe` (serialize the real value).
    #[test]
    fn the_loupe_serializes_with_the_keys_the_page_reads() {
        let v = serde_json::to_value(Loupe {
            url: "data:image/png;base64,AA".into(),
            hex: "#0aabff".into(),
        })
        .unwrap();
        assert_eq!(v["url"], "data:image/png;base64,AA");
        assert_eq!(v["hex"], "#0aabff");
        assert_eq!(v.as_object().unwrap().len(), 2);
    }

    #[test]
    fn the_quick_and_record_states_use_camel_case() {
        let q = serde_json::to_value(QuickState {
            entry: Entry {
                path: "/a.png".into(),
                kind: naming::Kind::Screenshot,
                at: 1,
                width: 2,
                height: 3,
                bytes: 4,
                secs: None,
            },
            thumb: "data:".into(),
            dismiss_secs: 6,
            copied: true,
        })
        .unwrap();
        assert_eq!(q["dismissSecs"], 6);
        assert_eq!(q["entry"]["kind"], "screenshot");
        let r = serde_json::to_value(RecordState::default()).unwrap();
        assert_eq!(r["phase"], "idle");
        assert_eq!(r["maxMs"], clock::MAX_MS);
        assert!(r.get("elapsedMs").is_some());
    }

    // ───────── the page's commit payload (real JSON, as the overlay sends it) ─────────

    #[test]
    fn commits_parse_region_window_and_screen_with_their_action() {
        let c = parse_commit(&serde_json::json!({
            "kind": "region", "rect": {"x": 1.0, "y": 2.0, "w": 30.0, "h": 40.0}, "display": 7, "action": "text"
        }))
        .unwrap();
        assert_eq!(
            c,
            Commit::Region {
                rect: Rect::new(1.0, 2.0, 30.0, 40.0),
                display: 7,
                action: session::Action::Text
            }
        );
        assert_eq!(
            parse_commit(
                &serde_json::json!({"kind": "window", "windowId": 9, "action": "capture"})
            )
            .unwrap(),
            Commit::Window {
                id: 9,
                action: session::Action::Capture
            }
        );
        assert_eq!(
            parse_commit(&serde_json::json!({"kind": "screen", "display": 2, "action": "record"}))
                .unwrap(),
            Commit::Screen {
                display: 2,
                action: session::Action::Record
            }
        );
    }

    /// An older page (or a plain overlay) sends no action: it means a screenshot, and a plain overlay ignores it
    /// anyway.
    #[test]
    fn a_commit_without_an_action_is_a_capture() {
        let c = parse_commit(&serde_json::json!({"kind": "window", "windowId": 1})).unwrap();
        assert_eq!(
            c,
            Commit::Window {
                id: 1,
                action: session::Action::Capture
            }
        );
    }

    #[test]
    fn bad_commits_are_refused_with_the_field_that_is_wrong() {
        for (v, needle) in [
            (serde_json::json!({"kind": "lasso"}), "kind"),
            (serde_json::json!({"kind": "region", "display": 1}), "rect"),
            (
                serde_json::json!({"kind": "region", "rect": {"x":0,"y":0,"w":1,"h":1}}),
                "display",
            ),
            (serde_json::json!({"kind": "window"}), "windowId"),
            (serde_json::json!({"kind": "screen"}), "display"),
            (
                serde_json::json!({"kind": "screen", "display": 1, "action": "fly"}),
                "action",
            ),
        ] {
            let e = parse_commit(&v).unwrap_err();
            assert!(e.contains(needle), "{v}: {e}");
        }
    }

    /// "Capture previous area" falls back to a message when there is none yet; after a region commit it has one.
    #[test]
    fn the_previous_area_is_the_last_region_committed_by_any_mode() {
        let mut last = None;
        assert_eq!(
            previous_event(&last).unwrap_err(),
            "No previous area yet: capture a region first."
        );
        remember_area(
            &mut last,
            &Commit::Window {
                id: 1,
                action: session::Action::Capture,
            },
        );
        assert!(last.is_none(), "a window is not an area");
        let r = Rect::new(1.0, 2.0, 3.0, 4.0);
        remember_area(
            &mut last,
            &Commit::Region {
                rect: r,
                display: 5,
                action: session::Action::Text,
            },
        );
        assert_eq!(
            previous_event(&last).unwrap(),
            Event::Previous {
                rect: r,
                display: 5
            }
        );
        // a window after it does not erase it
        remember_area(
            &mut last,
            &Commit::Window {
                id: 1,
                action: session::Action::Capture,
            },
        );
        assert!(last.is_some());
    }

    #[test]
    fn the_overlay_init_carries_what_the_hud_needs() {
        let v = serde_json::to_value(OverlayInit {
            display: DisplayInfo {
                id: 3,
                frame: Rect::new(0.0, 0.0, 10.0, 10.0),
                scale: 2.0,
            },
            mode: Pick::All,
            windows: vec![],
            cursor: Cursor { x: 0.0, y: 0.0 },
            hud: Hud {
                last_mode: prefs::LastMode::Window,
                timer_secs: 5,
                has_previous: true,
                capture_on_release: false,
            },
        })
        .unwrap();
        assert_eq!(v["mode"], "all");
        assert_eq!(v["hud"]["captureOnRelease"], false);
        assert_eq!(v["hud"]["lastMode"], "window");
        assert_eq!(v["hud"]["timerSecs"], 5);
        assert_eq!(v["hud"]["hasPrevious"], true);
    }

    /// The countdown reads the region from `pending_record`: set for a record/gif commit of a region OR a whole
    /// screen, in the plain overlays and in the HUD, never for a screenshot.
    #[test]
    fn the_pending_recording_follows_the_commit_not_only_the_overlay_mode() {
        use session::Action as A;
        let r = Rect::new(1.0, 2.0, 3.0, 4.0);
        let region = |action| Commit::Region {
            rect: r,
            display: 3,
            action,
        };
        let screen = |action| Commit::Screen { display: 3, action };
        // plain record / gif overlays
        assert_eq!(
            pending_of(&session::State::Picking(Pick::Record), &region(A::Capture)),
            Some((3, Some(r), false))
        );
        assert_eq!(
            pending_of(&session::State::Picking(Pick::Gif), &region(A::Capture)),
            Some((3, Some(r), true))
        );
        // the HUD
        let all = session::State::Picking(Pick::All);
        assert_eq!(
            pending_of(&all, &region(A::Record)),
            Some((3, Some(r), false))
        );
        assert_eq!(pending_of(&all, &region(A::Gif)), Some((3, Some(r), true)));
        assert_eq!(pending_of(&all, &screen(A::Record)), Some((3, None, false)));
        assert_eq!(pending_of(&all, &screen(A::Gif)), Some((3, None, true)));
        // a screenshot or a text read records nothing, and neither does a region overlay
        assert_eq!(pending_of(&all, &region(A::Capture)), None);
        assert_eq!(pending_of(&all, &region(A::Text)), None);
        assert_eq!(
            pending_of(&session::State::Picking(Pick::Region), &region(A::Record)),
            None
        );
        // a window is never recorded directly
        assert_eq!(
            pending_of(
                &all,
                &Commit::Window {
                    id: 1,
                    action: A::Record
                }
            ),
            None
        );
        assert_eq!(pending_of(&session::State::Idle, &region(A::Record)), None);
    }

    /// The HUD reopens on what the person last did: the mode of a commit, not of a hotkey.
    #[test]
    fn the_last_mode_is_what_the_commit_did() {
        use prefs::LastMode as L;
        use session::Action as A;
        let r = Rect::new(0.0, 0.0, 9.0, 9.0);
        let reg = |action| Commit::Region {
            rect: r,
            display: 1,
            action,
        };
        assert_eq!(last_mode_of(&reg(A::Capture)), L::Region);
        assert_eq!(last_mode_of(&reg(A::Record)), L::Record);
        assert_eq!(last_mode_of(&reg(A::Gif)), L::Gif);
        assert_eq!(last_mode_of(&reg(A::Text)), L::Text);
        assert_eq!(
            last_mode_of(&Commit::Window {
                id: 1,
                action: A::Capture
            }),
            L::Window
        );
        assert_eq!(
            last_mode_of(&Commit::Screen {
                display: 1,
                action: A::Capture
            }),
            L::Screen
        );
        assert_eq!(
            last_mode_of(&Commit::Screen {
                display: 1,
                action: A::Record
            }),
            L::Record
        );
    }
}
