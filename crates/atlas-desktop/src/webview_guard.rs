//! Never leave a blank window when the page cannot run (#272).
//!
//! On Omarchy (#265) every `WebKitWebProcess` aborted at launch and Atlas showed a dark, empty
//! window forever: the only error UI is JavaScript, and it died with the page. Two guards:
//!
//! - Linux hooks WebKitGTK's `web-process-terminated`: the first death reloads the page; a second
//!   within [`WINDOW`] stops reloading (it would loop) and shows the native error. Every death is
//!   a diagnostics incident (`webview-terminated`).
//! - Every OS: if the page has not called `app_ready` [`READY_TIMEOUT`] after it (re)loaded, the
//!   native error is shown too (`webview-not-ready` incident). That also covers a page that is
//!   alive but never boots (a failed chunk, a hung script).
//!
//! The native error says what happened and where the details are (`stderr.log`, `app.log`).
//! Linux: a GTK dialog with "Copy details" (the diagnostics report, redacted), "Reopen" and
//! "Quit". macOS and Windows: the plugin dialog (NSAlert / MessageBox) with the paths, "Reopen"
//! and "Quit"; the platform process-death signals and the clipboard button are #350.
//!
//! Debug builds can kill the web process on purpose (`atlas desktop debug kill-web-process`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Two deaths closer than this mean reloading will not help.
pub const WINDOW: Duration = Duration::from_secs(60);

/// A page that has not called `app_ready` this long after it (re)loaded is not coming. Far above a
/// slow cold start (WebKit spawn + page + three IPCs is well under 5 s on weak hardware), far
/// below "the user gives up and kills the app".
pub const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// What to do after the web process died at `now`, given the previous death.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Reload,
    GiveUp,
}

/// Pure decision: reload unless the previous death was less than [`WINDOW`] ago.
pub fn decide(last: Option<Instant>, now: Instant) -> Action {
    match last {
        Some(t) if now.saturating_duration_since(t) < WINDOW => Action::GiveUp,
        _ => Action::Reload,
    }
}

/// Pure decision: the page loaded at `loaded_at` and has not signalled ready for [`READY_TIMEOUT`].
pub fn stalled(ready: bool, loaded_at: Instant, now: Instant) -> bool {
    !ready && now.saturating_duration_since(loaded_at) >= READY_TIMEOUT
}

static READY: AtomicBool = AtomicBool::new(false);
/// Open dialog: the death hook and the ready watch must not stack a second one.
static DIALOG: AtomicBool = AtomicBool::new(false);
static LOADED_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// The page called `app_ready`.
pub fn mark_ready() {
    READY.store(true, Ordering::SeqCst);
}

/// The page is (re)loading: it has to signal ready again, and the timeout starts over.
fn note_load() {
    READY.store(false, Ordering::SeqCst);
    *LOADED_AT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
}

fn loaded_at() -> Instant {
    LOADED_AT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .unwrap_or_else(Instant::now)
}

/// Arm both guards on the main webview; call right after it is created.
pub fn install(app: &tauri::App) {
    note_load();
    #[cfg(target_os = "linux")]
    hook_termination(app);
    watch_ready(app.handle().clone());
}

/// Check once a second (an atomic load) whether the page has stalled; alarm once per stall.
fn watch_ready(app: tauri::AppHandle) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if DIALOG.load(Ordering::SeqCst)
                || !stalled(READY.load(Ordering::SeqCst), loaded_at(), Instant::now())
                || app.get_webview("main").is_none()
            {
                continue;
            }
            crate::diag::incident(
                "webview-not-ready",
                &format!(
                    "no app_ready {}s after the page loaded",
                    READY_TIMEOUT.as_secs()
                ),
            );
            let a = app.clone();
            let what = format!(
                "The page did not start within {} seconds",
                READY_TIMEOUT.as_secs()
            );
            let _ = app.run_on_main_thread(move || alarm(&a, &what));
        }
    });
}

/// Debug bridge: kill the main webview's web process, as a crash would.
#[cfg(debug_assertions)]
pub fn kill_web_process(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let view = app.get_webview("main").ok_or("main webview not found")?;
    #[cfg(target_os = "linux")]
    return view
        .with_webview(|pw| {
            use webkit2gtk::WebViewExt;
            pw.inner().terminate_web_process();
        })
        .map_err(|e| e.to_string());
    #[cfg(not(target_os = "linux"))]
    {
        let _ = view;
        Err("kill-web-process is Linux only (#350)".into())
    }
}

/// The text of the native error: what happened, and where the details are.
fn message(what: &str) -> String {
    let base = atlas_host::paths::base();
    format!(
        "{what}, so Atlas has nothing to show.\n\n\
         Details are in:\n  {}\n  {}\n\n\
         Include them when you report it: https://github.com/codeatlasdev/atlas/issues/new",
        atlas_host::paths::stderr_log(&base).display(),
        atlas_host::paths::app_log(&base).display(),
    )
}

#[cfg(target_os = "linux")]
fn alarm(app: &tauri::AppHandle, what: &str) {
    use tauri::Manager;
    let Some(view) = app.get_webview("main") else {
        return;
    };
    let what = what.to_string();
    let _ = view.with_webview(move |pw| give_up(&pw.inner(), &what));
}

#[cfg(not(target_os = "linux"))]
fn alarm(app: &tauri::AppHandle, what: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    if DIALOG.swap(true, Ordering::SeqCst) {
        return;
    }
    let a = app.clone();
    app.dialog()
        .message(message(what))
        .title("Atlas could not display its window")
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Reopen".into(),
            "Quit".into(),
        ))
        .show(move |reopen| {
            use tauri::Manager;
            DIALOG.store(false, Ordering::SeqCst);
            if !reopen {
                a.exit(1);
            } else if let Some(v) = a.get_webview("main") {
                note_load();
                let _ = v.reload();
            }
        });
}

/// Hook `web-process-terminated` on the main webview.
#[cfg(target_os = "linux")]
fn hook_termination(app: &tauri::App) {
    use tauri::Manager;
    let Some(view) = app.get_webview("main") else {
        crate::diag::log("webview guard: no main webview");
        return;
    };
    let _ = view.with_webview(|pw| {
        use webkit2gtk::WebViewExt;
        let wv: webkit2gtk::WebView = pw.inner();
        let last = std::rc::Rc::new(std::cell::Cell::new(None::<Instant>));
        wv.connect_web_process_terminated(move |wv, reason| {
            let why = match reason {
                webkit2gtk::WebProcessTerminationReason::Crashed => "crashed",
                webkit2gtk::WebProcessTerminationReason::ExceededMemoryLimit => "out of memory",
                webkit2gtk::WebProcessTerminationReason::TerminatedByApi => "terminated by the app",
                _ => "ended",
            };
            crate::diag::incident("webview-terminated", why);
            let now = Instant::now();
            match decide(last.replace(Some(now)), now) {
                Action::Reload => {
                    note_load();
                    wv.reload();
                }
                Action::GiveUp => {
                    give_up(wv, &format!("The page's web process {why} twice in a row"))
                }
            }
        });
    });
}

/// The native dialog: the page cannot show anything any more. GTK main thread only.
#[cfg(target_os = "linux")]
fn give_up(wv: &webkit2gtk::WebView, what: &str) {
    use gtk::prelude::*;
    use webkit2gtk::WebViewExt;
    const COPY: gtk::ResponseType = gtk::ResponseType::Other(1);
    if DIALOG.swap(true, Ordering::SeqCst) {
        return;
    }
    let parent = wv.toplevel().and_then(|w| w.downcast::<gtk::Window>().ok());
    let d = gtk::MessageDialog::new(
        parent.as_ref(),
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Error,
        gtk::ButtonsType::None,
        "Atlas could not display its window",
    );
    d.set_secondary_text(Some(&message(what)));
    d.add_button("Quit", gtk::ResponseType::Close);
    d.add_button("Copy details", COPY);
    d.add_button("Reopen", gtk::ResponseType::Accept);
    d.set_default_response(gtk::ResponseType::Accept);
    let answer = loop {
        let r = d.run();
        if r != COPY {
            break r;
        }
        copy_details();
    };
    d.close();
    DIALOG.store(false, Ordering::SeqCst);
    if answer == gtk::ResponseType::Accept {
        note_load();
        wv.reload();
    } else {
        std::process::exit(1);
    }
}

/// The diagnostics report the app already builds (logs tails, incidents, versions; redacted) as JSON.
#[cfg(target_os = "linux")]
fn copy_details() {
    let report = crate::diag::build(
        &atlas_host::paths::base(),
        atlas_host::bundle::Versions {
            app: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        },
    );
    let text = serde_json::to_string_pretty(&report).unwrap_or_else(|e| e.to_string());
    let clip = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clip.set_text(&text);
    // The dialog may end the process next: hand the text to the clipboard manager.
    clip.store();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #272: the first death reloads; a second soon after must not loop reloads behind a blank window.
    #[test]
    fn first_death_reloads_a_quick_second_gives_up_a_late_one_reloads() {
        let t0 = Instant::now();
        assert_eq!(decide(None, t0), Action::Reload);
        assert_eq!(
            decide(Some(t0), t0 + Duration::from_secs(5)),
            Action::GiveUp
        );
        assert_eq!(
            decide(Some(t0), t0 + WINDOW - Duration::from_millis(1)),
            Action::GiveUp
        );
        assert_eq!(decide(Some(t0), t0 + WINDOW), Action::Reload);
    }

    /// #272: a page that never calls `app_ready` is a blank window; a ready one never alarms, however old.
    #[test]
    fn a_page_that_never_signals_ready_stalls_only_after_the_timeout() {
        let t0 = Instant::now();
        assert!(!stalled(
            false,
            t0,
            t0 + READY_TIMEOUT - Duration::from_millis(1)
        ));
        assert!(stalled(false, t0, t0 + READY_TIMEOUT));
        assert!(!stalled(true, t0, t0 + READY_TIMEOUT * 10));
    }

    /// The text is what a user reads when nothing else works: both log paths must be in it.
    #[test]
    fn the_message_names_both_logs() {
        let m = message("The page did not start");
        assert!(m.contains("stderr.log") && m.contains("app.log"), "{m}");
        assert!(m.starts_with("The page did not start,"));
    }
}
