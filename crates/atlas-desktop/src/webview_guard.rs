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

use crate::native_ui;
use atlas_i18n::{t, t_in};
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
            let what = t!(
                "native_notify_alert_not_ready",
                secs = READY_TIMEOUT.as_secs()
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

/// The text of the native error: `what` (a sentence of the language, what happened) and where the details are.
fn message(tag: &str, what: &str) -> String {
    let base = atlas_host::paths::base();
    let details = t_in!(
        tag,
        "native_notify_alert_details",
        stderr = atlas_host::paths::stderr_log(&base).display().to_string(),
        app = atlas_host::paths::app_log(&base).display().to_string(),
        url = "https://github.com/codeatlasdev/atlas/issues/new",
    );
    format!("{what}\n\n{details}")
}

/// The alert of the watchdog for `what`, in the display language.
fn alert_for(what: &str) -> native_ui::Alert {
    native_ui::Alert {
        title: t!("native_notify_alert_title"),
        message: message(atlas_i18n::lang(), what),
        ok: t!("native_notify_alert_reopen"),
        cancel: t!("native_notify_alert_quit"),
    }
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

/// An agent instance shows nothing (`native_ui`): the incident is recorded, the app keeps running so the agent can
/// inspect it, and `DIALOG` stays set so that happens once per stall.
#[cfg(not(target_os = "linux"))]
fn alarm(app: &tauri::AppHandle, what: &str) {
    if DIALOG.swap(true, Ordering::SeqCst) {
        return;
    }
    let a = app.clone();
    native_ui::alert(app, alert_for(what), move |reopen| {
        use tauri::Manager;
        DIALOG.store(false, Ordering::SeqCst);
        if !reopen {
            crate::app_exit::exit(&a, 1);
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
            // The log line stays English; the dialog's sentence is the catalog message of the same reason.
            let (why, sentence) = match reason {
                webkit2gtk::WebProcessTerminationReason::Crashed => {
                    ("crashed", t!("native_notify_alert_crashed"))
                }
                webkit2gtk::WebProcessTerminationReason::ExceededMemoryLimit => {
                    ("out of memory", t!("native_notify_alert_out_of_memory"))
                }
                webkit2gtk::WebProcessTerminationReason::TerminatedByApi => (
                    "terminated by the app",
                    t!("native_notify_alert_terminated"),
                ),
                _ => ("ended", t!("native_notify_alert_ended")),
            };
            crate::diag::incident("webview-terminated", why);
            let now = Instant::now();
            match decide(last.replace(Some(now)), now) {
                Action::Reload => {
                    note_load();
                    wv.reload();
                }
                Action::GiveUp => give_up(wv, &sentence),
            }
        });
    });
}

/// The native dialog: the page cannot show anything any more. GTK main thread only. An agent instance shows nothing
/// (`native_ui`): the incident is recorded, the app keeps running, and `DIALOG` stays set so that happens once.
#[cfg(target_os = "linux")]
fn give_up(wv: &webkit2gtk::WebView, what: &str) {
    use gtk::prelude::*;
    use webkit2gtk::WebViewExt;
    if DIALOG.swap(true, Ordering::SeqCst) {
        return;
    }
    let parent = wv.toplevel().and_then(|w| w.downcast::<gtk::Window>().ok());
    let answer = native_ui::gtk_alert(
        parent.as_ref(),
        &alert_for(what),
        &t!("native_notify_alert_copy_details"),
        copy_details,
    );
    if let Some(reopen) = answer {
        DIALOG.store(false, Ordering::SeqCst);
        if reopen {
            note_load();
            wv.reload();
        } else {
            #[allow(
                clippy::disallowed_methods,
                reason = "Linux (GTK) only: no agent display exists there to destroy"
            )]
            std::process::exit(1);
        }
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
        for tag in ["en", "pt-BR"] {
            let m = message(tag, "What happened.");
            assert!(m.contains("stderr.log") && m.contains("app.log"), "{m}");
            assert!(m.starts_with("What happened.\n\n"), "{m}");
            assert!(m.contains("https://github.com/codeatlasdev/atlas/issues/new"));
        }
    }

    // The dialog is native text the person reads when nothing else works: each language has its own sentences, and the
    // wait is a number inside them, not a composed fragment.
    #[test]
    fn the_dialog_text_is_in_the_language_and_carries_the_wait() {
        let not_ready = |tag| t_in!(tag, "native_notify_alert_not_ready", secs = 30);
        assert_eq!(
            not_ready("en"),
            "The page did not start within 30 seconds, so Atlas has nothing to show."
        );
        assert_eq!(
            not_ready("pt-BR"),
            "A página não iniciou em 30 segundos, então o Atlas não tem nada para mostrar."
        );
        let quit = |tag, os| atlas_i18n::lookup_on(tag, "native_notify_alert_quit", None, os);
        assert_eq!(quit("en", "windows").as_deref(), Some("Exit"));
        assert_eq!(quit("pt-BR", "mac").as_deref(), Some("Sair"));
    }

    /// Incident 2026-11-18: an agent instance's "page did not start" alert covered the maintainer's screen. The
    /// watchdog's alert, in an agent instance, is recorded with its title and text and never reaches the dialog.
    #[test]
    fn the_watchdog_alert_in_an_agent_instance_is_recorded_and_never_shown() {
        use crate::agent_mode::Mode;
        use crate::native_ui::{Verdict, alert_with};
        use std::cell::{Cell, RefCell};
        let what = t!(
            "native_notify_alert_not_ready",
            secs = READY_TIMEOUT.as_secs()
        );
        let (shown, lines) = (Cell::new(false), RefCell::new(Vec::new()));
        let v = alert_with(
            Mode::Agent,
            alert_for(&what),
            |l| lines.borrow_mut().push(l),
            |_| shown.set(true),
        );
        assert_eq!(v, Verdict::Suppressed);
        assert!(!shown.get(), "no dialog in an agent instance");
        let lines = lines.into_inner();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains(&t!("native_notify_alert_title")),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains(&what), "{}", lines[0]);
    }

    /// The user's app still gets the dialog, with the catalog's buttons: Reopen answers yes, Quit no.
    #[test]
    fn the_watchdog_alert_in_the_users_app_reaches_the_dialog_with_both_buttons() {
        use crate::agent_mode::Mode;
        use crate::native_ui::{Verdict, alert_with};
        let got = std::cell::RefCell::new(None);
        let v = alert_with(
            Mode::User,
            alert_for("What happened."),
            |_| panic!("nothing to record"),
            |a| *got.borrow_mut() = Some(a),
        );
        assert_eq!(v, Verdict::Allowed);
        let a = got.into_inner().expect("the dialog sink ran");
        assert_eq!(a.title, t!("native_notify_alert_title"));
        assert!(a.message.starts_with("What happened.\n\n"), "{}", a.message);
        assert_eq!(
            (a.ok, a.cancel),
            (
                t!("native_notify_alert_reopen"),
                t!("native_notify_alert_quit")
            )
        );
    }
}
