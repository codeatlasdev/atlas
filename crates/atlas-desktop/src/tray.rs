//! Menu-bar popover. Left click on the tray icon toggles a borderless, translucent,
//! always-on-top window (`tray`, route `/tray`) under the icon; right click keeps a small
//! native menu. The window is built once, hidden, and only moved/shown afterwards.
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

/// Icon rect from a tray event, as physical (x, y, w, h).
fn rect_px(rect: &Rect, scale: f64) -> (f64, f64, f64, f64) {
    let p = rect.position.to_physical::<f64>(scale);
    let s = rect.size.to_physical::<f64>(scale);
    (p.x, p.y, s.width, s.height)
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("tray".into()))
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

/// Right-click menu: just the essentials, everything else lives in the popover.
fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "open",
        "Open Atlas",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&PredefinedMenuItem::quit(app, Some("Quit Atlas"))?)?;
    Ok(menu)
}

/// Create the popover window and the tray icon.
pub fn start(app: &AppHandle) -> tauri::Result<()> {
    app.manage(TrayState::default());
    build(app)?;
    TrayIconBuilder::with_id("main")
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/tray.png"
        ))?)
        .icon_as_template(true)
        .menu(&menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| {
            if ev.id().as_ref() == "open" {
                crate::show_main(app);
            }
        })
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

/// Popover: fit the window to its content (logical px).
#[tauri::command]
pub fn tray_resize(app: AppHandle, height: f64) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_size(LogicalSize::new(WIDTH, height));
    }
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
    use super::place;

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
}
