//! Native views (the GPU terminal, browser tabs, any future kind) under the page.
//!
//! A native view is an `NSView` BELOW the main webview. The page is transparent exactly over each view's slot (the
//! page's own ground layers carry a hole there, `apps/desktop/src/lib/native/ground.ts`), so the view shows through
//! it, and everything the page draws over a slot (menus, dialogs, a modal's scrim, shadows, blur) is ordinary DOM
//! painted above the view in the same compositor commit: there is nothing to keep in step. Only the INPUT needs
//! help: the page is on top, so the router (`router.rs`, carried out by `mac.rs`) hands mouse events over an
//! interactive view's slot to that view, except inside the rects the page published with `natives_hit`.
//!
//! This module is the one place a kind is known to Rust, by `(kind, tab)` in the registry (`register` /
//! `unregister`): it knows no kind by name. Stacking, the underlay and the monitor are `mac` (macOS only; the other
//! platforms have no native views of their own, so every command here is a no-op there).

// The registry and the router are pure, so their tests run anywhere; only macOS has native views to hold or route.
#[cfg(any(test, target_os = "macos"))]
mod registry;
#[cfg(any(test, target_os = "macos"))]
mod router;

#[cfg(target_os = "macos")]
mod mac;

// The command's argument type exists on every platform (the handler list is shared); only macOS reads it.
#[cfg(not(any(test, target_os = "macos")))]
mod registry {
    #[derive(serde::Deserialize)]
    pub struct HitMap {}
}

pub use registry::HitMap;
#[cfg(target_os = "macos")]
pub use registry::Registry;

#[cfg(target_os = "macos")]
pub(crate) static REGISTRY: std::sync::LazyLock<parking_lot::Mutex<Registry>> =
    std::sync::LazyLock::new(Default::default);

#[cfg(target_os = "macos")]
pub(crate) fn with_registry<T>(f: impl FnOnce(&Registry) -> T) -> T {
    f(&REGISTRY.lock())
}

/// Register a native view (`kind`: `"terminal"`, `"browser"`, any new kind) under its tab id. Call when the view is
/// created, and again when it is replaced by another `NSView`. `interactive`: the view takes mouse input itself, so
/// the router hands it the events over its slot (a browser page); a terminal's input is DOM events on its slot.
/// Any thread.
#[cfg(target_os = "macos")]
pub fn register(kind: &str, tab: &str, view: usize, interactive: bool) {
    REGISTRY.lock().register(kind, tab, view, interactive);
}

/// Unregister a destroyed view (its pointer may be reused by the next one). Any thread.
#[cfg(target_os = "macos")]
pub fn unregister(kind: &str, tab: &str) {
    REGISTRY.lock().unregister(kind, tab);
}

/// Put a native view that just joined the window directly below the page. Main thread.
#[cfg(target_os = "macos")]
pub fn lower_below_page(view: usize) {
    mac::lower_below_page(view);
}

/// Setup (macOS, main thread): record the main webview, add the underlay, install the input monitor. Before any
/// native view exists.
#[cfg(target_os = "macos")]
pub fn install(app: &tauri::AppHandle) {
    mac::install(app);
}

/// The rects the page paints over native views and owns input in (`lib/native/hit.ts`): open menus, dialogs, the
/// edge peek, a modal's whole-window scrim. Sent only when the set changes, never per frame; the router reads it on
/// every mouse event. Window points, top-left origin.
#[tauri::command]
pub fn natives_hit(hit: HitMap) {
    #[cfg(target_os = "macos")]
    REGISTRY.lock().set_hit_map(hit);
    #[cfg(not(target_os = "macos"))]
    let _ = hit;
}

/// The colour under every native view: `[r, g, b, a]`, 0..1. It is what a slot hole shows for the frame before a
/// native view has landed (a view that was just shown, resized or moved by the page), so it never shows the window
/// material. The page sends its surface colour whenever the theme changes.
#[tauri::command]
pub async fn natives_ground(app: tauri::AppHandle, rgba: [f64; 4]) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        app.run_on_main_thread(move || mac::set_ground(rgba))
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, rgba);
        Ok(())
    }
}
