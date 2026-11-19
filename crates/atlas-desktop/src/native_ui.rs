//! Native UI that lives outside the app's window, behind ONE gate.
//!
//! An agent instance (`ATLAS_AGENT=1`, `agent_mode.rs`) keeps its window on a private virtual display, but app-level
//! native UI is placed by the OS on the user's active display: a message box, a file picker, a notification banner, a
//! context menu popped at the cursor, the browser or Finder opened by a link. On 2026-11-18 the watchdog's
//! "Atlas could not display its window" alert covered the maintainer's screen while an agent was proving a cold-start
//! hang. So nothing in this crate raises such UI directly: the raw APIs (`tauri_plugin_dialog`, `tauri_plugin_notification`,
//! `ContextMenu::popup`, the GTK dialog) are disallowed by `clippy.toml` everywhere except here, and every function
//! below asks [`gate`] first.
//!
//! In an agent instance [`gate`] shows nothing. It records what would have been shown (kind, title and text) as a
//! diagnostics incident and as an error line in `atlas desktop debug logs --level error`, and the caller takes the
//! safe default: an alert is not answered (the app keeps running so the agent can look at it), a picker returns
//! nothing, a notification and a menu do not appear, a launcher reports that it did not run.
//!
//! The pages cannot go around it either: the capability grants no dialog, shell or notification command (the one
//! exception, `notification:allow-is-permission-granted`, is what the plugin's own `window.Notification` shim asks
//! at load), the open and save panels are the `pick_path` and `save_path` commands below, and `<a target=_blank>`
//! opens through `open_url` (`apps/desktop/src/lib/nativeUi.ts`).
//!
//! This module is the only one allowed to call those APIs (`#![allow(clippy::disallowed_methods)]`).
#![allow(clippy::disallowed_methods)]

use crate::agent_mode::Mode;
use serde::Deserialize;
use tauri::{AppHandle, Runtime};

/// The diagnostics incident kind of a suppressed native UI.
pub const INCIDENT: &str = "native-ui-suppressed";

/// What kind of native UI was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A message box (the watchdog's error).
    Alert,
    /// An open or save panel.
    Picker,
    /// A system notification (banner, sound).
    Notification,
    /// A native menu popped at the cursor.
    Menu,
    /// Another app started for the user: the browser, the file manager, an editor, System Settings.
    Launch,
    /// A window that would sit on a real display: the capture overlay, the tray popover. An agent instance's windows
    /// go through `agent_display::park_and_show` only.
    Window,
}

impl Kind {
    const fn name(self) -> &'static str {
        match self {
            Kind::Alert => "alert",
            Kind::Picker => "file picker",
            Kind::Notification => "notification",
            Kind::Menu => "menu",
            Kind::Launch => "launch",
            Kind::Window => "window",
        }
    }
}

/// What the gate says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    Suppressed,
}

/// The decision, with the recording injected: in `mode`, may this native UI show? When not, `record` gets the line
/// that says what was held back.
pub fn judge(
    mode: Mode,
    kind: Kind,
    title: &str,
    text: &str,
    record: impl FnOnce(String),
) -> Verdict {
    if mode.native_ui_allowed() {
        return Verdict::Allowed;
    }
    let what: Vec<&str> = [title, text]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    record(format!(
        "native {} suppressed in an agent instance (it would have appeared on the user's screen): {}",
        kind.name(),
        what.join(": ")
    ));
    Verdict::Suppressed
}

/// [`judge`] for this process, recording into the diagnostics and the debug log.
pub fn gate(kind: Kind, title: &str, text: &str) -> Verdict {
    judge(Mode::current(), kind, title, text, record)
}

fn record(line: String) {
    crate::diag::incident(INCIDENT, &line);
    #[cfg(debug_assertions)]
    crate::debug::note("error", &crate::diag::redact(&line));
}

// ───────────────────────────── alerts ─────────────────────────────

/// A two-button error message: `ok` is the answer that goes on (Reopen), `cancel` the one that gives up (Quit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub title: String,
    pub message: String,
    pub ok: String,
    pub cancel: String,
}

/// [`alert`] with the mode, the recording and the dialog injected: `show` runs only when the gate allows it.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn alert_with(
    mode: Mode,
    a: Alert,
    record: impl FnOnce(String),
    show: impl FnOnce(Alert),
) -> Verdict {
    let verdict = judge(mode, Kind::Alert, &a.title, &a.message, record);
    if verdict == Verdict::Allowed {
        show(a);
    }
    verdict
}

/// Show `a` and call `then(true)` when `ok` was pressed. Suppressed: `then` never runs, nobody answered.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub fn alert<R: Runtime>(
    app: &AppHandle<R>,
    a: Alert,
    then: impl FnOnce(bool) + Send + 'static,
) -> Verdict {
    use tauri_plugin_dialog::{MessageDialogButtons, MessageDialogKind};
    alert_with(Mode::current(), a, record, |a| {
        let dialog = tauri_plugin_dialog::DialogExt::dialog(app);
        dialog
            .message(a.message)
            .title(a.title)
            .kind(MessageDialogKind::Error)
            .buttons(MessageDialogButtons::OkCancelCustom(a.ok, a.cancel))
            .show(then);
    })
}

/// The GTK message box, with a third "copy details" button that keeps it open. `Some(true)` = `a.ok`, `Some(false)` =
/// `a.cancel`; `None` = suppressed. GTK main thread only.
#[cfg(target_os = "linux")]
pub fn gtk_alert(
    parent: Option<&gtk::Window>,
    a: &Alert,
    copy_label: &str,
    on_copy: impl Fn(),
) -> Option<bool> {
    use gtk::prelude::*;
    const COPY: gtk::ResponseType = gtk::ResponseType::Other(1);
    if gate(Kind::Alert, &a.title, &a.message) == Verdict::Suppressed {
        return None;
    }
    let d = gtk::MessageDialog::new(
        parent,
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Error,
        gtk::ButtonsType::None,
        &a.title,
    );
    d.set_secondary_text(Some(&a.message));
    d.add_button(&a.cancel, gtk::ResponseType::Close);
    d.add_button(copy_label, COPY);
    d.add_button(&a.ok, gtk::ResponseType::Accept);
    d.set_default_response(gtk::ResponseType::Accept);
    let answer = loop {
        let r = d.run();
        if r != COPY {
            break r;
        }
        on_copy();
    };
    d.close();
    Some(answer == gtk::ResponseType::Accept)
}

// ───────────────────────────── file pickers ─────────────────────────────

/// One file-type filter of a picker.
#[derive(Debug, Clone, Deserialize)]
pub struct Filter {
    pub name: String,
    pub extensions: Vec<String>,
}

/// What the pages ask a picker for (the subset of `@tauri-apps/plugin-dialog`'s options the app uses).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathDialog {
    pub title: Option<String>,
    /// A folder, not a file (ignored when saving).
    #[serde(default)]
    pub directory: bool,
    /// Where to start; a file path also names the file to save as.
    pub default_path: Option<String>,
    #[serde(default)]
    pub filters: Vec<Filter>,
}

#[derive(Clone, Copy)]
enum Pick {
    File,
    Folder,
    Save,
}

/// The picker of `o` in `how`; `None` = cancelled, or suppressed by the gate.
async fn pick<R: Runtime>(
    app: &AppHandle<R>,
    o: PathDialog,
    how: Pick,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::FilePath;
    let title = o.title.clone().unwrap_or_default();
    let start = o.default_path.clone().unwrap_or_default();
    if gate(Kind::Picker, &title, &start) == Verdict::Suppressed {
        return Ok(None);
    }
    let mut b = parented(app, tauri_plugin_dialog::DialogExt::dialog(app).file());
    if let Some(t) = o.title {
        b = b.set_title(t);
    }
    for f in &o.filters {
        let ext: Vec<&str> = f.extensions.iter().map(String::as_str).collect();
        b = b.add_filter(&f.name, &ext);
    }
    if let Some(p) = o.default_path {
        b = with_default_path(b, std::path::PathBuf::from(p));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    let done = move |p: Option<FilePath>| {
        let _ = tx.send(p.map(|p| p.to_string()));
    };
    match how {
        Pick::File => b.pick_file(done),
        Pick::Folder => b.pick_folder(done),
        Pick::Save => b.save_file(done),
    }
    rx.await.map_err(|e| e.to_string())
}

/// The dialogs the plugin placed as a sheet of the main window, so do these.
#[cfg(any(windows, target_os = "macos"))]
fn parented<R: Runtime>(
    app: &AppHandle<R>,
    b: tauri_plugin_dialog::FileDialogBuilder<R>,
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    use tauri::Manager;
    match app.get_window("main") {
        Some(w) => b.set_parent(&w),
        None => b,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn parented<R: Runtime>(
    _: &AppHandle<R>,
    b: tauri_plugin_dialog::FileDialogBuilder<R>,
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    b
}

/// A start path the way the plugin read it: an existing folder is the folder shown; anything else is a file whose
/// parent is shown and whose name is pre-filled.
fn with_default_path<R: Runtime>(
    b: tauri_plugin_dialog::FileDialogBuilder<R>,
    path: std::path::PathBuf,
) -> tauri_plugin_dialog::FileDialogBuilder<R> {
    let path: std::path::PathBuf = path.components().collect();
    if path.is_dir() {
        return b.set_directory(path);
    }
    let mut b = b;
    if let Some(parent) = path.parent().filter(|p| p.components().count() > 0) {
        b = b.set_directory(parent);
    }
    match path.file_name() {
        Some(name) => b.set_file_name(name.to_string_lossy()),
        None => b,
    }
}

/// An open panel for one file or folder: the path, or `None` when cancelled.
#[tauri::command]
pub async fn pick_path(app: AppHandle, options: PathDialog) -> Result<Option<String>, String> {
    let how = if options.directory {
        Pick::Folder
    } else {
        Pick::File
    };
    pick(&app, options, how).await
}

/// A save panel: the path to write, or `None` when cancelled.
#[tauri::command]
pub async fn save_path(app: AppHandle, options: PathDialog) -> Result<Option<String>, String> {
    pick(&app, options, Pick::Save).await
}

// ───────────────────────────── notifications, menus, launchers ─────────────────────────────

/// A system notification. `Suppressed`: nothing was shown.
pub fn notify<R: Runtime>(
    app: &AppHandle<R>,
    title: impl Into<String>,
    body: impl Into<String>,
) -> Result<Verdict, String> {
    let (title, body) = (title.into(), body.into());
    if gate(Kind::Notification, &title, &body) == Verdict::Suppressed {
        return Ok(Verdict::Suppressed);
    }
    let n = tauri_plugin_notification::NotificationExt::notification(app);
    n.builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())?;
    Ok(Verdict::Allowed)
}

/// Pop `menu` at the cursor over `win`. `what` names the menu in the record. `Suppressed`: nothing popped.
pub fn popup<R: Runtime>(
    menu: &tauri::menu::Menu<R>,
    win: tauri::Window<R>,
    what: &str,
) -> Result<Verdict, String> {
    use tauri::menu::ContextMenu;
    if gate(Kind::Menu, what, "") == Verdict::Suppressed {
        return Ok(Verdict::Suppressed);
    }
    menu.popup(win).map_err(|e| e.to_string())?;
    Ok(Verdict::Allowed)
}

/// [`launch`] with the mode and the recording injected.
pub fn launch_with(
    mode: Mode,
    what: &str,
    target: &str,
    record: impl FnOnce(String),
) -> Result<(), String> {
    match judge(mode, Kind::Launch, what, target, record) {
        Verdict::Allowed => Ok(()),
        Verdict::Suppressed => Err(format!(
            "{what}: not started, an agent instance never opens other apps on the user's screen ({target})"
        )),
    }
}

/// Ask before starting another app for the user (browser, file manager, editor, System Settings): `Err` when this is
/// an agent instance, with what the agent needs to know.
pub fn launch(what: &str, target: &str) -> Result<(), String> {
    launch_with(Mode::current(), what, target, record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn recorded(mode: Mode, kind: Kind, title: &str, text: &str) -> (Verdict, Vec<String>) {
        let lines = RefCell::new(Vec::new());
        let v = judge(mode, kind, title, text, |l| lines.borrow_mut().push(l));
        (v, lines.into_inner())
    }

    fn alert_of(title: &str, message: &str) -> Alert {
        Alert {
            title: title.into(),
            message: message.into(),
            ok: "Reopen".into(),
            cancel: "Quit".into(),
        }
    }

    #[test]
    fn the_users_app_shows_native_ui_and_records_nothing() {
        for kind in [
            Kind::Alert,
            Kind::Picker,
            Kind::Notification,
            Kind::Menu,
            Kind::Launch,
            Kind::Window,
        ] {
            assert_eq!(
                recorded(Mode::User, kind, "t", "x"),
                (Verdict::Allowed, vec![])
            );
        }
    }

    #[test]
    fn an_agent_instance_shows_no_native_ui_and_records_what_it_held_back() {
        for kind in [
            Kind::Alert,
            Kind::Picker,
            Kind::Notification,
            Kind::Menu,
            Kind::Launch,
            Kind::Window,
        ] {
            let (v, lines) = recorded(Mode::Agent, kind, "Page failed", "no app_ready after 30 s");
            assert_eq!(v, Verdict::Suppressed, "{kind:?}");
            assert_eq!(lines.len(), 1, "{kind:?}");
            let line = &lines[0];
            assert!(line.contains(kind.name()), "{line}");
            assert!(
                line.contains("Page failed"),
                "the title is in the record: {line}"
            );
            assert!(
                line.contains("no app_ready after 30 s"),
                "the text is in the record: {line}"
            );
        }
    }

    #[test]
    fn an_alert_in_an_agent_instance_is_recorded_and_never_shown() {
        let shown = Cell::new(false);
        let lines = RefCell::new(Vec::new());
        let v = alert_with(
            Mode::Agent,
            alert_of(
                "Atlas could not display its window",
                "The page did not start",
            ),
            |l| lines.borrow_mut().push(l),
            |_| shown.set(true),
        );
        assert_eq!(v, Verdict::Suppressed);
        assert!(
            !shown.get(),
            "the dialog sink must not run in an agent instance"
        );
        assert_eq!(lines.borrow().len(), 1);
        assert!(lines.borrow()[0].contains("Atlas could not display its window"));
    }

    #[test]
    fn an_alert_in_the_users_app_reaches_the_dialog_untouched() {
        let got = RefCell::new(None);
        let a = alert_of("Title", "Body");
        let v = alert_with(
            Mode::User,
            a.clone(),
            |_| panic!("nothing to record"),
            |a| *got.borrow_mut() = Some(a),
        );
        assert_eq!(v, Verdict::Allowed);
        assert_eq!(got.into_inner(), Some(a));
    }

    #[test]
    fn a_launch_in_an_agent_instance_says_it_did_not_run_and_the_user_s_launch_goes_ahead() {
        let lines = RefCell::new(Vec::new());
        let e = launch_with(Mode::Agent, "open URL", "https://example.com", |l| {
            lines.borrow_mut().push(l)
        })
        .unwrap_err();
        assert!(
            e.contains("not started") && e.contains("https://example.com"),
            "{e}"
        );
        assert_eq!(lines.borrow().len(), 1);
        assert_eq!(
            launch_with(Mode::User, "open URL", "https://example.com", |_| panic!()),
            Ok(())
        );
    }

    #[test]
    fn the_record_names_what_was_held_back_without_dangling_separators() {
        let (_, lines) = recorded(Mode::Agent, Kind::Picker, "Add project", "");
        assert!(lines[0].ends_with("Add project"), "{}", lines[0]);
    }

    #[test]
    fn the_options_the_pages_send_deserialize_with_their_defaults() {
        // camelCase keys, and a bare `{}` is a plain file picker with no title.
        let o: PathDialog = serde_json::from_value(serde_json::json!({
            "title": "Add project",
            "directory": true,
            "defaultPath": "/tmp",
            "filters": [{"name": "Theme", "extensions": ["json"]}],
        }))
        .unwrap();
        assert!(o.directory);
        assert_eq!(o.default_path.as_deref(), Some("/tmp"));
        assert_eq!(o.filters[0].extensions, ["json"]);
        let bare: PathDialog = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(!bare.directory && bare.filters.is_empty() && bare.title.is_none());
    }
}
