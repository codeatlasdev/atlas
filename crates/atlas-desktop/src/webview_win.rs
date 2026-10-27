//! Windows: WebView2 `MemoryUsageTargetLevel`. A hidden (tray) or minimized main window has no frames to
//! paint, so ask the browser process to trim its working set (Low); back to Normal when it shows again.
//! `TrySuspend` is deliberately not used: it freezes timers and the page's sockets, which carry the hostd
//! streams and the terminals.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    ICoreWebView2_19,
};
use windows_core::Interface;

/// Browser arguments of every Atlas webview. WebView2 allows one set per user-data folder, so the main window,
/// the tray popover and the embedded browser's tabs all pass exactly this (`.additional_browser_args(ARGS)`).
/// First three = wry's defaults (it replaces them when args are given); the rest stops Edge's background
/// services (component updater, sync, telemetry pings, crash uploader, default-app seeding): measured -110 MB
/// working set and two fewer processes on an idle page. They affect no page behavior.
pub const ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
    --disable-background-networking --disable-component-update --disable-sync --no-pings \
    --disable-breakpad --disable-default-apps --metrics-recording-only";

static LOW: AtomicBool = AtomicBool::new(false);

/// Apply Low when the main window is hidden or minimized, Normal otherwise. Cheap and idempotent: only a
/// change of state reaches WebView2.
pub fn sync(app: &AppHandle) {
    let Some(win) = app.get_window("main") else {
        return;
    };
    let low = !win.is_visible().unwrap_or(true) || win.is_minimized().unwrap_or(false);
    if LOW.load(Ordering::Relaxed) == low {
        return;
    }
    let Some(wv) = app.get_webview("main") else {
        return;
    };
    let level = if low {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    };
    let _ = wv.with_webview(move |p| unsafe {
        let set = p
            .controller()
            .CoreWebView2()
            .and_then(|core| core.cast::<ICoreWebView2_19>())
            .and_then(|core| core.SetMemoryUsageTargetLevel(level));
        // Only a success records the state, so a call that raced the webview's creation is retried.
        if set.is_ok() {
            LOW.store(low, Ordering::Relaxed);
        }
    });
}
