//! Layout zoom of the app's own webview (⌘+ ⌘− ⌘0, `apps/desktop/src/lib/zoom.ts`). `Webview::set_zoom` is the
//! engine's page zoom (WKWebView `pageZoom`, WebKitGTK `zoom-level`): the whole layout scales and CSS px stay
//! CSS px, so the frontend converts the rects it hands to native views (terminal, browsers, stage, captures)
//! to window points itself. Child webviews (embedded browsers) are separate pages and keep their own zoom.

use tauri::{AppHandle, Manager};

/// Same bounds as the frontend ladder (`ZOOM_LEVELS`); the settings file can be hand-edited.
const MIN: f64 = 0.5;
const MAX: f64 = 3.0;

/// A zoom Rust is willing to apply: finite values clamp into the ladder's range, anything else is 100 %.
pub fn sane(factor: f64) -> f64 {
    if factor.is_finite() {
        factor.clamp(MIN, MAX)
    } else {
        1.0
    }
}

/// Scale the main webview (not a child: the window's own page).
#[tauri::command]
pub fn zoom_set(app: AppHandle, factor: f64) -> Result<(), String> {
    app.get_webview("main")
        .ok_or("no main webview")?
        .set_zoom(sane(factor))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_is_clamped_to_the_ladder_and_junk_is_100_percent() {
        assert_eq!(sane(1.0), 1.0);
        assert_eq!(sane(1.25), 1.25);
        assert_eq!(sane(0.0), MIN);
        assert_eq!(sane(-4.0), MIN);
        assert_eq!(sane(40.0), MAX);
        assert_eq!(sane(f64::NAN), 1.0);
        assert_eq!(sane(f64::INFINITY), 1.0);
    }
}
