//! Windows: WebView2 `MemoryUsageTargetLevel`. A hidden (tray) or minimized main window has no frames to
//! paint, so ask the browser process to trim its working set (Low); back to Normal when it shows again.
//! `TrySuspend` is deliberately not used: it freezes timers and the page's sockets, which carry the hostd
//! streams and the terminals.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    ICoreWebView2_19, ICoreWebView2Settings3,
};
use windows_core::Interface;

/// Turn WebView2's browser accelerator keys off for an app page (the main window and the tray popover):
/// F5 / Ctrl+R reload, Ctrl+F / F3 find, Ctrl+P print, Ctrl+Shift+C / F12 devtools, Ctrl+± zoom, Ctrl+U … are
/// Edge's, and an unbound one would reload the app or open Edge UI. `keydown` still reaches the page (WebView2
/// guarantees it), so Atlas's own keymap is unaffected. Never called for the embedded browser's tabs: there users
/// expect Ctrl+F and F5 to work like a browser.
pub fn no_accelerators(webview: &tauri::WebviewWindow) {
    let _ = webview.with_webview(|p| unsafe {
        let _ = p
            .controller()
            .CoreWebView2()
            .and_then(|core| core.Settings())
            .and_then(|s| s.cast::<ICoreWebView2Settings3>())
            .and_then(|s| s.SetAreBrowserAcceleratorKeysEnabled(false));
    });
}

/// Browser arguments of every Atlas webview. WebView2 allows one set per user-data folder, so the main window,
/// the tray popover and the embedded browser's tabs all pass exactly this (`.additional_browser_args(&args())`).
/// First three = wry's defaults (it replaces them when args are given); the rest stops Edge's background
/// services (component updater, sync, crash uploader, default-app seeding). In a controlled run (Edge engine,
/// throwaway profile, idle page) that was about -110 MB working set and two fewer processes; not measured
/// cleanly on Atlas itself. Pages keep their behavior (`--no-pings` was left out: it changes `<a ping>`).
const ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
    --disable-background-networking --disable-component-update --disable-sync \
    --disable-breakpad --disable-default-apps --metrics-recording-only";

/// [`ARGS`], plus `--remote-debugging-port=<port>` when `ATLAS_WEBVIEW_DEVTOOLS_PORT` is set. Release builds have
/// no devtools and Tauri's args replace `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`, so this is the only way to read
/// the console of an installed app (`chrome://inspect` → localhost:<port>). Loopback only; read once per process
/// so every webview gets the same set.
pub fn args() -> &'static str {
    static ALL: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        with_devtools(std::env::var("ATLAS_WEBVIEW_DEVTOOLS_PORT").ok().as_deref())
    });
    &ALL
}

fn with_devtools(port: Option<&str>) -> String {
    match port
        .and_then(|p| p.trim().parse::<u16>().ok())
        .filter(|p| *p > 0)
    {
        Some(p) => {
            format!("{ARGS} --remote-debugging-port={p} --remote-debugging-address=127.0.0.1")
        }
        None => ARGS.to_string(),
    }
}

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

#[cfg(test)]
mod tests {
    /// Incident (1.5.0-alpha.24): the Windows app died at launch with `0x8007139F` because a window added
    /// later (the agent dock) built its webview without [`super::ARGS`]. WebView2 refuses a second set of
    /// options in one process, so one forgotten builder takes the whole app down, and only on Windows.
    /// Every place that builds a webview must pass the shared args right after creating the builder.
    #[test]
    fn every_webview_builder_passes_the_shared_browser_args() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&p).unwrap();
                // Production code only: a `#[cfg(test)]` module builds mock webviews.
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                let lines: Vec<&str> = code.lines().collect();
                for (i, l) in lines.iter().enumerate() {
                    let builds = [
                        "WebviewWindowBuilder::new(",
                        "WebviewWindowBuilder::from_config(",
                        "WebviewBuilder::new(",
                    ]
                    .iter()
                    .any(|b| l.contains(b));
                    if !builds {
                        continue;
                    }
                    let next = lines[i..lines.len().min(i + 15)].join("\n");
                    if !next.contains("webview_win::args()") {
                        missing.push(format!(
                            "{}:{}",
                            p.strip_prefix(&src).unwrap().display(),
                            i + 1
                        ));
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "webview built without webview_win::args(): {missing:?}"
        );
    }

    /// Incident (1.5.0-alpha.24, Windows): `app_ready` was a sync command, so it ran on the main thread and its
    /// `run_on_main_thread` ran inline, building the tray popover's WebView2 inside the IPC handler. That never
    /// returns on Windows, and every later `invoke`/chunk fetch queued behind it (dead buttons, no Settings).
    /// A sync `#[tauri::command]` must never reach webview creation; those commands are `async`.
    #[test]
    fn sync_commands_never_create_webviews() {
        const CREATORS: [&str; 6] = [
            "show_now(",
            "ensure_popover(",
            "WebviewWindowBuilder::",
            "WebviewBuilder::new(",
            "dock::start(",
            "surface::open(",
        ];
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut bad = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                // CRLF checkouts (Windows CI) would hide the `\n}\n` item boundary below.
                let text = std::fs::read_to_string(&p).unwrap().replace("\r\n", "\n");
                let code = text.split("#[cfg(test)]").next().unwrap_or_default();
                for chunk in code.split("#[tauri::command]").skip(1) {
                    let head = chunk.trim_start();
                    let sync = head.starts_with("pub fn ") || head.starts_with("fn ");
                    // The body: up to the next item at column 0.
                    let body = chunk.split("\n}\n").next().unwrap_or(chunk);
                    if sync && let Some(c) = CREATORS.iter().find(|c| body.contains(*c)) {
                        let name = head.split('(').next().unwrap_or(head);
                        bad.push(format!(
                            "{}: {name} calls {c}",
                            p.strip_prefix(&src).unwrap().display()
                        ));
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "sync commands that create webviews (make them async): {bad:?}"
        );
    }

    /// A bad value must never break launch: only a valid non-zero port adds the flag, and it stays loopback.
    #[test]
    fn devtools_port_is_opt_in_and_validated() {
        assert_eq!(super::with_devtools(None), super::ARGS);
        for bad in ["", "abc", "0", "70000", "-1"] {
            assert_eq!(super::with_devtools(Some(bad)), super::ARGS, "{bad}");
        }
        let on = super::with_devtools(Some(" 9222 "));
        assert!(on.starts_with(super::ARGS));
        assert!(on.contains("--remote-debugging-port=9222"));
        assert!(on.contains("--remote-debugging-address=127.0.0.1"));
    }
}
