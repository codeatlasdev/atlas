//! Tray icon and menu-bar popover. On macOS a left click on the icon toggles a borderless, translucent,
//! always-on-top window (`tray`, route `/tray`) under the icon; right click keeps a small
//! native menu. The window is built once, hidden, and only moved/shown afterwards.
//!
//! Linux: the icon is an AppIndicator/StatusNotifier item. Hosts deliver no click events (only the menu works),
//! so the menu is the whole UI there (Show / Hide, Quit; see [`lifecycle::tray_menu`]). The icon is drawn only
//! when something on the session bus hosts it, which [`reachable`] asks; `main.rs` uses that to decide whether
//! closing the window may hide it (`lifecycle.rs`).
//!
//! The popover has no stores of its own: the main window publishes a snapshot
//! (`tray_publish`), Rust caches it and forwards it as `tray://snapshot`; clicks come back
//! through `tray_action`, which focuses the main window and emits `tray://action` there.
//! Start/Stop run here through the same commands the main window uses.

use parking_lot::Mutex;
use serde_json::Value;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Rect, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::lifecycle::{self, TrayItem};

pub const LABEL: &str = "tray";
const WIDTH: f64 = 360.0;
const HEIGHT: f64 = 520.0;
/// Gap between the menu bar icon and the popover, logical px.
const GAP: f64 = 6.0;
/// Margin kept to the screen edges, logical px.
const MARGIN: f64 = 8.0;
/// A blur right after showing is the click that opened it settling, not the user leaving.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// Last snapshot the main window published, so a freshly shown popover paints at once.
#[derive(Default)]
pub struct TrayState {
    snapshot: Mutex<Value>,
    shown_at: Mutex<Option<std::time::Instant>>,
    /// Where the icon was when the popover was last opened (physical x, y, w, h), for [`room`].
    icon: Mutex<Option<(f64, f64, f64, f64)>>,
}

/// Top-left (physical px) of a popover of `size` (w, h) under `icon` (x, y, w, h),
/// centered on it, inside `screen` (x, y, w, h). `scale` converts the logical gap/margin.
pub fn place(
    icon: (f64, f64, f64, f64),
    size: (f64, f64),
    screen: (f64, f64, f64, f64),
    scale: f64,
) -> (f64, f64) {
    let (gap, margin) = (GAP * scale, MARGIN * scale);
    let x = icon.0 + icon.2 / 2.0 - size.0 / 2.0;
    let y = icon.1 + icon.3 + gap;
    let (sx, sy, sw, sh) = screen;
    // `max` last: on a screen smaller than the popover the left/top edge wins.
    let x = x.min(sx + sw - size.0 - margin).max(sx + margin);
    let y = y.min(sy + sh - size.1 - margin).max(sy + margin);
    (x, y)
}

/// Logical px of height the popover may use under `icon` (physical x, y, w, h) on `screen` (physical x, y, w, h):
/// from the icon's bottom edge plus the gap down to the screen's bottom margin. Never negative.
pub fn room(icon: (f64, f64, f64, f64), screen: (f64, f64, f64, f64), scale: f64) -> f64 {
    let below = (screen.1 + screen.3) - (icon.1 + icon.3);
    (below / scale - GAP - MARGIN).max(0.0)
}

/// Icon rect from a tray event, as physical (x, y, w, h).
fn rect_px(rect: &Rect, scale: f64) -> (f64, f64, f64, f64) {
    let p = rect.position.to_physical::<f64>(scale);
    let s = rect.size.to_physical::<f64>(scale);
    (p.x, p.y, s.width, s.height)
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let (theme, script) = crate::boot::window_boot(crate::boot::load_cache().as_ref());
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("tray".into()))
        .theme(theme)
        .initialization_script(script)
        .title("Atlas")
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .visible(false)
        .focused(false)
        .accept_first_mouse(true)
        .build()?;
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState, apply_vibrancy};
        // `Active`: the popover is never the "key" app window, but must stay translucent.
        let _ = apply_vibrancy(
            &win,
            NSVisualEffectMaterial::Popover,
            Some(NSVisualEffectState::Active),
            Some(14.0),
        );
    }
    let w = win.clone();
    let handle = app.clone();
    win.on_window_event(move |ev| {
        if let WindowEvent::Focused(false) = ev {
            let settled = handle
                .state::<TrayState>()
                .shown_at
                .lock()
                .is_none_or(|t| t.elapsed() > SETTLE);
            if settled {
                let _ = w.hide();
            }
        }
    });
    Ok(win)
}

fn show(app: &AppHandle, icon: Option<&Rect>) {
    let Some(win) = app.get_webview_window(LABEL) else {
        return;
    };
    if let Some(icon) = icon {
        let scale = win.scale_factor().unwrap_or(1.0);
        let icon = rect_px(icon, scale);
        *app.state::<TrayState>().icon.lock() = Some(icon);
        let size = win
            .outer_size()
            .map(|s| (f64::from(s.width), f64::from(s.height)))
            .unwrap_or((WIDTH * scale, HEIGHT * scale));
        let screen = app
            .monitor_from_point(icon.0 + icon.2 / 2.0, icon.1 + icon.3 / 2.0)
            .ok()
            .flatten()
            .map(|m| {
                let (p, s) = (m.position(), m.size());
                (
                    f64::from(p.x),
                    f64::from(p.y),
                    f64::from(s.width),
                    f64::from(s.height),
                )
            });
        let (x, y) = match screen {
            Some(screen) => place(icon, size, screen, scale),
            None => (icon.0 + icon.2 / 2.0 - size.0 / 2.0, icon.1 + icon.3),
        };
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    *app.state::<TrayState>().shown_at.lock() = Some(std::time::Instant::now());
    let _ = win.show();
    let _ = win.set_focus();
    let _ = win.emit("tray://shown", ());
}

fn toggle(app: &AppHandle, icon: &Rect) {
    match app.get_webview_window(LABEL) {
        Some(w) if w.is_visible().unwrap_or(false) => {
            let _ = w.hide();
        }
        _ => show(app, Some(icon)),
    }
}

const ID_OPEN: &str = "open";
const ID_TOGGLE: &str = "toggle";
const ID_QUIT: &str = "quit";

/// The tray's native menu, from [`lifecycle::tray_menu`]. Quit is a plain item handled in [`on_menu`]: the
/// predefined `quit` only exists on macOS (muda's GTK backend renders it as a disabled entry that never fires).
fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    for item in lifecycle::tray_menu(cfg!(target_os = "linux")) {
        match item {
            TrayItem::Open => menu.append(&MenuItem::with_id(
                app,
                ID_OPEN,
                "Open Atlas",
                true,
                None::<&str>,
            )?)?,
            TrayItem::ToggleWindow => menu.append(&MenuItem::with_id(
                app,
                ID_TOGGLE,
                "Show / Hide Atlas",
                true,
                None::<&str>,
            )?)?,
            TrayItem::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
            TrayItem::Quit => menu.append(&MenuItem::with_id(
                app,
                ID_QUIT,
                "Quit Atlas",
                true,
                None::<&str>,
            )?)?,
        }
    }
    Ok(menu)
}

/// Tray menu click. Quit goes through `AppHandle::exit` with an explicit code, which [`lifecycle::exit`] never blocks.
fn on_menu(app: &AppHandle, id: &str) {
    match id {
        ID_OPEN => crate::show_main(app),
        ID_TOGGLE => toggle_main(app),
        ID_QUIT => app.exit(0),
        _ => {}
    }
}

/// Hide the main window when it is showing, bring it forward when it is not (hidden, minimized or behind).
fn toggle_main(app: &AppHandle) {
    let showing = app
        .get_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    match app.get_window("main") {
        Some(w) if showing => {
            let _ = w.hide();
        }
        _ => crate::show_main(app),
    }
}

/// Is somebody on the session bus hosting StatusNotifier items (KDE Plasma, GNOME with the AppIndicator extension,
/// XFCE/Cinnamon/MATE/waybar trays)? Without one the icon is created but drawn nowhere. Every such host owns
/// `org.kde.StatusNotifierWatcher`; libayatana-appindicator itself only registers with it. Any failure to ask (no
/// session bus, no reply in 1.5 s) counts as "no host": the safe answer, because it makes closing quit.
#[cfg(target_os = "linux")]
fn tray_host_present() -> bool {
    use gio::prelude::*;
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>) else {
        return false;
    };
    bus.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&("org.kde.StatusNotifierWatcher",).to_variant()),
        Some(gio::glib::VariantTy::new("(b)").expect("static type string")),
        gio::DBusCallFlags::NONE,
        1500,
        None::<&gio::Cancellable>,
    )
    .ok()
    .and_then(|v| v.get::<(bool,)>())
    .is_some_and(|(owned,)| owned)
}

/// Tray hosts are a Linux concern; the macOS menu bar always exists.
#[cfg(not(target_os = "linux"))]
fn tray_host_present() -> bool {
    true
}

/// May the main window hide into the background right now? Asks the session bus each time (a host can appear after
/// the app starts: a GNOME extension enabled later, a panel that registers late at login). False only when [`start`]
/// ran and no host owns the StatusNotifierWatcher: then a hidden window could never be brought back. Without a tray
/// (agent instances, `agent_mode::tray_allowed`: none by design, driven over the debug bridge) it is true, which
/// keeps their close-hides behavior. Called on close/exit requests only, never in a loop.
pub fn reachable(app: &AppHandle) -> bool {
    app.try_state::<TrayState>().is_none() || tray_host_present()
}

/// The tray glyph: white on Linux (hosts draw the PNG as is on dark panels), the black template on macOS (tinted by
/// the system). Both are rendered from `assets/tray-template.svg` by `scripts/icons.ts`.
fn icon() -> tauri::Result<tauri::image::Image<'static>> {
    tauri::image::Image::from_bytes(if cfg!(target_os = "linux") {
        include_bytes!("../icons/tray-linux.png")
    } else {
        include_bytes!("../icons/tray.png")
    })
}

/// Create the popover window and the tray icon. On Linux the popover is built but never opened (no click events
/// there); it stays hidden, so the `tray_*` commands the page registers keep working unchanged.
pub fn start(app: &AppHandle) -> tauri::Result<()> {
    app.manage(TrayState::default());
    build(app)?;
    TrayIconBuilder::with_id("main")
        .icon(icon()?)
        .icon_as_template(!cfg!(target_os = "linux"))
        .menu(&menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| on_menu(app, ev.id().as_ref()))
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = ev
            {
                toggle(tray.app_handle(), &rect);
            }
        })
        .build(app)?;
    Ok(())
}

/// Main window → popover: the current state, cached for the next time it is shown.
#[tauri::command]
pub fn tray_publish(app: AppHandle, state: State<'_, TrayState>, snapshot: Value) {
    *state.snapshot.lock() = snapshot.clone();
    // The popover is built once and hidden: skip the IPC while nobody can see it.
    if app
        .get_webview_window(LABEL)
        .is_some_and(|w| w.is_visible().unwrap_or(false))
    {
        let _ = app.emit_to(LABEL, "tray://snapshot", snapshot);
    }
}

/// Popover: the last published snapshot (`null` before the main window sent one).
#[tauri::command]
pub fn tray_snapshot(state: State<'_, TrayState>) -> Value {
    state.snapshot.lock().clone()
}

#[tauri::command]
pub fn tray_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
    }
}

/// Popover: fit the window to its content (logical px), never taller than the room the screen leaves below the
/// menu bar icon (the page scrolls inside the rest). Returns the height it set so the page can size its list.
#[tauri::command]
pub fn tray_resize(app: AppHandle, state: State<'_, TrayState>, height: f64) -> f64 {
    let Some(w) = app.get_webview_window(LABEL) else {
        return height;
    };
    let height = match *state.icon.lock() {
        Some(icon) => {
            let scale = w.scale_factor().unwrap_or(1.0);
            let screen = w.current_monitor().ok().flatten().map(|m| {
                (
                    f64::from(m.position().x),
                    f64::from(m.position().y),
                    f64::from(m.size().width),
                    f64::from(m.size().height),
                )
            });
            screen.map_or(height, |s| height.min(room(icon, s, scale)))
        }
        None => height,
    };
    let _ = w.set_size(LogicalSize::new(WIDTH, height));
    height
}

/// Popover → main window: hide the popover, bring the app forward and hand the click to it.
#[tauri::command]
pub fn tray_action(app: AppHandle, action: Value) {
    tray_hide(app.clone());
    crate::show_main(&app);
    let _ = app.emit_to("main", "tray://action", action);
}

#[cfg(test)]
mod tests {
    use super::{place, room};

    const SCREEN: (f64, f64, f64, f64) = (0.0, 0.0, 3024.0, 1964.0);
    const SIZE: (f64, f64) = (720.0, 1040.0);

    #[test]
    fn centers_under_the_icon_with_a_gap() {
        let (x, y) = place((1500.0, 0.0, 48.0, 50.0), SIZE, SCREEN, 2.0);
        assert_eq!(x, 1500.0 + 24.0 - 360.0);
        assert_eq!(y, 50.0 + 12.0);
    }

    #[test]
    fn clamps_at_the_right_edge_of_the_screen() {
        // Icons sit at the right end of the menu bar: the popover must not leave the screen.
        let (x, _) = place((2980.0, 0.0, 40.0, 50.0), SIZE, SCREEN, 2.0);
        assert_eq!(x, 3024.0 - 720.0 - 16.0);
    }

    #[test]
    fn clamps_at_the_left_edge_and_on_a_second_monitor() {
        let (x, _) = place((10.0, 0.0, 40.0, 50.0), SIZE, SCREEN, 2.0);
        assert_eq!(x, 16.0);
        // A monitor left of the primary has a negative origin.
        let ext = (-2560.0, -200.0, 2560.0, 1440.0);
        let (x, y) = place((-60.0, -200.0, 40.0, 50.0), SIZE, ext, 2.0);
        assert_eq!(x, -2560.0 + 2560.0 - 720.0 - 16.0);
        assert_eq!(y, -200.0 + 50.0 + 12.0);
    }

    #[test]
    fn a_popover_taller_than_the_screen_keeps_its_top() {
        let (_, y) = place((100.0, 0.0, 40.0, 50.0), (720.0, 3000.0), SCREEN, 2.0);
        assert_eq!(y, 16.0);
    }

    // Incident (v1.5.0-alpha.7): the project list hung off the bottom of the popover with no scroll. The page asked
    // for up to 560 logical px whatever the display; the window must never be taller than the screen leaves room for,
    // and the page scrolls inside the rest.
    #[test]
    fn room_is_the_screen_below_the_icon_minus_gap_and_margin() {
        // 1964 physical px tall @2x = 982 logical; icon bottom at 50 physical -> 25 logical.
        let r = room((1500.0, 0.0, 48.0, 50.0), SCREEN, 2.0);
        assert_eq!(r, 982.0 - 25.0 - 6.0 - 8.0);
    }

    #[test]
    fn room_on_a_short_screen_is_small_but_never_negative() {
        let short = (0.0, 0.0, 2560.0, 700.0);
        assert_eq!(
            room((100.0, 0.0, 40.0, 50.0), short, 2.0),
            350.0 - 25.0 - 6.0 - 8.0
        );
        assert_eq!(room((100.0, 650.0, 40.0, 50.0), short, 2.0), 0.0);
    }

    #[test]
    fn room_follows_a_screen_that_does_not_start_at_zero() {
        let ext = (0.0, -1080.0, 1920.0, 1080.0);
        let r = room((100.0, -1080.0, 40.0, 50.0), ext, 1.0);
        assert_eq!(r, 1080.0 - 50.0 - 6.0 - 8.0);
    }
}
